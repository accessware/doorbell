//! A lock file, which only the first copy can hold, and a Unix socket beside it.
//!
//! Both live in a folder only the user can open, so other users can't reach the socket. The lock ends with the process, so a copy that crashed never blocks the next one, and the socket it left behind is replaced.

use std::{
	env,
	fs::{self, DirBuilder, File, OpenOptions, TryLockError},
	io::{self, Read, Write},
	os::unix::{
		fs::{DirBuilderExt as _, PermissionsExt as _},
		net::{UnixListener, UnixStream},
	},
	path::{Path, PathBuf},
	thread,
	time::Duration,
};

use crate::{Ring, message};

/// How long a second copy waits for the first to start listening, which it does just after taking the lock.
const CONNECT_ATTEMPTS: u32 = 50;
const CONNECT_INTERVAL: Duration = Duration::from_millis(100);
/// So a copy that connects and never writes can't hold up the ones after it.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub struct Door {
	lock: File,
	listener: UnixListener,
}

impl Door {
	pub fn listen(self, on_message: impl Fn(Vec<String>) + Send + 'static) {
		thread::spawn(move || {
			// Held by this thread, which lives as long as the process, so the lock does too.
			let Self { lock: _lock, listener } = self;
			for stream in listener.incoming().flatten() {
				// macOS refuses this once the other copy has closed its end, which a quick one already has, and then the read can't hang anyway.
				let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
				let mut bytes = Vec::new();
				let limit = u64::try_from(message::MAX_BYTES).unwrap_or(u64::MAX) + 1;
				if stream.take(limit).read_to_end(&mut bytes).is_ok()
					&& let Some(message) = message::decode(&bytes)
				{
					on_message(message);
				}
			}
		});
	}
}

pub fn ring(name: &str, message: &[u8]) -> io::Result<Ring> {
	let dir = folder()?;
	let socket = dir.join(format!("{name}.sock"));
	let lock = OpenOptions::new().write(true).create(true).truncate(false).open(dir.join(format!("{name}.lock")))?;
	match lock.try_lock() {
		Ok(()) => {
			// The lock was free, so no copy is running, and a socket left by one that crashed can go.
			let _ = fs::remove_file(&socket);
			let listener = UnixListener::bind(&socket)?;
			Ok(Ring::First(crate::Door(Door { lock, listener })))
		}
		Err(TryLockError::WouldBlock) => {
			deliver(&socket, message)?;
			Ok(Ring::Delivered)
		}
		Err(TryLockError::Error(error)) => Err(error),
	}
}

fn deliver(socket: &Path, message: &[u8]) -> io::Result<()> {
	let mut attempts = 0;
	let mut stream = loop {
		match UnixStream::connect(socket) {
			Ok(stream) => break stream,
			Err(_) if attempts < CONNECT_ATTEMPTS => {
				attempts += 1;
				thread::sleep(CONNECT_INTERVAL);
			}
			Err(error) => return Err(error),
		}
	};
	stream.write_all(message)
}

/// `$XDG_RUNTIME_DIR` where there is one, which only the user can open. Elsewhere, such as on macOS, a folder of our own in the temp folder, which on macOS is already the user's own.
fn folder() -> io::Result<PathBuf> {
	if let Some(dir) = env::var_os("XDG_RUNTIME_DIR").filter(|dir| !dir.is_empty()) {
		return Ok(PathBuf::from(dir));
	}
	let user = env::var("USER").unwrap_or_default();
	let dir = env::temp_dir().join(format!("doorbell-{user}"));
	DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
	// Someone else could have made it first in a shared temp folder. If they did, either it isn't private, or it's theirs and creating the lock in it fails.
	if fs::metadata(&dir)?.permissions().mode() & 0o077 != 0 {
		return Err(io::Error::new(
			io::ErrorKind::PermissionDenied,
			format!("{} can be opened by other users", dir.display()),
		));
	}
	Ok(dir)
}
