//! A named pipe, which only the first copy can create.
//!
//! The pipe's default security lets only the same user, and administrators, write to it, and it refuses connections from other computers. Its name includes the user's name, so two users' copies don't meet.

use std::{env, ffi::OsStr, io, iter, os::windows::ffi::OsStrExt as _, thread};

use windows::{
	Win32::{
		Foundation::{
			CloseHandle, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED,
			GENERIC_WRITE, GetLastError, HANDLE, WIN32_ERROR,
		},
		Storage::FileSystem::{
			CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_SHARE_NONE, FlushFileBuffers,
			OPEN_EXISTING, PIPE_ACCESS_INBOUND, ReadFile, WriteFile,
		},
		System::Pipes::{
			ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
			PIPE_TYPE_BYTE, PIPE_WAIT, WaitNamedPipeW,
		},
		UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow},
	},
	core::PCWSTR,
};

use crate::{Ring, message};

const BUFFER: u32 = 64 * 1024;
/// How long a second copy waits for a busy first copy, in milliseconds.
const WAIT_MS: u32 = 5000;

#[derive(Debug)]
pub struct Door {
	/// A `HANDLE` as a number, since `HANDLE` can't cross to the listening thread. `None` once that thread has it.
	pipe: Option<usize>,
}

impl Door {
	pub fn listen(mut self, on_message: impl Fn(Vec<String>) + Send + 'static) {
		let Some(raw) = self.pipe.take() else { return };
		thread::spawn(move || {
			let pipe = HANDLE(raw as *mut _);
			loop {
				// A copy that connected before this call is already connected, which isn't a failure.
				let connected = match unsafe { ConnectNamedPipe(pipe, None) } {
					Ok(()) => true,
					Err(error) => code(&error) == ERROR_PIPE_CONNECTED,
				};
				if connected && let Some(message) = read_message(pipe) {
					on_message(message);
				}
				let _ = unsafe { DisconnectNamedPipe(pipe) };
			}
		});
	}
}

impl Drop for Door {
	fn drop(&mut self) {
		if let Some(raw) = self.pipe {
			let _ = unsafe { CloseHandle(HANDLE(raw as *mut _)) };
		}
	}
}

/// Reads until the other copy closes its end.
fn read_message(pipe: HANDLE) -> Option<Vec<String>> {
	let mut bytes = Vec::new();
	let mut buffer = vec![0; BUFFER as usize];
	loop {
		let mut read = 0;
		if unsafe { ReadFile(pipe, Some(&mut buffer), Some(&raw mut read), None) }.is_err() || read == 0 {
			break;
		}
		bytes.extend_from_slice(&buffer[..read as usize]);
		if bytes.len() > message::MAX_BYTES {
			return None;
		}
	}
	message::decode(&bytes)
}

pub fn ring(name: &str, message: &[u8]) -> io::Result<Ring> {
	let user = env::var("USERNAME").unwrap_or_default();
	let path: Vec<u16> = OsStr::new(&format!(r"\\.\pipe\{name}-{user}")).encode_wide().chain(iter::once(0)).collect();
	let path = PCWSTR(path.as_ptr());
	// Twice more if the other copy exits between failing to create the pipe and connecting to it.
	for _ in 0..3 {
		let pipe = unsafe {
			CreateNamedPipeW(
				path,
				PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
				PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
				1,
				0,
				BUFFER,
				0,
				None,
			)
		};
		if !pipe.is_invalid() {
			return Ok(Ring::First(crate::Door(Door { pipe: Some(pipe.0 as usize) })));
		}
		let error = unsafe { GetLastError() };
		// Access denied is how the first-instance flag says another copy already has the pipe.
		if error != ERROR_ACCESS_DENIED && error != ERROR_PIPE_BUSY {
			return Err(os_error(error));
		}
		match deliver(path, message) {
			Ok(()) => return Ok(Ring::Delivered),
			Err(error) if error == ERROR_FILE_NOT_FOUND => {}
			Err(error) => return Err(os_error(error)),
		}
	}
	Err(io::Error::other("another copy kept starting and exiting"))
}

fn deliver(path: PCWSTR, message: &[u8]) -> Result<(), WIN32_ERROR> {
	// Windows only lets the process the user is using bring a window to the front, so this one passes that on to the first copy.
	let _ = unsafe { AllowSetForegroundWindow(ASFW_ANY) };
	let pipe = loop {
		match unsafe {
			CreateFileW(path, GENERIC_WRITE.0, FILE_SHARE_NONE, None, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, None)
		} {
			Ok(pipe) => break pipe,
			Err(error) if code(&error) == ERROR_PIPE_BUSY => {
				if !unsafe { WaitNamedPipeW(path, WAIT_MS) }.as_bool() {
					return Err(unsafe { GetLastError() });
				}
			}
			Err(error) => return Err(code(&error)),
		}
	};
	let mut written = 0;
	let mut result = Ok(());
	while written < message.len() {
		let mut count = 0;
		if let Err(error) = unsafe { WriteFile(pipe, Some(&message[written..]), Some(&raw mut count), None) } {
			result = Err(code(&error));
			break;
		}
		written += count as usize;
	}
	// Waits until the first copy has read it all, since closing first could lose the end of it.
	if result.is_ok()
		&& let Err(error) = unsafe { FlushFileBuffers(pipe) }
	{
		result = Err(code(&error));
	}
	let _ = unsafe { CloseHandle(pipe) };
	result
}

fn code(error: &windows::core::Error) -> WIN32_ERROR {
	WIN32_ERROR::from_error(error).unwrap_or_else(|| WIN32_ERROR(error.code().0.cast_unsigned()))
}

fn os_error(error: WIN32_ERROR) -> io::Error {
	io::Error::from_raw_os_error(error.0.cast_signed())
}
