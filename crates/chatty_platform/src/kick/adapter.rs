#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use async_trait::async_trait;
use chatty_domain::{Platform, RoomKey};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::protocol::Message;
use tracing::{debug, info, warn};
use url::Url;

use super::client::KickClient;
use crate::assets::{
	DispatchType, SevenTvCacheMode, SevenTvPlatform, SevenTvSubscription, ensure_asset_cache_pruner,
	ensure_seventv_event_api, fetch_7tv_badges_bundle, fetch_7tv_bundle_with_sets, fetch_7tv_channel_badges_bundle,
	fetch_kick_badge_bundle, fetch_kick_emote_bundles,
};
use crate::{
	AdapterAuth, AdapterControl, AdapterControlRx, AdapterEvent, AdapterEventTx, AssetBundle, AssetImage, AssetProvider,
	AssetRef, AssetScale, AssetScope, ChatMessage, CommandError, CommandRequest, IngestEvent, IngestPayload,
	ModerationAction, ModerationEvent, PermissionsInfo, PlatformAdapter, SecretString, UserRef, new_session_id, status,
};

#[derive(Clone)]
pub struct KickConfig {
	pub base_url: String,
	pub broadcaster_id_overrides: HashMap<String, String>,
	pub resolve_cache_ttl: Duration,
	pub pusher_ws_url: String,
	pub reconnect_min_delay: Duration,
	pub reconnect_max_delay: Duration,
}

impl Default for KickConfig {
	fn default() -> Self {
		Self::new()
	}
}

impl KickConfig {
	pub fn new() -> Self {
		Self {
			base_url: "https://api.kick.com".to_string(),
			broadcaster_id_overrides: HashMap::new(),
			resolve_cache_ttl: Duration::from_secs(300),
			pusher_ws_url: "wss://ws-us2.pusher.com/app/32cbd69e4b950bf97679".to_string(),
			reconnect_min_delay: Duration::from_millis(500),
			reconnect_max_delay: Duration::from_secs(30),
		}
	}
}

const KICK_PUSHER_PROTOCOL: &str = "7";
const KICK_PUSHER_CLIENT: &str = "js";
const KICK_PUSHER_VERSION: &str = "8.4.0";

pub struct KickEventAdapter {
	cfg: KickConfig,
	joined_rooms: Arc<RwLock<HashSet<RoomKey>>>,
	moderator_rooms: Arc<RwLock<HashMap<u64, HashSet<RoomKey>>>>,
	auth_user_ids: Arc<RwLock<HashSet<u64>>>,
	user_scopes: Arc<RwLock<HashMap<u64, HashSet<String>>>>,
	broadcaster_id_by_room: HashMap<RoomKey, (u64, std::time::Instant)>,
	chatroom_id_by_room: HashMap<RoomKey, (u64, std::time::Instant)>,
	room_by_chatroom_id: HashMap<u64, RoomKey>,
	seventv_subscriptions: Arc<RwLock<HashMap<RoomKey, Vec<SevenTvSubscription>>>>,
	last_auth_error_notice: Option<String>,
}

impl KickEventAdapter {
	pub fn new(cfg: KickConfig) -> Self {
		Self {
			cfg,
			joined_rooms: Arc::new(RwLock::new(HashSet::new())),
			moderator_rooms: Arc::new(RwLock::new(HashMap::new())),
			auth_user_ids: Arc::new(RwLock::new(HashSet::new())),
			user_scopes: Arc::new(RwLock::new(HashMap::new())),
			broadcaster_id_by_room: HashMap::new(),
			chatroom_id_by_room: HashMap::new(),
			room_by_chatroom_id: HashMap::new(),
			seventv_subscriptions: Arc::new(RwLock::new(HashMap::new())),
			last_auth_error_notice: None,
		}
	}

	fn platform(&self) -> Platform {
		Platform::Kick
	}

	fn maybe_notice_auth_issue(&mut self, reason: &str, events_tx: &AdapterEventTx) {
		if self.last_auth_error_notice.as_deref() != Some(reason) {
			self.last_auth_error_notice = Some(reason.to_string());
			let _ = events_tx.try_send(status(self.platform(), false, reason.to_string()));
		}
	}

	fn client_for_token(&self, token: &SecretString) -> KickClient {
		KickClient::new(self.cfg.base_url.clone(), token.expose().to_string())
	}

	fn pusher_url(&self) -> anyhow::Result<Url> {
		let mut url = Url::parse(&self.cfg.pusher_ws_url).context("parse kick pusher ws url")?;
		url.query_pairs_mut()
			.append_pair("protocol", KICK_PUSHER_PROTOCOL)
			.append_pair("client", KICK_PUSHER_CLIENT)
			.append_pair("version", KICK_PUSHER_VERSION)
			.append_pair("flash", "false");
		Ok(url)
	}

	async fn connect_ws(&self) -> anyhow::Result<KickWs> {
		let url = self.pusher_url()?.to_string();
		let (ws, _) = tokio_tungstenite::connect_async(url).await.context("kick ws connect")?;
		Ok(ws)
	}

