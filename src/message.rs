//! A message is a count of strings, then each string's length and bytes, all lengths as little-endian `u32`s.

pub const MAX_BYTES: usize = 1 << 20;

pub fn encode(parts: &[impl AsRef<str>]) -> Vec<u8> {
	let mut bytes = Vec::new();
	bytes.extend_from_slice(&u32::try_from(parts.len()).unwrap_or(u32::MAX).to_le_bytes());
	for part in parts {
		let part = part.as_ref().as_bytes();
		bytes.extend_from_slice(&u32::try_from(part.len()).unwrap_or(u32::MAX).to_le_bytes());
		bytes.extend_from_slice(part);
	}
	bytes
}

/// `None` for anything that isn't a whole, well-formed message, so a stray connection can't reach the app.
pub fn decode(mut bytes: &[u8]) -> Option<Vec<String>> {
	let count = take_u32(&mut bytes)?;
	let mut parts = Vec::new();
	for _ in 0..count {
		let len = usize::try_from(take_u32(&mut bytes)?).ok()?;
		let (part, rest) = bytes.split_at_checked(len)?;
		parts.push(String::from_utf8(part.to_vec()).ok()?);
		bytes = rest;
	}
	bytes.is_empty().then_some(parts)
}

fn take_u32(bytes: &mut &[u8]) -> Option<u32> {
	let (number, rest) = bytes.split_first_chunk::<4>()?;
	*bytes = rest;
	Some(u32::from_le_bytes(*number))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_message_survives_the_trip() {
		let parts = ["C:\\My Books\\one.epub", "", "https://example.org/two.pdf", "café"];
		assert_eq!(decode(&encode(&parts)), Some(parts.map(String::from).to_vec()));
		assert_eq!(decode(&encode(&[] as &[&str])), Some(Vec::new()));
	}

	#[test]
	fn a_broken_message_is_ignored() {
		let whole = encode(&["one", "two"]);
		assert_eq!(decode(&whole[..whole.len() - 1]), None);
		assert_eq!(decode(&[whole.as_slice(), b"x"].concat()), None);
		assert_eq!(decode(b"GET / HTTP/1.1\r\n"), None);
		assert_eq!(decode(&[1, 0, 0, 0, 2, 0, 0, 0, 0xFF, 0xFE]), None);
	}
}
