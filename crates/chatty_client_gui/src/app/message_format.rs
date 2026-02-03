#![forbid(unsafe_code)]

use std::time::SystemTime;

use chatty_domain::RoomKey;

pub fn tokenize_message_text(text: &str) -> Vec<String> {
	text.split_whitespace().map(|t| t.to_string()).collect()
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
	use super::*;
	use std::time::Duration;

	#[test]
	fn tokenize_message_text_splits_whitespace() {
		let tokens = tokenize_message_text("hello   world\nchatty");
		assert_eq!(tokens, vec!["hello", "world", "chatty"]);
	}

	#[test]
	fn build_message_key_includes_ids_and_time() {
		let room = RoomKey::parse("twitch:abc").expect("room key");
		let time = std::time::UNIX_EPOCH + Duration::from_millis(123);
		let key = build_message_key(&room, Some("srv"), Some("plat"), time);
		assert_eq!(key, "twitch:abc:srv:plat:123");
	}
}