	async fn apply_auth_update(&mut self, auth: AdapterAuth) {
		if let AdapterAuth::UserAccessToken {
			access_token, user_id, ..
		} = auth
		{
			self.last_auth_error_notice = None;

			let parsed_user_id = user_id.as_deref().and_then(|id| id.trim().parse::<u64>().ok());
			{
				let mut guard = self.auth_user_ids.write().await;
				if let Some(uid) = parsed_user_id {
					guard.insert(uid);
				}
			}

			if let Some(uid) = parsed_user_id {
				let client = self.client_for_token(&access_token);
				match client.introspect_token(access_token.expose()).await {
					Ok(info) => {
						if let Some(scope) = info.scope {
							let scopes: HashSet<String> = scope
								.split_whitespace()
								.filter(|s| !s.trim().is_empty())
								.map(|s| s.to_string())
								.collect();
							self.user_scopes.write().await.insert(uid, scopes.clone());
							info!(user_id = uid, scopes = %scope, "kick token introspect scopes");
						}
					}
					Err(err) => {
						warn!(error = %err, user_id = uid, "kick token introspect failed");
					}
				}
			} else {
				warn!("kick auth update missing user_id; scope visibility unavailable");
			}

			self.moderator_rooms.write().await.clear();
		}
	}

	async fn resolve_broadcaster_id(&mut self, room: &RoomKey, token: &SecretString) -> Result<u64, CommandError> {
		if room.platform != Platform::Kick {
			return Err(CommandError::InvalidTopic(None));
		}
		if let Some(override_id) = self.cfg.broadcaster_id_overrides.get(room.room_id.as_str()) {
			return override_id.parse::<u64>().map_err(|_| CommandError::InvalidTopic(None));
		}

		if let Some((cached, ts)) = self.broadcaster_id_by_room.get(room)
			&& ts.elapsed() < self.cfg.resolve_cache_ttl
		{
			return Ok(*cached);
		}

		let slug = room.room_id.as_str();
		if slug.chars().all(|c| c.is_ascii_digit()) {
			let id = slug.parse::<u64>().map_err(|_| CommandError::InvalidTopic(None))?;
			self.broadcaster_id_by_room
				.insert(room.clone(), (id, std::time::Instant::now()));
			return Ok(id);
		}

		let client = self.client_for_token(token);
		let resolved = client
			.resolve_broadcaster_id(slug)
			.await
			.map_err(|e| CommandError::Internal(e.to_string()))?;

		let Some(id) = resolved else {
			return Err(CommandError::InvalidTopic(None));
		};

		self.broadcaster_id_by_room
			.insert(room.clone(), (id, std::time::Instant::now()));
		Ok(id)
	}

	async fn resolve_chatroom_id(&mut self, room: &RoomKey) -> Result<u64, CommandError> {
		if room.platform != Platform::Kick {
			return Err(CommandError::InvalidTopic(None));
		}

		if let Some((cached, ts)) = self.chatroom_id_by_room.get(room)
			&& ts.elapsed() < self.cfg.resolve_cache_ttl
		{
			return Ok(*cached);
		}

		let slug = room.room_id.as_str();
		let id = if slug.chars().all(|c| c.is_ascii_digit()) {
			slug.parse::<u64>().map_err(|_| CommandError::InvalidTopic(None))?
		} else {
			let client = KickClient::new(self.cfg.base_url.clone(), "");
			let resolved = client
				.resolve_chatroom_id(slug)
				.await
				.map_err(|e| CommandError::Internal(e.to_string()))?;
			resolved.ok_or(CommandError::InvalidTopic(None))?
		};

		self.chatroom_id_by_room.insert(room.clone(), (id, std::time::Instant::now()));
		self.room_by_chatroom_id.insert(id, room.clone());
		Ok(id)
	}

	fn command_auth(auth: Option<AdapterAuth>) -> Option<(SecretString, Option<String>)> {
		match auth {
			Some(AdapterAuth::UserAccessToken {
				access_token, user_id, ..
			}) => Some((access_token, user_id)),
			_ => None,
		}
	}

