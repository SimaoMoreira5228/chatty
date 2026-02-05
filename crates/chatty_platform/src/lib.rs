#![forbid(unsafe_code)]

pub mod assets;
pub mod kick;
pub mod twitch;

use std::collections::BTreeMap;
use std::fmt;
use std::time::{Duration, SystemTime};

use anyhow::anyhow;
use chatty_domain::{MessageIds, Platform, PlatformMessageId, RoomId, RoomKey};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

/// Server → adapter control message.
#[derive(Debug)]
pub enum AdapterControl {
	/// Begin ingesting a room.
	Join {
		room: RoomKey,
	},

	/// Stop ingesting a room.
	Leave {
		room: RoomKey,
	},

	/// Update adapter credentials.
	UpdateAuth {
		auth: AdapterAuth,
	},

	/// Execute a platform command (send/moderation).
	Command {
		request: CommandRequest,
		auth: Option<AdapterAuth>,
		resp: oneshot::Sender<Result<(), CommandError>>,
	},

	/// Query permission snapshot for a room.
	QueryPermissions {
		room: RoomKey,
		auth: Option<AdapterAuth>,
		resp: oneshot::Sender<PermissionsInfo>,
	},

	/// Query current adapter auth snapshot (best-effort).
	QueryAuth {
		resp: oneshot::Sender<Option<AdapterAuth>>,
	},

	/// Request a graceful shutdown.
	Shutdown,
}

/// Platform-agnostic command request.
#[derive(Debug, Clone)]
pub enum CommandRequest {
	SendChat {
		room: RoomKey,
		text: String,
		reply_to_platform_message_id: Option<String>,
	},
	DeleteMessage {
		room: RoomKey,
		platform_message_id: String,
	},
	TimeoutUser {
		room: RoomKey,
		user_id: String,
		duration_seconds: u32,
		reason: Option<String>,
	},
	BanUser {
		room: RoomKey,
		user_id: String,
		reason: Option<String>,
	},
}

/// Permission snapshot for a room.
#[derive(Debug, Clone, Copy, Default)]
pub struct PermissionsInfo {
	pub can_send: bool,
	pub can_reply: bool,
	pub can_delete: bool,
	pub can_timeout: bool,
	pub can_ban: bool,
	pub is_moderator: bool,
	pub is_broadcaster: bool,
}

impl CommandRequest {
	pub fn platform(&self) -> Platform {
		match self {
			Self::SendChat { room, .. }
			| Self::DeleteMessage { room, .. }
			| Self::TimeoutUser { room, .. }
			| Self::BanUser { room, .. } => room.platform,
		}
	}
}

/// Command execution errors.
#[derive(Debug, Clone)]
pub enum CommandError {
	NotSupported(Option<String>),
	NotAuthorized(Option<String>),
	InvalidTopic(Option<String>),
	InvalidCommand(Option<String>),
	Internal(String),
}

impl fmt::Display for CommandError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::NotSupported(Some(detail)) => write!(f, "not supported: {detail}"),
			Self::NotSupported(None) => f.write_str("not supported"),
			Self::NotAuthorized(Some(detail)) => write!(f, "not authorized: {detail}"),
			Self::NotAuthorized(None) => f.write_str("not authorized"),
			Self::InvalidTopic(Some(detail)) => write!(f, "invalid topic: {detail}"),
			Self::InvalidTopic(None) => f.write_str("invalid topic"),
			Self::InvalidCommand(Some(detail)) => write!(f, "invalid command: {detail}"),
			Self::InvalidCommand(None) => f.write_str("invalid command"),
			Self::Internal(msg) => write!(f, "internal error: {msg}"),
		}
	}
}

/// Adapter authentication data.
#[derive(Debug, Clone)]
pub enum AdapterAuth {
	/// No auth.
	None,

	/// OAuth-style bearer token.
	UserAccessToken {
		access_token: SecretString,
		refresh_token: Option<SecretString>,
		user_id: Option<String>,
		expires_in: Option<Duration>,
	},

	/// Twitch user OAuth payload.
	TwitchUser {
		client_id: String,
		access_token: SecretString,
		refresh_token: Option<SecretString>,
		user_id: Option<String>,
		username: Option<String>,
		expires_in: Option<Duration>,
	},

