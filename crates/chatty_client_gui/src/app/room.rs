#![forbid(unsafe_code)]

use std::str::FromStr;

use chatty_domain::{Platform, RoomId, RoomKey, RoomTopic};

use crate::settings;

#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
pub struct RoomPermissions {
	pub can_send: bool,
	pub can_reply: bool,
	pub can_delete: bool,
	pub can_timeout: bool,
	pub can_ban: bool,
	pub is_moderator: bool,
	pub is_broadcaster: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RoomStateUi {
	pub emote_only: Option<bool>,
	pub subscribers_only: Option<bool>,
	pub unique_chat: Option<bool>,
	pub slow_mode: Option<bool>,
	pub slow_mode_wait_time_seconds: Option<u64>,
	pub followers_only: Option<bool>,
	pub followers_only_duration_minutes: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct JoinRequest {
	pub raw: String,
}

impl JoinRequest {
	pub fn parse_rooms(&self) -> Vec<RoomKey> {
		let s = self.raw.trim();
		if s.is_empty() {
			return Vec::new();
		}

		let mut rooms = Vec::new();
		for part in s.split(',') {
			let part = part.trim();
			if part.is_empty() {
				continue;
			}

			if part.starts_with(RoomTopic::PREFIX) {
				if let Ok(room) = RoomTopic::parse(part) {
					rooms.push(room);
				}
				continue;
			}

			if let Some((platform_s, room_s)) = part.split_once(':')
				&& let (Ok(platform), Ok(room_id)) = (Platform::from_str(platform_s), RoomId::new(room_s.to_string()))
			{
				rooms.push(RoomKey::new(platform, room_id));
				continue;
			}

			if let Ok(room_id) = RoomId::new(part.to_string()) {
				let default_platform = settings::get_cloned().default_platform;
				rooms.push(RoomKey::new(default_platform, room_id));
			}
		}
		rooms
	}

	pub fn parse_first(&self) -> Option<RoomKey> {
		self.parse_rooms().into_iter().next()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_rooms_empty_returns_none() {
		let req = JoinRequest { raw: "  ".to_string() };
		assert!(req.parse_rooms().is_empty());
		assert!(req.parse_first().is_none());
	}

	#[test]
	fn parse_rooms_explicit_platforms() {
		let req = JoinRequest {
			raw: "twitch:alpha, kick:beta".to_string(),
		};
		let rooms = req.parse_rooms();
		assert_eq!(rooms.len(), 2);
		assert!(
			rooms
				.iter()
				.any(|r| r.platform == Platform::Twitch && r.room_id.as_str() == "alpha")
		);
		assert!(
			rooms
				.iter()
				.any(|r| r.platform == Platform::Kick && r.room_id.as_str() == "beta")
		);
	}

	#[test]
	fn parse_rooms_topic_prefix() {
		let room = RoomKey::new(Platform::Twitch, RoomId::new("streamer").expect("room id"));
		let topic = RoomTopic::format(&room);
		let req = JoinRequest { raw: topic };
		let rooms = req.parse_rooms();
		assert_eq!(rooms, vec![room]);
	}

	#[test]
	fn parse_rooms_default_platform() {
		let req = JoinRequest {
			raw: "lonely".to_string(),
		};
		let rooms = req.parse_rooms();
		assert_eq!(rooms.len(), 1);
		let default_platform = settings::get_cloned().default_platform;
		assert_eq!(rooms[0].platform, default_platform);
		assert_eq!(rooms[0].room_id.as_str(), "lonely");
	}
}