	async fn execute_command(&mut self, request: CommandRequest, auth: Option<AdapterAuth>) -> Result<(), CommandError> {
		let room = match &request {
			CommandRequest::SendChat { room, .. }
			| CommandRequest::DeleteMessage { room, .. }
			| CommandRequest::TimeoutUser { room, .. }
			| CommandRequest::BanUser { room, .. } => room,
		};

		if room.platform != Platform::Kick {
			return Err(CommandError::InvalidTopic(None));
		}

		let Some((token, auth_user_id)) = Self::command_auth(auth) else {
			return Err(CommandError::NotAuthorized(Some(
				"kick auth missing access token".to_string(),
			)));
		};
		let broadcaster_id = self.resolve_broadcaster_id(room, &token).await?;
		let chatroom_id = self.resolve_chatroom_id(room).await.ok();
		let is_broadcaster = auth_user_id
			.as_deref()
			.and_then(|id| if id.trim().is_empty() { None } else { Some(id) })
			.and_then(|id| parse_numeric_id(id).ok())
			.map(|id| id == broadcaster_id)
			.unwrap_or(false);
		let auth_user_id_num = auth_user_id
			.as_deref()
			.and_then(|id| if id.trim().is_empty() { None } else { Some(id) })
			.and_then(|id| parse_numeric_id(id).ok());
		let is_moderator = self.is_moderator_for_room(room, auth_user_id_num).await;
		let can_moderate = is_moderator || is_broadcaster;
		let client = self.client_for_token(&token);

		match request {
			CommandRequest::SendChat {
				text,
				reply_to_platform_message_id,
				..
			} => {
				let Some(chatroom_id) = chatroom_id else {
					return Err(CommandError::InvalidTopic(Some("kick chatroom id unresolved".to_string())));
				};
				let message_ref = chrono::Utc::now().timestamp_millis().to_string();
				client
					.send_chat_message(chatroom_id, &text, &message_ref, reply_to_platform_message_id.as_deref())
					.await
					.map_err(map_kick_error)?
			}
			CommandRequest::DeleteMessage { platform_message_id, .. } => {
				if !can_moderate {
					return Err(CommandError::NotAuthorized(None));
				}
				client
					.delete_chat_message(&platform_message_id)
					.await
					.map_err(map_kick_error)?
			}
			CommandRequest::TimeoutUser {
				user_id,
				duration_seconds,
				reason,
				..
			} => {
				if !can_moderate {
					return Err(CommandError::NotAuthorized(None));
				}
				let parsed_user_id = parse_numeric_id(&user_id)?;
				client
					.ban_user(broadcaster_id, parsed_user_id, Some(duration_seconds), reason.as_deref())
					.await
					.map_err(map_kick_error)?
			}
			CommandRequest::BanUser { user_id, reason, .. } => {
				if !can_moderate {
					return Err(CommandError::NotAuthorized(None));
				}
				let parsed_user_id = parse_numeric_id(&user_id)?;
				client
					.ban_user(broadcaster_id, parsed_user_id, None, reason.as_deref())
					.await
					.map_err(map_kick_error)?
			}
		};

		Ok(())
	}

	async fn permissions_for_room(&mut self, room: &RoomKey, auth: Option<AdapterAuth>) -> PermissionsInfo {
		if room.platform != Platform::Kick {
			return PermissionsInfo::default();
		}
		let Some((token, auth_user_id)) = Self::command_auth(auth) else {
			return PermissionsInfo::default();
		};
		let broadcaster_id = match self.resolve_broadcaster_id(room, &token).await {
			Ok(id) => id,
			Err(_) => return PermissionsInfo::default(),
		};
		let auth_user_id_num = auth_user_id
			.as_deref()
			.and_then(|id| if id.trim().is_empty() { None } else { Some(id) })
			.and_then(|id| parse_numeric_id(id).ok());
		let is_broadcaster = auth_user_id_num.map(|id| id == broadcaster_id).unwrap_or(false);
		let is_moderator = self.is_moderator_for_room(room, auth_user_id_num).await;
		let can_moderate = is_moderator || is_broadcaster;

		PermissionsInfo {
			can_send: true,
			can_reply: true,
			can_delete: can_moderate,
			can_timeout: can_moderate,
			can_ban: can_moderate,
			is_moderator,
			is_broadcaster,
		}
	}

	async fn is_moderator_for_room(&self, room: &RoomKey, user_id: Option<u64>) -> bool {
		let Some(uid) = user_id else {
			return false;
		};

		let guard = self.moderator_rooms.read().await;
		guard.get(&uid).map(|rooms| rooms.contains(room)).unwrap_or(false)
	}

	fn backoff_delay(attempt: u32, min: Duration, max: Duration) -> Duration {
		let min_ms = min.as_millis() as u64;
		let max_ms = max.as_millis() as u64;
		let exp = 2u64.saturating_pow(attempt.min(10));
		let delay_ms = min_ms.saturating_mul(exp).min(max_ms);
		Duration::from_millis(delay_ms)
	}

	async fn send_pusher_subscribe(
		&self,
		ws_tx: &mut futures_util::stream::SplitSink<KickWs, Message>,
		chatroom_id: u64,
	) -> anyhow::Result<()> {
		let payload = serde_json::json!({
			"event": "pusher:subscribe",
			"data": { "auth": "", "channel": format!("chatrooms.{chatroom_id}.v2") }
		});
		ws_tx
			.send(Message::Text(payload.to_string().into()))
			.await
			.context("kick ws subscribe")
	}

