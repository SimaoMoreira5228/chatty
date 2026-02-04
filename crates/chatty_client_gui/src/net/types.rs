use core::fmt;

use crate::app::view_models::{AssetRefUi, ChatReplyUi};

/// UI-level events emitted by the networking layer.
#[derive(Clone)]
pub enum UiEvent {
	Connecting,
	Reconnecting {
		attempt: u32,
		next_retry_in_ms: u64,
	},
	Connected {
		server_name: String,
		server_instance_id: String,
	},
	Disconnected {
		reason: String,
	},
	ErrorWithServer {
		message: String,
		server: Option<String>,
	},
	ChatMessage {
		topic: String,
		author_login: String,
		author_display: Option<String>,
		author_id: Option<String>,
		text: String,
		server_message_id: Option<String>,
		platform_message_id: Option<String>,
		badge_ids: Vec<String>,
		emotes: Vec<AssetRefUi>,
		reply: Option<ChatReplyUi>,
	},
	RoomPermissions {
		topic: String,
		can_send: bool,
		can_reply: bool,
		can_delete: bool,
		can_timeout: bool,
		can_ban: bool,
		is_moderator: bool,
		is_broadcaster: bool,
	},
	RoomState {
		topic: String,
		emote_only: Option<bool>,
		subscribers_only: Option<bool>,
		unique_chat: Option<bool>,
		slow_mode: Option<bool>,
		slow_mode_wait_time_seconds: Option<u64>,
		followers_only: Option<bool>,
		followers_only_duration_minutes: Option<u64>,
	},
	AssetBundle {
		topic: String,
		cache_key: String,
		etag: Option<String>,
		provider: i32,
		scope: i32,
		emotes: Vec<AssetRefUi>,
		badges: Vec<AssetRefUi>,
	},
	CommandResult {
		status: i32,
		detail: String,
	},
}

impl fmt::Debug for UiEvent {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			UiEvent::Connecting => write!(f, "UiEvent::Connecting"),
			UiEvent::Reconnecting {
				attempt,
				next_retry_in_ms,
			} => {
				write!(
					f,
					"UiEvent::Reconnecting {{ attempt: {}, next_retry_in_ms: {} }}",
					attempt, next_retry_in_ms
				)
			}
			UiEvent::Connected {
				server_name,
				server_instance_id,
			} => {
				write!(
					f,
					"UiEvent::Connected {{ server_name: {}, server_instance_id: {} }}",
					server_name, server_instance_id
				)
			}
			UiEvent::Disconnected { reason } => {
				write!(f, "UiEvent::Disconnected {{ reason: {} }}", reason)
			}
			UiEvent::ErrorWithServer { message, server } => {
				write!(f, "UiEvent::ErrorWithServer {{ message: {}, server: {:?} }}", message, server)
			}
			UiEvent::ChatMessage {
				topic,
				author_login,
				server_message_id,
				platform_message_id,
				..
			} => {
				write!(
					f,
					"UiEvent::ChatMessage {{ topic: {}, author_login: {}, server_message_id: {:?}, platform_message_id: {:?}, ... }}",
					topic, author_login, server_message_id, platform_message_id
				)
			}
			UiEvent::RoomPermissions {
				topic,
				can_send,
				can_reply,
				can_delete,
				can_timeout,
				can_ban,
				is_moderator,
				is_broadcaster,
			} => {
				write!(
					f,
					"UiEvent::RoomPermissions {{ topic: {}, can_send: {}, can_reply: {}, can_delete: {}, can_timeout: {}, can_ban: {}, is_moderator: {}, is_broadcaster: {} }}",
					topic, can_send, can_reply, can_delete, can_timeout, can_ban, is_moderator, is_broadcaster
				)
			}
			UiEvent::RoomState {
				topic,
				emote_only,
				subscribers_only,
				unique_chat,
				slow_mode,
				slow_mode_wait_time_seconds,
				followers_only,
				followers_only_duration_minutes,
			} => {
				write!(
					f,
					"UiEvent::RoomState {{ topic: {}, emote_only: {:?}, subscribers_only: {:?}, unique_chat: {:?}, slow_mode: {:?}, slow_mode_wait_time_seconds: {:?}, followers_only: {:?}, followers_only_duration_minutes: {:?} }}",
					topic,
					emote_only,
					subscribers_only,
					unique_chat,
					slow_mode,
					slow_mode_wait_time_seconds,
					followers_only,
					followers_only_duration_minutes
				)
			}
			UiEvent::AssetBundle { topic, cache_key, .. } => {
				write!(
					f,
					"UiEvent::AssetBundle {{ topic: {}, cache_key: {}, ... }}",
					topic, cache_key
				)
			}
			UiEvent::CommandResult { status, detail } => {
				write!(f, "UiEvent::CommandResult {{ status: {}, detail: {} }}", status, detail)
			}
		}
	}
}
