#![forbid(unsafe_code)]

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Supported chat platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
	Twitch,
	Kick,
	YouTube,
}

impl Platform {
	/// Stable string identifier.
	pub const fn as_str(self) -> &'static str {
		match self {
			Platform::Twitch => "twitch",
			Platform::Kick => "kick",
			Platform::YouTube => "youtube",
		}
	}
}

impl fmt::Display for Platform {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(self.as_str())
	}
}

/// Errors for parsing identifiers from strings.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ParseIdError {
	#[error("empty value")]
	Empty,
	#[error("unknown platform: {0}")]
	UnknownPlatform(String),
	#[error("invalid format: {0}")]
	InvalidFormat(String),
}

impl FromStr for Platform {
	type Err = ParseIdError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let s = s.trim();
		if s.is_empty() {
			return Err(ParseIdError::Empty);
		}

		match s.to_ascii_lowercase().as_str() {
			"twitch" => Ok(Platform::Twitch),
			"kick" => Ok(Platform::Kick),
			"youtube" | "you_tube" | "yt" => Ok(Platform::YouTube),
			other => Err(ParseIdError::UnknownPlatform(other.to_string())),
		}
	}
}

/// Platform-specific room (channel) identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RoomId(String);

impl RoomId {
	/// Create a non-empty `RoomId`.
	pub fn new(id: impl Into<String>) -> Result<Self, ParseIdError> {
		let id = id.into();
		if id.trim().is_empty() {
			return Err(ParseIdError::Empty);
		}
		Ok(Self(id))
	}
	pub fn as_str(&self) -> &str {
		&self.0
	}
	pub fn into_string(self) -> String {
		self.0
	}
}

impl fmt::Display for RoomId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.0)
	}
}

impl FromStr for RoomId {
	type Err = ParseIdError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		RoomId::new(s.to_string())
	}
}

/// Unique room key: `(platform, room_id)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoomKey {
	pub platform: Platform,
	pub room_id: RoomId,
}

impl RoomKey {
	/// Construct a `RoomKey`.
	pub fn new(platform: Platform, room_id: RoomId) -> Self {
		Self { platform, room_id }
	}

	/// Parse a `platform:room_id` string.
	pub fn parse(s: &str) -> Result<Self, ParseIdError> {
		let s = s.trim();
		if s.is_empty() {
			return Err(ParseIdError::Empty);
		}

		let (platform_s, room_s) = s
			.split_once(':')
			.ok_or_else(|| ParseIdError::InvalidFormat("expected platform:room_id".into()))?;

		let platform = Platform::from_str(platform_s)?;
		let room_id = RoomId::new(room_s.to_string())?;
		Ok(Self::new(platform, room_id))
	}
}

impl fmt::Display for RoomKey {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}:{}", self.platform, self.room_id)
	}
}

impl FromStr for RoomKey {
	type Err = ParseIdError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		RoomKey::parse(s)
	}
}

/// topic helpers for room subscriptions.
pub struct RoomTopic;

impl RoomTopic {
	/// Prefix for room topics.
	pub const PREFIX: &'static str = "room:";

	/// Format a room topic (e.g. `room:twitch/demo`).
	pub fn format(room: &RoomKey) -> String {
		format!("{}{}{}{}", Self::PREFIX, room.platform.as_str(), '/', room.room_id.as_str())
	}

	/// Parse a room topic of the form `room:<platform>/<room>`.
	pub fn parse(s: &str) -> Result<RoomKey, ParseIdError> {
		let s = s.trim();
		if s.is_empty() {
			return Err(ParseIdError::Empty);
		}

		let rest = s
			.strip_prefix(Self::PREFIX)
			.ok_or_else(|| ParseIdError::InvalidFormat("expected room:<platform>/<room>".into()))?;

		let (platform_s, room_s) = rest
			.split_once('/')
			.ok_or_else(|| ParseIdError::InvalidFormat("expected room:<platform>/<room>".into()))?;

		let platform = Platform::from_str(platform_s)?;
		let room_id = RoomId::new(room_s.to_string())?;
		Ok(RoomKey::new(platform, room_id))
	}
}

/// Server-assigned message identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ServerMessageId(pub uuid::Uuid);

impl ServerMessageId {
	/// Create a new random server message id.
	pub fn new_v4() -> Self {
		Self(uuid::Uuid::new_v4())
	}
}

impl fmt::Display for ServerMessageId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

/// Platform-native message identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlatformMessageId(String);

impl PlatformMessageId {
	/// Create a non-empty platform message id.
	pub fn new(id: impl Into<String>) -> Result<Self, ParseIdError> {
		let id = id.into();
		if id.trim().is_empty() {
			return Err(ParseIdError::Empty);
		}
		Ok(Self(id))
	}
	pub fn as_str(&self) -> &str {
		&self.0
	}
	pub fn into_string(self) -> String {
		self.0
	}
}

impl fmt::Display for PlatformMessageId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.0)
	}
}

/// Unified message identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageIds {
	pub server_id: ServerMessageId,
	pub platform_id: Option<PlatformMessageId>,
}

impl MessageIds {
	pub fn new(server_id: ServerMessageId, platform_id: Option<PlatformMessageId>) -> Self {
		Self { server_id, platform_id }
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn platform_parse_and_display() {
		assert_eq!("twitch".parse::<Platform>().unwrap(), Platform::Twitch);
		assert_eq!("YT".parse::<Platform>().unwrap(), Platform::YouTube);
		assert_eq!(Platform::Kick.to_string(), "kick");
	}

	#[test]
	fn room_key_parse_roundtrip() {
		let rk = RoomKey::parse("twitch:shroud").unwrap();
		assert_eq!(rk.platform, Platform::Twitch);
		assert_eq!(rk.room_id.as_str(), "shroud");
		assert_eq!(rk.to_string(), "twitch:shroud");
	}

	#[test]
	fn room_topic_parse_roundtrip() {
		let room = RoomTopic::parse("room:twitch/shroud").unwrap();
		assert_eq!(room.platform, Platform::Twitch);
		assert_eq!(room.room_id.as_str(), "shroud");
		assert_eq!(RoomTopic::format(&room), "room:twitch/shroud");
	}

	#[test]
	fn rejects_empty_ids() {
		assert!(RoomId::new("").is_err());
		assert!(PlatformMessageId::new("   ").is_err());
		assert!("".parse::<RoomKey>().is_err());
	}
}