	async fn send_pusher_unsubscribe(
		&self,
		ws_tx: &mut futures_util::stream::SplitSink<KickWs, Message>,
		chatroom_id: u64,
	) -> anyhow::Result<()> {
		let payload = serde_json::json!({
			"event": "pusher:unsubscribe",
			"data": { "channel": format!("chatrooms.{chatroom_id}.v2") }
		});
		ws_tx
			.send(Message::Text(payload.to_string().into()))
			.await
			.context("kick ws unsubscribe")
	}

	async fn send_pusher_pong(&self, ws_tx: &mut futures_util::stream::SplitSink<KickWs, Message>) -> anyhow::Result<()> {
		let payload = serde_json::json!({
			"event": "pusher:pong",
			"data": {}
		});
		ws_tx
			.send(Message::Text(payload.to_string().into()))
			.await
			.context("kick ws pong")
	}

	async fn handle_pusher_event(&mut self, envelope: KickPusherEvent, events_tx: &AdapterEventTx) -> anyhow::Result<()> {
		match envelope.event.as_str() {
			"App\\Events\\ChatMessageEvent" => {
				if let Some(payload) = parse_pusher_payload::<KickWsChatMessage>(envelope.data) {
					self.handle_chat_message(payload, envelope.channel.as_deref(), events_tx)
						.await;
				}
			}
			"App\\Events\\MessageDeletedEvent" => {
				if let Some(payload) = parse_pusher_payload::<KickWsMessageDeleted>(envelope.data) {
					self.handle_message_deleted(payload, envelope.channel.as_deref(), events_tx)
						.await;
				}
			}
			"App\\Events\\UserBannedEvent" => {
				if let Some(payload) = parse_pusher_payload::<KickWsUserBan>(envelope.data) {
					self.handle_user_banned(payload, envelope.channel.as_deref(), events_tx).await;
				}
			}
			"App\\Events\\UserUnbannedEvent" => {
				if let Some(payload) = parse_pusher_payload::<KickWsUserUnban>(envelope.data) {
					self.handle_user_unbanned(payload, envelope.channel.as_deref(), events_tx)
						.await;
				}
			}
			_ => {}
		}
		Ok(())
	}

	async fn handle_chat_message(&mut self, payload: KickWsChatMessage, channel: Option<&str>, events_tx: &AdapterEventTx) {
		let room = self.room_by_chatroom_id.get(&payload.chatroom_id).cloned().or_else(|| {
			channel
				.and_then(chatroom_id_from_channel)
				.and_then(|id| self.room_by_chatroom_id.get(&id).cloned())
		});
		let Some(room) = room else {
			warn!(chatroom_id = payload.chatroom_id, channel = ?channel, "kick ws message for unknown room");
			return;
		};

		if self.auth_user_ids.read().await.contains(&payload.sender.id) {
			let has_mod_badge = payload
				.sender
				.identity
				.as_ref()
				.map(|identity| {
					identity
						.badges
						.iter()
						.any(|badge| matches!(badge.badge_type.as_str(), "moderator" | "broadcaster"))
				})
				.unwrap_or(false);
			let mut guard = self.moderator_rooms.write().await;
			let entry = guard.entry(payload.sender.id).or_insert_with(HashSet::new);
			if has_mod_badge {
				entry.insert(room.clone());
			} else {
				entry.remove(&room);
			}
		}

		let author = UserRef {
			id: payload.sender.id.to_string(),
			login: payload.sender.username.clone(),
			display: Some(payload.sender.username.clone()),
		};
		let (normalized, emotes) = normalize_kick_content(&payload.content);
		let mut chat_message = ChatMessage::new(author, normalized);
		let room_id = room.room_id.as_str();
		chat_message.badges = payload
			.sender
			.identity
			.as_ref()
			.map(|identity| {
				identity
					.badges
					.iter()
					.map(|badge| match badge.badge_type.as_str() {
						"subscriber" => format!("kick:subscriber:{room_id}"),
						"moderator" => "kick:moderator".to_string(),
						"vip" => "kick:vip".to_string(),
						"broadcaster" => "kick:broadcaster".to_string(),
						_ => format!("kick:{}", badge.badge_type),
					})
					.collect::<Vec<_>>()
			})
			.unwrap_or_default();
		chat_message.ids.platform_id = Some(payload.id.clone());
		chat_message.emotes = emotes;

		let mut ingest = IngestEvent::new(Platform::Kick, room.room_id.clone(), IngestPayload::ChatMessage(chat_message));
		if let Ok(platform_id) = chatty_domain::PlatformMessageId::new(payload.id.clone()) {
			ingest.platform_message_id = Some(platform_id);
		}
		if let Some(ts) = payload.created_at.as_deref()
			&& let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(ts)
		{
			let utc = parsed.with_timezone(&chrono::Utc);
			let st = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(utc.timestamp_millis() as u64);
			ingest.platform_time = Some(st);
		}

		if events_tx.try_send(AdapterEvent::Ingest(Box::new(ingest))).is_err() {
			warn!("kick ws ingest channel closed");
		}
	}

