#![forbid(unsafe_code)]

use std::time::SystemTime;

use chatty_domain::RoomKey;
use smallvec::SmallVec;
use smol_str::SmolStr;

use crate::app::view_models::TokenParts;

pub fn tokenize_message_parts(text: &str) -> SmallVec<[TokenParts; 8]> {
	fn to_smol(s: &str) -> SmolStr {
		SmolStr::new(s)
	}

	let is_word_char = |ch: char| ch.is_alphanumeric() || ch == '_';
	text.split_whitespace()
		.map(|token| {
			let trimmed = token.trim_matches(|ch: char| ch.is_ascii_punctuation());
			if !trimmed.is_empty() && trimmed != token {
				if let Some(start) = token.find(trimmed) {
					let end = start + trimmed.len();
					let prefix = &token[..start];
					let suffix = &token[end..];
					TokenParts {
						token: to_smol(token),
						prefix: to_smol(prefix),
						core: to_smol(trimmed),
						suffix: to_smol(suffix),
						has_word: true,
					}
				} else {
					TokenParts {
						token: to_smol(token),
						prefix: SmolStr::new(""),
						core: to_smol(token),
						suffix: SmolStr::new(""),
						has_word: false,
					}
				}
			} else {
				let mut start = None;
				let mut end = None;
				for (idx, ch) in token.char_indices() {
					if is_word_char(ch) {
						start = Some(idx);
						break;
					}
				}
				for (idx, ch) in token.char_indices().rev() {
					if is_word_char(ch) {
						end = Some(idx + ch.len_utf8());
						break;
					}
				}
				if let (Some(start), Some(end)) = (start, end) {
					let core = &token[start..end];
					let prefix = &token[..start];
					let suffix = &token[end..];
					TokenParts {
						token: to_smol(token),
						prefix: to_smol(prefix),
						core: to_smol(core),
						suffix: to_smol(suffix),
						has_word: true,
					}
				} else {
					TokenParts {
						token: to_smol(token),
						prefix: SmolStr::new(""),
						core: to_smol(token),
						suffix: SmolStr::new(""),
						has_word: false,
					}
				}
			}
		})
		.collect()
}

pub fn build_message_key(
	room: &RoomKey,
	server_message_id: Option<&str>,
	platform_message_id: Option<&str>,
	time: SystemTime,
) -> String {
	let time = time.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
	format!(
		"{}:{}:{}:{}",
		room,
		server_message_id.unwrap_or(""),
		platform_message_id.unwrap_or(""),
		time
	)
}

#[cfg(test)]
mod tests {
	use std::time::Duration;

	use super::*;

	#[test]
	fn tokenize_message_parts_splits_whitespace() {
		let tokens = tokenize_message_parts("hello   world\nchatty");
		assert_eq!(tokens.len(), 3);
		assert_eq!(tokens[0].core.as_str(), "hello");
		assert_eq!(tokens[1].core.as_str(), "world");
		assert_eq!(tokens[2].core.as_str(), "chatty");
	}

	#[test]
	fn build_message_key_includes_ids_and_time() {
		let room = RoomKey::parse("twitch:abc").expect("room key");
		let time = std::time::UNIX_EPOCH + Duration::from_millis(123);
		let key = build_message_key(&room, Some("srv"), Some("plat"), time);
		assert_eq!(key, "twitch:abc:srv:plat:123");
	}
}