	/// Application token (client credentials).
	AppAccessToken {
		access_token: SecretString,
		expires_in: Option<Duration>,
	},

	/// Opaque platform-specific auth payload.
	OpaqueJson(serde_json::Value),
}

/// Wrapper that redacts in logs.
#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
	pub fn new(s: impl Into<String>) -> Self {
		Self(s.into())
	}

	/// Access the inner secret string.
	pub fn expose(&self) -> &str {
		&self.0
	}
}

impl fmt::Debug for SecretString {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str("SecretString(<redacted>)")
	}
}

impl fmt::Display for SecretString {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str("<redacted>")
	}
}

impl serde::Serialize for SecretString {
	fn serialize<S>(&self, serializer: S) -> Result<<S as serde::Serializer>::Ok, <S as serde::Serializer>::Error>
	where
		S: serde::Serializer,
	{
		serializer.serialize_str("")
	}
}

impl<'de> serde::Deserialize<'de> for SecretString {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		let s = String::deserialize(deserializer)?;
		Ok(SecretString::new(s))
	}
}

/// Adapter → server event message.
#[derive(Debug, Clone)]
pub enum AdapterEvent {
	/// Normalized ingest event.
	Ingest(Box<IngestEvent>),

	/// Adapter status update.
	Status(AdapterStatus),
}

/// Platform-agnostic ingest envelope.
#[derive(Debug, Clone)]
pub struct IngestEvent {
	pub platform: Platform,

	pub room: RoomKey,

	/// Server receipt timestamp (not for ordering).
	pub ingest_time: SystemTime,

	pub platform_time: Option<SystemTime>,

	/// Platform-native message id, if provided.
	pub platform_message_id: Option<PlatformMessageId>,

	pub trace: IngestTrace,

	pub payload: IngestPayload,
}

impl IngestEvent {
	/// Construct with `platform` and `room_id`.
	pub fn new(platform: Platform, room_id: RoomId, payload: IngestPayload) -> Self {
		let room = RoomKey::new(platform, room_id);
		Self {
			platform,
			room,
			ingest_time: SystemTime::now(),
			platform_time: None,
			platform_message_id: None,
			trace: IngestTrace::default(),
			payload,
		}
	}
}

/// Adapter-local trace metadata.
#[derive(Debug, Clone, Default)]
pub struct IngestTrace {
	/// Adapter session/connection identifier.
	pub session_id: Option<String>,

	pub local_seq: Option<u64>,

	/// Extra key/value metadata (avoid secrets).
	pub fields: BTreeMap<String, String>,
}

/// Normalized ingest payloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IngestPayload {
	ChatMessage(ChatMessage),

	/// Asset bundle updates (emotes/badges).
	AssetBundle(AssetBundle),

	/// Platform/system notices.
	UserNotice(UserNotice),

	/// Moderation/system events.
	Moderation(Box<ModerationEvent>),

	/// Room state changes.
	RoomState(RoomState),
}

/// Asset provider identifiers.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AssetProvider {
	Twitch,
	Kick,
	SevenTv,
	Ffz,
	Bttv,
}

/// Asset scope for a bundle.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AssetScope {
	Global,
	Channel,
}

/// Asset image scale (1x..4x).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AssetScale {
	One,
	Two,
	Three,
	Four,
}

impl AssetScale {
	pub fn as_u8(self) -> u8 {
		match self {
			Self::One => 1,
			Self::Two => 2,
			Self::Three => 3,
			Self::Four => 4,
		}
	}
}

/// Asset image with scale metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetImage {
	pub scale: AssetScale,
	pub url: String,
	pub format: String,
	pub width: u32,
	pub height: u32,
}

/// Normalized asset reference (emote/badge).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRef {
	pub id: String,
	pub name: String,
	pub images: Vec<AssetImage>,
}

/// Asset bundle payload for a room or global scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetBundle {
	pub provider: AssetProvider,
	pub scope: AssetScope,
	pub cache_key: String,
	pub etag: Option<String>,
	pub emotes: Vec<AssetRef>,
	pub badges: Vec<AssetRef>,
}