	async fn handle_message_deleted(
		&mut self,
		payload: KickWsMessageDeleted,
		channel: Option<&str>,
		events_tx: &AdapterEventTx,
	) {
		let chatroom_id = channel.and_then(chatroom_id_from_channel);
		let room = chatroom_id.and_then(|id| self.room_by_chatroom_id.get(&id).cloned());
		let Some(room) = room else {
			return;
		};
		let Some(message) = payload.message else {
			return;
		};

		let mod_event = ModerationEvent {
			kind: "delete".to_string(),
			actor: None,
			target: None,
			target_message_platform_id: Some(message.id.clone()),
			notes: None,
			action: Some(ModerationAction::DeleteMessage { message_id: message.id }),
		};
		let ingest = IngestEvent::new(
			Platform::Kick,
			room.room_id.clone(),
			IngestPayload::Moderation(Box::new(mod_event)),
		);
		let _ = events_tx.try_send(AdapterEvent::Ingest(Box::new(ingest)));
	}

	async fn handle_user_banned(&mut self, payload: KickWsUserBan, channel: Option<&str>, events_tx: &AdapterEventTx) {
		let chatroom_id = channel.and_then(chatroom_id_from_channel);
		let room = chatroom_id.and_then(|id| self.room_by_chatroom_id.get(&id).cloned());
		let Some(room) = room else {
			return;
		};

		let expires_at = payload
			.expires_at
			.as_deref()
			.and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
			.map(|dt| dt.with_timezone(&chrono::Utc))
			.map(std::time::SystemTime::from);
		let is_timeout = expires_at.is_some();

		let mod_event = ModerationEvent {
			kind: if is_timeout {
				"timeout".to_string()
			} else {
				"ban".to_string()
			},
			actor: Some(UserRef {
				id: payload.banned_by.id.to_string(),
				login: payload.banned_by.username.clone(),
				display: Some(payload.banned_by.username.clone()),
			}),
			target: Some(UserRef {
				id: payload.user.id.to_string(),
				login: payload.user.username.clone(),
				display: Some(payload.user.username.clone()),
			}),
			target_message_platform_id: None,
			notes: None,
			action: Some(if is_timeout {
				ModerationAction::Timeout {
					duration_seconds: None,
					expires_at,
					reason: None,
				}
			} else {
				ModerationAction::Ban {
					is_permanent: Some(true),
					reason: None,
				}
			}),
		};
		let ingest = IngestEvent::new(
			Platform::Kick,
			room.room_id.clone(),
			IngestPayload::Moderation(Box::new(mod_event)),
		);
		let _ = events_tx.try_send(AdapterEvent::Ingest(Box::new(ingest)));
	}

	async fn handle_user_unbanned(&mut self, payload: KickWsUserUnban, channel: Option<&str>, events_tx: &AdapterEventTx) {
		let chatroom_id = channel.and_then(chatroom_id_from_channel);
		let room = chatroom_id.and_then(|id| self.room_by_chatroom_id.get(&id).cloned());
		let Some(room) = room else {
			return;
		};

		let mod_event = ModerationEvent {
			kind: "unban".to_string(),
			actor: Some(UserRef {
				id: payload.unbanned_by.id.to_string(),
				login: payload.unbanned_by.username.clone(),
				display: Some(payload.unbanned_by.username.clone()),
			}),
			target: Some(UserRef {
				id: payload.user.id.to_string(),
				login: payload.user.username.clone(),
				display: Some(payload.user.username.clone()),
			}),
			target_message_platform_id: None,
			notes: None,
			action: Some(ModerationAction::Unban {}),
		};
		let ingest = IngestEvent::new(
			Platform::Kick,
			room.room_id.clone(),
			IngestPayload::Moderation(Box::new(mod_event)),
		);
		let _ = events_tx.try_send(AdapterEvent::Ingest(Box::new(ingest)));
	}
}

fn map_kick_error(err: anyhow::Error) -> CommandError {
	let msg = err.to_string();
	if msg.contains("status=401") || msg.contains("status=403") {
		CommandError::NotAuthorized(Some(format!("kick {msg}")))
	} else if msg.contains("status=404") {
		CommandError::InvalidTopic(Some(format!("kick {msg}")))
	} else {
		CommandError::Internal(format!("kick {msg}"))
	}
}

fn parse_numeric_id(value: &str) -> Result<u64, CommandError> {
	value
		.parse::<u64>()
		.map_err(|_| CommandError::InvalidCommand(Some("kick invalid numeric id".to_string())))
}

