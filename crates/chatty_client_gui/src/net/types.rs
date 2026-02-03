use crate::app::view_models::AssetRefUi;

/// UI-level events emitted by the networking layer.
#[derive(Debug, Clone)]
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
