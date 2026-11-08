//! Keeps a desktop app to one running copy, and hands what a second copy was started with, such as files to open, to the first.
//!
//! Call [`ring`] early in `main` with what this copy was started with. If another copy is running, it gets the message and `ring` returns [`Ring::Delivered`], so this copy can exit. Otherwise this copy is the first, and [`Door::listen`] hears the copies started after it.
//!
//! ```no_run
//! let args: Vec<String> = std::env::args().skip(1).collect();
//! match doorbell::ring("myapp", &args) {
//!     Ok(doorbell::Ring::Delivered) => {}
//!     Ok(doorbell::Ring::First(door)) => {
//!         door.listen(|message| println!("Another copy asked to open {message:?}"));
//!         // Run the app.
//!     }
//!     Err(e) => eprintln!("Couldn't check for another copy: {e}"),
//! }
//! ```
//!
//! On Windows the copies talk over a named pipe, and on Linux and macOS over a Unix socket. Either one reaches only copies run by the same user.

mod message;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use std::io;

#[cfg(unix)]
use unix as platform;
#[cfg(windows)]
use windows as platform;

/// What [`ring`] found.
#[derive(Debug)]
pub enum Ring {
	/// No other copy is running, so this one is first. Listen at the door for the copies started after it.
	First(Door),
	/// Another copy is running, and has the message. This copy can exit.
	Delivered,
}

/// Held by the first copy. Copies started later reach it here.
///
/// Keep it, or call [`Door::listen`], for as long as the app runs. Dropping it lets the next copy to start become the first.
#[derive(Debug)]
pub struct Door(platform::Door);

impl Door {
	/// Calls `on_message` on a background thread with each message from a copy started later.
	///
	/// A message is the list of strings that copy passed to [`ring`]. An empty one usually means it was started with nothing to open, so bring the window to the front. To act on a message in a GUI, post it to the UI thread from `on_message`.
	pub fn listen(self, on_message: impl Fn(Vec<String>) + Send + 'static) {
		self.0.listen(on_message);
	}
}

/// Finds out whether another copy of the app called `name` is running, and if one is, hands it `message`.
///
/// `name` identifies the app, such as `"paperback"`. It can have letters, digits, `.`, `-` and `_`. Copies run by different users don't see each other.
///
/// # Errors
///
/// Returns an error if `name` has other characters, if `message` is over 1 MB, or if the copies couldn't reach each other, such as when another copy is running but doesn't answer within a few seconds.
pub fn ring(name: &str, message: &[impl AsRef<str>]) -> io::Result<Ring> {
	if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)) {
		return Err(io::Error::new(
			io::ErrorKind::InvalidInput,
			"the app name can only have letters, digits, '.', '-' and '_'",
		));
	}
	let encoded = message::encode(message);
	if encoded.len() > message::MAX_BYTES {
		return Err(io::Error::new(io::ErrorKind::InvalidInput, "the message is over 1 MB"));
	}
	platform::ring(name, &encoded)
}