pub(crate) type KickWs = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Debug, Deserialize)]
struct KickPusherEvent {
	event: String,
	data: JsonValue,
	#[serde(default)]
	channel: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KickWsChatMessage {
	id: String,
	chatroom_id: u64,
	content: String,
	#[serde(default)]
	created_at: Option<String>,
	sender: KickWsUser,
}

#[derive(Debug, Deserialize)]
struct KickWsUser {
	id: u64,
	username: String,
	#[serde(default)]
	identity: Option<KickWsIdentity>,
}

#[derive(Debug, Deserialize)]
struct KickWsIdentity {
	#[serde(default)]
	badges: Vec<KickWsBadge>,
}

#[derive(Debug, Deserialize)]
struct KickWsBadge {
	#[serde(rename = "type")]
	badge_type: String,
}

#[derive(Debug, Deserialize)]
struct KickWsMessageDeleted {
	#[serde(default)]
	message: Option<KickWsMessageRef>,
}

#[derive(Debug, Deserialize)]
struct KickWsMessageRef {
	id: String,
}

#[derive(Debug, Deserialize)]
struct KickWsUserBan {
	user: KickWsActor,
	banned_by: KickWsActor,
	#[serde(default)]
	expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KickWsUserUnban {
	user: KickWsActor,
	unbanned_by: KickWsActor,
}

#[derive(Debug, Deserialize)]
struct KickWsActor {
	id: u64,
	username: String,
}

fn parse_pusher_payload<T: DeserializeOwned>(data: JsonValue) -> Option<T> {
	if let Some(s) = data.as_str() {
		serde_json::from_str(s).ok()
	} else {
		serde_json::from_value(data).ok()
	}
}

fn normalize_kick_content(content: &str) -> (String, Vec<AssetRef>) {
	let mut output = String::with_capacity(content.len());
	let mut emotes = Vec::new();
	let mut seen = HashSet::new();
	let mut idx = 0;
	while idx < content.len() {
		let remaining = &content[idx..];
		if let Some(start) = remaining.find("[emote:") {
			let abs_start = idx + start;
			output.push_str(&content[idx..abs_start]);
			let rest = &content[abs_start + 7..];
			if let Some(end_rel) = rest.find(']') {
				let inside = &rest[..end_rel];
				let mut parts = inside.splitn(2, ':');
				let id = parts.next().unwrap_or("").trim();
				let name = parts.next().unwrap_or("").trim();
				if !id.is_empty() && !name.is_empty() {
					output.push_str(name);
					if seen.insert(id.to_string()) {
						emotes.push(AssetRef {
							id: format!("kick:emote:{}", id),
							name: name.to_string(),
							images: vec![AssetImage {
								scale: AssetScale::One,
								url: format!("https://files.kick.com/emotes/{}/fullsize", id),
								format: "png".to_string(),
								width: 0,
								height: 0,
							}],
						});
					}
					idx = abs_start + 7 + end_rel + 1;
					continue;
				}
			}
		}

		let ch = content[idx..].chars().next().unwrap();
		output.push(ch);
		idx += ch.len_utf8();
	}

	(output, emotes)
}

fn chatroom_id_from_channel(channel: &str) -> Option<u64> {
	if let Some(stripped) = channel.strip_prefix("chatrooms.") {
		let mut parts = stripped.split('.');
		if let Some(id) = parts.next() {
			return id.parse::<u64>().ok();
		}
	}
	if let Some(stripped) = channel.strip_prefix("chatroom_") {
		return stripped.parse::<u64>().ok();
	}
	None
}

#[async_trait]
impl PlatformAdapter for KickEventAdapter {
	fn platform(&self) -> Platform {
		Platform::Kick
	}