/// Normalized chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
	/// Message ids.
	pub ids: IngestMessageIds,

	pub author: UserRef,

	pub text: String,

	/// Reply preview when available.
	pub reply: Option<ChatReply>,

	/// Provider-specific badge ids attached to the author.
	pub badges: Vec<String>,

	/// Provider-specific emotes present in the message.
	pub emotes: Vec<AssetRef>,
}

/// Reply preview metadata (platform-provided).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatReply {
	pub server_message_id: Option<String>,
	pub platform_message_id: Option<String>,
	pub user_id: Option<String>,
	pub user_login: String,
	pub user_display: Option<String>,
	pub message: String,
}

impl ChatMessage {
	pub fn new(author: UserRef, text: impl Into<String>) -> Self {
		Self {
			ids: IngestMessageIds {
				server_id: Uuid::new_v4(),
				platform_id: None,
			},
			author,
			text: text.into(),
			reply: None,
			badges: Vec::new(),
			emotes: Vec::new(),
		}
	}
}

/// Serde-friendly message ids for adapter ingest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestMessageIds {
	pub server_id: Uuid,
	pub platform_id: Option<String>,
}

impl From<MessageIds> for IngestMessageIds {
	fn from(v: MessageIds) -> Self {
		Self {
			server_id: v.server_id.0,
			platform_id: v.platform_id.map(|p| p.into_string()),
		}
	}
}

/// Platform user reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRef {
	pub id: String,
	pub login: String,
	pub display: Option<String>,
}

/// Normalized user notice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserNotice {
	pub kind: String,
	pub text: Option<String>,
	pub user: Option<UserRef>,
}

/// Normalized moderation event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModerationEvent {
	/// Platform action kind.
	pub kind: String,

	#[serde(default)]
	pub actor: Option<UserRef>,

	#[serde(default)]
	pub target: Option<UserRef>,

	#[serde(default)]
	pub target_message_platform_id: Option<String>,

	#[serde(default)]
	pub notes: Option<String>,

	#[serde(default)]
	pub action: Option<ModerationAction>,
}

/// Structured moderation action payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModerationAction {
	Timeout {
		#[serde(default)]
		duration_seconds: Option<u64>,
		#[serde(default)]
		expires_at: Option<SystemTime>,
		#[serde(default)]
		reason: Option<String>,
	},
	Untimeout {},
	Ban {
		#[serde(default)]
		is_permanent: Option<bool>,
		#[serde(default)]
		reason: Option<String>,
	},
	Unban {},

	DeleteMessage {
		message_id: String,
	},
	ClearChat {},
	ClearUserMessages {
		user: UserRef,
	},

	/// An automod system held a message for review / blocked it.
	AutoModHold {
		message_id: Option<String>,
		user: Option<UserRef>,
		#[serde(default)]
		reason: Option<String>,
	},
	/// An automod message's status was updated (allowed/denied/etc).
	AutoModUpdate {
		message_id: Option<String>,
		user: Option<UserRef>,
		#[serde(default)]
		status: Option<String>,
	},
	AutoModTermsUpdate {
		#[serde(default)]
		action: Option<String>,
		#[serde(default)]
		terms: Option<Vec<String>>,
	},

	ShieldModeBegin {
		#[serde(default)]
		started_at: Option<SystemTime>,
	},
	ShieldModeEnd {
		#[serde(default)]
		ended_at: Option<SystemTime>,
		#[serde(default)]
		duration_seconds: Option<u64>,
	},

	ModeratorAdd {
		user: UserRef,
	},
	ModeratorRemove {
		user: UserRef,
	},
	VipAdd {
		user: UserRef,
	},
	VipRemove {
		user: UserRef,
	},

	UnbanRequestCreate {
		request_id: Option<String>,
		user: UserRef,
		#[serde(default)]
		text: Option<String>,
	},
	UnbanRequestResolve {
		request_id: Option<String>,
		user: UserRef,
		#[serde(default)]
		resolution: Option<String>,
		#[serde(default)]
		resolved_by: Option<UserRef>,
		#[serde(default)]
		resolved_at: Option<SystemTime>,
	},
}

/// Normalized room state event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomState {
	#[serde(default)]
	pub flags: BTreeMap<String, String>,

	#[serde(default)]
	pub settings: RoomChatSettings,

	#[serde(default)]
	pub actor: Option<UserRef>,

	#[serde(default)]
	pub notes: Option<String>,
}

/// Structured room/chat settings snapshot or delta.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoomChatSettings {
	#[serde(default)]
	pub emote_only: Option<bool>,

	#[serde(default)]
	pub subscribers_only: Option<bool>,

	#[serde(default)]
	pub unique_chat: Option<bool>,

	#[serde(default)]
	pub slow_mode: Option<bool>,
	#[serde(default)]
	pub slow_mode_wait_time_seconds: Option<u64>,

	#[serde(default)]
	pub followers_only: Option<bool>,
	#[serde(default)]
	pub followers_only_duration_minutes: Option<u64>,
}

/// Adapter status event.
#[derive(Debug, Clone)]
pub struct AdapterStatus {
	pub platform: Platform,
	pub connected: bool,
	pub detail: String,
	pub last_error: Option<String>,
	pub time: SystemTime,
}

/// Helper types for wiring adapters.
pub type AdapterControlTx = mpsc::Sender<AdapterControl>;
pub type AdapterControlRx = mpsc::Receiver<AdapterControl>;
pub type AdapterEventTx = mpsc::Sender<AdapterEvent>;
pub type AdapterEventRx = mpsc::Receiver<AdapterEvent>;

/// Spawn wiring returned by an adapter factory.
#[derive(Debug)]
pub struct AdapterHandle {
	pub platform: Platform,
	pub control_tx: AdapterControlTx,
	pub events_rx: AdapterEventRx,
}

/// Trait representing a runnable adapter.
#[async_trait::async_trait]
pub trait PlatformAdapter: Send + Sync + 'static {
	/// Which platform this adapter implements.
	fn platform(&self) -> Platform;

	/// Run the adapter until shutdown or fatal error.
	async fn run(self: Box<Self>, control_rx: AdapterControlRx, events_tx: AdapterEventTx) -> anyhow::Result<()>;
}

/// Build a standard bounded channel pair.
pub fn bounded_adapter_channels(
	control_capacity: usize,
	events_capacity: usize,
) -> (AdapterControlTx, AdapterControlRx, AdapterEventTx, AdapterEventRx) {
	let (control_tx, control_rx) = mpsc::channel(control_capacity);
	let (events_tx, events_rx) = mpsc::channel(events_capacity);
	(control_tx, control_rx, events_tx, events_rx)
}

/// Build a status event.
pub fn status(platform: Platform, connected: bool, detail: impl Into<String>) -> AdapterEvent {
	AdapterEvent::Status(AdapterStatus {
		platform,
		connected,
		detail: detail.into(),
		last_error: None,
		time: SystemTime::now(),
	})
}

/// Build a fatal-error status event.
pub fn status_error(platform: Platform, detail: impl Into<String>, err: impl fmt::Display) -> AdapterEvent {
	AdapterEvent::Status(AdapterStatus {
		platform,
		connected: false,
		detail: detail.into(),
		last_error: Some(err.to_string()),
		time: SystemTime::now(),
	})
}

/// Generate an opaque session id.
pub fn new_session_id() -> String {
	Uuid::new_v4().to_string()
}

/// Validate basic ingest invariants.
pub fn validate_ingest_event(ev: &IngestEvent) -> anyhow::Result<()> {
	if ev.platform != ev.room.platform {
		return Err(anyhow!(
			"ingest event platform mismatch: ev.platform={} room.platform={}",
			ev.platform,
			ev.room.platform
		));
	}

	let room = ev.room.room_id.as_str().trim();
	if room.is_empty() {
		return Err(anyhow!("room_id must be non-empty"));
	}

	if let IngestPayload::ChatMessage(m) = &ev.payload {
		if m.text.trim().is_empty() {
			return Err(anyhow!("chat message text must be non-empty"));
		}
		if m.author.id.trim().is_empty() || m.author.login.trim().is_empty() {
			return Err(anyhow!("chat message author fields must be non-empty"));
		}
	}

	if let IngestPayload::AssetBundle(bundle) = &ev.payload
		&& bundle.cache_key.trim().is_empty()
	{
		return Err(anyhow!("asset bundle cache_key must be non-empty"));
	}

	Ok(())
}