	async fn run(self: Box<Self>, mut control_rx: AdapterControlRx, events_tx: AdapterEventTx) -> anyhow::Result<()> {
		ensure_asset_cache_pruner();
		let mut this = *self;
		let session_id = new_session_id();
		let platform = this.platform();

		let _ = events_tx.try_send(status(
			platform,
			true,
			format!("kick adapter online (session_id={session_id})"),
		));

		let mut reconnect_attempt: u32 = 0;

		'outer: loop {
			let ws = match this.connect_ws().await {
				Ok(ws) => {
					reconnect_attempt = 0;
					ws
				}
				Err(err) => {
					warn!(error = %err, "kick ws connect failed");
					let delay =
						Self::backoff_delay(reconnect_attempt, this.cfg.reconnect_min_delay, this.cfg.reconnect_max_delay);
					reconnect_attempt = reconnect_attempt.saturating_add(1);
					tokio::time::sleep(delay).await;
					continue;
				}
			};

			let (mut ws_tx, mut ws_rx) = ws.split();
			let _ = events_tx.try_send(status(platform, true, "kick ws connected"));

			let rooms: Vec<RoomKey> = {
				let guard = this.joined_rooms.read().await;
				guard.iter().cloned().collect()
			};
			for room in rooms {
				if let Ok(chatroom_id) = this.resolve_chatroom_id(&room).await
					&& let Err(err) = this.send_pusher_subscribe(&mut ws_tx, chatroom_id).await
				{
					warn!(error = %err, chatroom_id, room = %room, "kick ws subscribe failed");
				}
			}

			loop {
				tokio::select! {
					msg = ws_rx.next() => {
						let Some(msg) = msg else {
							warn!("kick ws closed");
							break;
						};
						match msg {
							Ok(Message::Text(text)) => {
								match serde_json::from_str::<KickPusherEvent>(&text) {
									Ok(envelope) => {
										if envelope.event == "pusher:ping" {
											if let Err(err) = this.send_pusher_pong(&mut ws_tx).await {
												warn!(error = %err, "kick ws pong failed");
											}
										} else if envelope.event == "pusher:error" {
											warn!(payload = %text, "kick ws error event");
										} else if let Err(err) = this.handle_pusher_event(envelope, &events_tx).await {
											warn!(error = %err, "kick ws event handling failed");
										}
									}
									Err(err) => {
										warn!(error = %err, payload = %text, "kick ws message parse failed");
									}
								}
							}
							Ok(Message::Ping(payload)) => {
								let _ = ws_tx.send(Message::Pong(payload)).await;
							}
							Ok(Message::Close(_)) => {
								warn!("kick ws closed");
								break;
							}
							Ok(_) => {}
							Err(err) => {
								warn!(error = %err, "kick ws receive error");
								break;
							}
						}
					}
					cmd = control_rx.recv() => {
						let Some(cmd) = cmd else {
							info!(%platform, "kick adapter control channel closed; shutting down");
							break 'outer;
						};

						match cmd {
							AdapterControl::Join { room } => {
								if room.platform != platform {
									debug!(%platform, room=%room, "ignoring Join for non-matching platform");
									continue;
								}
								let mut guard = this.joined_rooms.write().await;
								if guard.insert(room.clone()) {
									let detail = format!("joined kick room:{}", room.room_id.as_str());
									let _ = events_tx.try_send(status(platform, true, detail));
									let cache_key = format!("kick:channel:{}:native", room.room_id.as_str());
									info!(%platform, room=%room.room_id, cache_key=%cache_key, "emitting AssetBundle ingest");
									let ingest = IngestEvent::new(
										platform,
										room.room_id.clone(),
										IngestPayload::AssetBundle(AssetBundle {
											provider: AssetProvider::Kick,
											scope: AssetScope::Channel,
											cache_key: cache_key.clone(),
											etag: Some("empty".to_string()),
											emotes: Vec::new(),
											badges: Vec::new(),
										}),
									);
									let _ = events_tx.try_send(AdapterEvent::Ingest(Box::new(ingest)));
									drop(guard);

								if let Ok(chatroom_id) = this.resolve_chatroom_id(&room).await
									&& let Err(err) = this.send_pusher_subscribe(&mut ws_tx, chatroom_id).await {
										warn!(error = %err, chatroom_id, room = %room, "kick ws subscribe failed");
									}

								let room_for_assets = room.clone();
								let events_tx_spawn = events_tx.clone();
								let seventv_subscriptions = this.seventv_subscriptions.clone();
								let broadcaster_id = this
									.resolve_broadcaster_id(&room, &SecretString::new(String::new()))
									.await
									.ok();
								tokio::spawn(async move {
									if let Some(id) = broadcaster_id {
										info!(%platform, room=%room_for_assets.room_id, broadcaster_id=%id, "fetching 7tv channel badges bundle (kick)");
										if let Ok(bundle) =
											fetch_7tv_channel_badges_bundle(SevenTvPlatform::Kick, &id.to_string()).await
										{
											info!(%platform, room=%room_for_assets.room_id, broadcaster_id=%id, cache_key=%bundle.cache_key, "emitting AssetBundle ingest");
											let ingest = IngestEvent::new(
												Platform::Kick,
												room_for_assets.room_id.clone(),
												IngestPayload::AssetBundle(bundle),
											);
											let _ = events_tx_spawn.try_send(AdapterEvent::Ingest(Box::new(ingest)));
										}

										info!(%platform, room=%room_for_assets.room_id, broadcaster_id=%id, "fetching 7tv emote set bundle (kick)");
										match fetch_7tv_bundle_with_sets(SevenTvPlatform::Kick, &id.to_string(), SevenTvCacheMode::UseCache).await {
											Ok((bundle, sets)) => {
												info!(%platform, room=%room_for_assets.room_id, broadcaster_id=%id, cache_key=%bundle.cache_key, "emitting AssetBundle ingest");
												let ingest = IngestEvent::new(
													Platform::Kick,
													room_for_assets.room_id.clone(),
													IngestPayload::AssetBundle(bundle),
												);
												let _ = events_tx_spawn.try_send(AdapterEvent::Ingest(Box::new(ingest)));

												let set_ids = sets.set_ids();
												if !set_ids.is_empty() {
													let api = ensure_seventv_event_api();
													let mut subscriptions = Vec::new();
													for set_id in set_ids {
														let (subscription, mut rx) = api.subscribe(DispatchType::EmoteSetUpdate, set_id.clone());
														subscriptions.push(subscription);
														let events_tx_updates = events_tx_spawn.clone();
														let room_updates = room_for_assets.clone();
														let platform_id = id.to_string();
														tokio::spawn(async move {
															while rx.recv().await.is_some() {
																match fetch_7tv_bundle_with_sets(
																	SevenTvPlatform::Kick,
																	&platform_id,
																	SevenTvCacheMode::Refresh,
																)
																.await
																{
																	Ok((bundle, _)) => {
																		info!(room=%room_updates.room_id, cache_key=%bundle.cache_key, "emitting updated 7tv emote set bundle (kick)");
																		let ingest = IngestEvent::new(
																			Platform::Kick,
																			room_updates.room_id.clone(),
																			IngestPayload::AssetBundle(bundle),
																			);
																		let _ = events_tx_updates.try_send(AdapterEvent::Ingest(Box::new(ingest)));
																	}
																	Err(error) => {
																		info!(room=%room_updates.room_id, error=?error, "failed to refresh 7tv emote set bundle (kick)");
																	}
																}
															}
														});
													}

													let mut guard = seventv_subscriptions.write().await;
													guard.insert(room_for_assets.clone(), subscriptions);
												}
											}
											Err(error) => {
												warn!(%platform, room=%room_for_assets.room_id, broadcaster_id=%id, error=?error, "failed to fetch 7tv emote set bundle (kick)");
											}
										}
									} else {
										warn!(%platform, room=%room_for_assets.room_id, "kick broadcaster id unresolved; skipping 7tv asset fetches");
									}

									if let Ok(bundle) = fetch_7tv_badges_bundle().await {
										info!(%platform, room=%room_for_assets.room_id, cache_key=%bundle.cache_key, "emitting AssetBundle ingest");
										let ingest = IngestEvent::new(
											Platform::Kick,
											room_for_assets.room_id.clone(),
											IngestPayload::AssetBundle(bundle),
										);
										let _ = events_tx_spawn.try_send(AdapterEvent::Ingest(Box::new(ingest)));
									}

									if let Some(bundle) = fetch_kick_badge_bundle(room_for_assets.room_id.as_str()).await {
										info!(%platform, room=%room_for_assets.room_id, cache_key=%bundle.cache_key, "emitting AssetBundle ingest");
										let ingest = IngestEvent::new(
											Platform::Kick,
											room_for_assets.room_id.clone(),
											IngestPayload::AssetBundle(bundle),
										);
										let _ = events_tx_spawn.try_send(AdapterEvent::Ingest(Box::new(ingest)));
									}

									let bundles = fetch_kick_emote_bundles(room_for_assets.room_id.as_str()).await;
									for bundle in bundles {
										info!(%platform, room=%room_for_assets.room_id, cache_key=%bundle.cache_key, scope=?bundle.scope, "emitting Kick emote bundle");
										let ingest = IngestEvent::new(
											Platform::Kick,
											room_for_assets.room_id.clone(),
											IngestPayload::AssetBundle(bundle),
										);
										let _ = events_tx_spawn.try_send(AdapterEvent::Ingest(Box::new(ingest)));
									}
								});
							}
						}
						AdapterControl::Leave { room } => {
							if room.platform != platform {
								debug!(%platform, room=%room, "ignoring Leave for non-matching platform");
								continue;
							}
							let mut guard = this.joined_rooms.write().await;
							if guard.remove(&room) {
								let detail = format!("left kick room:{}", room.room_id.as_str());
								let _ = events_tx.try_send(status(platform, true, detail));
							}
							drop(guard);
							if let Some((chatroom_id, _)) = this.chatroom_id_by_room.remove(&room) {
								this.room_by_chatroom_id.remove(&chatroom_id);
								if let Err(err) = this.send_pusher_unsubscribe(&mut ws_tx, chatroom_id).await {
									warn!(error = %err, chatroom_id, room = %room, "kick ws unsubscribe failed");
								}
							}

							if let Some(subscriptions) = this.seventv_subscriptions.write().await.remove(&room) {
								drop(subscriptions);
							}
						}
						AdapterControl::UpdateAuth { auth } => {
							this.apply_auth_update(auth).await;
							let has_users = !this.auth_user_ids.read().await.is_empty();
							if !has_users {
								this.maybe_notice_auth_issue("kick auth missing access token", &events_tx);
							} else {
								let _ = events_tx.try_send(status(platform, true, "kick auth updated"));
							}
						}
						AdapterControl::Command { request, auth, resp } => {
							let result = this.execute_command(request, auth).await;
							let _ = resp.send(result);
						}
						AdapterControl::QueryPermissions { room, auth, resp } => {
							let result = this.permissions_for_room(&room, auth).await;
							let _ = resp.send(result);
						}
						AdapterControl::Shutdown => {
							info!(%platform, "kick adapter received Shutdown");
							break 'outer;
						}
					}
					}
				}
			}

			let delay = Self::backoff_delay(reconnect_attempt, this.cfg.reconnect_min_delay, this.cfg.reconnect_max_delay);
			reconnect_attempt = reconnect_attempt.saturating_add(1);
			tokio::time::sleep(delay).await;
		}

		let _ = events_tx.try_send(status(platform, false, "kick adapter offline"));
		Ok(())
	}
}
