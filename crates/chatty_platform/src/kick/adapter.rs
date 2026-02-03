#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use bytes::Bytes;
use chatty_domain::{Platform, RoomId, RoomKey};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use rsa::RsaPublicKey;
use rsa::pkcs8::DecodePublicKey;
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::client::KickClient;
use crate::assets::{
	SevenTvPlatform, ensure_asset_cache_pruner, fetch_7tv_badges_bundle, fetch_7tv_bundle, fetch_7tv_channel_badges_bundle,
	fetch_kick_badge_bundle, fetch_kick_emote_bundle,
};
use crate::{
	AdapterAuth, AdapterControl, AdapterControlRx, AdapterEvent, AdapterEventTx, AssetBundle, AssetImage, AssetProvider,
	AssetRef, AssetScale, AssetScope, ChatMessage, CommandError, CommandRequest, IngestEvent, IngestPayload,
	ModerationAction, ModerationEvent, PermissionsInfo, PlatformAdapter, SecretString, UserRef, new_session_id, status,
};

#[derive(Clone)]
pub struct KickConfig {
	pub base_url: String,
	pub system_access_token: Option<SecretString>,
	pub broadcaster_id_overrides: HashMap<String, String>,
	pub resolve_cache_ttl: Duration,
	pub webhook_bind: Option<SocketAddr>,
	pub webhook_path: String,
	pub webhook_public_key_path: Option<PathBuf>,
	pub webhook_verify_signatures: bool,
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
			system_access_token: None,
			broadcaster_id_overrides: HashMap::new(),
			resolve_cache_ttl: Duration::from_secs(300),
			webhook_bind: None,
			webhook_path: "/kick/events".to_string(),
			webhook_public_key_path: None,
			webhook_verify_signatures: true,
		}
	}
}

const KICK_BASE_EVENTS: [&str; 1] = ["chat.message.sent"];
const KICK_MODERATOR_EVENTS: [&str; 1] = ["moderation.banned"];
const KICK_WEBHOOK_RECONCILE_INTERVAL_SECS: u64 = 120;

pub struct KickEventAdapter {
	cfg: KickConfig,
	joined_rooms: Arc<RwLock<HashSet<RoomKey>>>,
	moderator_rooms: Arc<RwLock<HashMap<u64, HashSet<RoomKey>>>>,
	auth_user_ids: Arc<RwLock<HashSet<u64>>>,
	user_scopes: Arc<RwLock<HashMap<u64, HashSet<String>>>>,
	broadcaster_id_by_room: HashMap<RoomKey, (u64, std::time::Instant)>,
	#[allow(dead_code)]
	emote_ids_by_room: Arc<RwLock<HashMap<RoomKey, HashSet<String>>>>,
	user_tokens: Arc<RwLock<HashMap<String, SecretString>>>,
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
			emote_ids_by_room: Arc::new(RwLock::new(HashMap::new())),
			user_tokens: Arc::new(RwLock::new(HashMap::new())),
			last_auth_error_notice: None,
		}
	}

	fn normalize_public_key_pem(raw_key: &str) -> String {
		let trimmed = raw_key.trim();
		if trimmed.contains("BEGIN PUBLIC KEY") {
			return trimmed.to_string();
		}

		format!("-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----", trimmed)
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

	async fn room_has_moderator(&self, room: &RoomKey) -> bool {
		let guard = self.moderator_rooms.read().await;
		guard.values().any(|rooms| rooms.contains(room))
	}

	async fn desired_webhook_events(&self, room: &RoomKey) -> Vec<super::client::KickEventSpec> {
		let mut events: Vec<super::client::KickEventSpec> = KICK_BASE_EVENTS
			.iter()
			.map(|name| super::client::KickEventSpec::new(*name, 1))
			.collect();

		if self.room_has_moderator(room).await {
			events.extend(
				KICK_MODERATOR_EVENTS
					.iter()
					.map(|name| super::client::KickEventSpec::new(*name, 1)),
			);
		}

		events
	}

	async fn pick_any_token(&self) -> Option<SecretString> {
		let guard = self.user_tokens.read().await;
		guard.values().next().cloned()
	}

	async fn pick_subscription_token(&self) -> Option<SecretString> {
		if let Some(token) = self.cfg.system_access_token.clone() {
			return Some(token);
		}
		self.pick_any_token().await
	}

	async fn apply_auth_update(&mut self, auth: AdapterAuth) {
		if let AdapterAuth::UserAccessToken {
			access_token, user_id, ..
		} = auth
		{
			self.last_auth_error_notice = None;

			let token_key = user_id
				.clone()
				.filter(|id| !id.trim().is_empty())
				.unwrap_or_else(|| access_token.expose().to_string());
			let mut tokens = self.user_tokens.write().await;
			tokens.insert(token_key, access_token.clone());

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
			} => client
				.send_chat_message(broadcaster_id, &text, reply_to_platform_message_id.as_deref())
				.await
				.map_err(map_kick_error)?,
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

	async fn ensure_webhook_subscription(&mut self, room: &RoomKey) -> Result<(), CommandError> {
		let Some(token) = self.pick_subscription_token().await else {
			return Err(CommandError::NotAuthorized(Some(
				"kick auth missing access token".to_string(),
			)));
		};

		let broadcaster_id = self.resolve_broadcaster_id(room, &token).await?;
		let desired_events = self.desired_webhook_events(room).await;
		let client = self.client_for_token(&token);

		let existing = client
			.list_event_subscriptions(Some(broadcaster_id))
			.await
			.map_err(|e| CommandError::Internal(e.to_string()))?;

		let mut missing = Vec::new();
		for desired in desired_events {
			let found = existing
				.iter()
				.any(|s| s.event == desired.name && s.version == desired.version);

			if !found {
				missing.push(desired);
			}
		}

		if missing.is_empty() {
			return Ok(());
		}

		metrics::counter!("chatty_kick_webhook_subscribe_requests_total").increment(1);
		client
			.create_event_subscriptions(Some(broadcaster_id), missing)
			.await
			.map_err(|e| CommandError::Internal(e.to_string()))?;
		metrics::counter!("chatty_kick_webhook_subscribe_success_total").increment(1);
		Ok(())
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

const KICK_PUBLIC_KEY_PEM: &str = include_str!("public_key.pem");
const MAX_KICK_WEBHOOK_BODY_BYTES: usize = 1_048_576; // 1 MiB

#[derive(Debug, serde::Deserialize)]
struct KickWebhookChatMessage {
	message_id: String,
	content: String,
	created_at: Option<String>,
	sender: KickWebhookUser,
	broadcaster: Option<KickWebhookUser>,
	#[serde(default)]
	emotes: Vec<KickWebhookEmote>,
}

#[derive(Debug, serde::Deserialize)]
struct KickWebhookUser {
	user_id: u64,
	username: String,
	channel_slug: String,
	identity: Option<KickWebhookIdentity>,
}

#[derive(Debug, serde::Deserialize)]
struct KickWebhookIdentity {
	#[serde(default)]
	badges: Vec<KickWebhookBadge>,
}

#[derive(Debug, serde::Deserialize)]
struct KickWebhookBadge {
	#[serde(rename = "type")]
	badge_type: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
struct KickWebhookEmote {
	emote_id: String,
	positions: Vec<KickWebhookEmotePosition>,
}

#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
struct KickWebhookEmotePosition {
	s: u32,
	e: u32,
}

#[derive(Clone)]
struct KickWebhookState {
	path: String,
	verify_signatures: bool,
	public_key: Option<RsaPublicKey>,
	joined_rooms: Arc<RwLock<HashSet<RoomKey>>>,
	moderator_rooms: Arc<RwLock<HashMap<u64, HashSet<RoomKey>>>>,
	auth_user_ids: Arc<RwLock<HashSet<u64>>>,
	emote_ids_by_room: Arc<RwLock<HashMap<RoomKey, HashSet<String>>>>,
	events_tx: AdapterEventTx,
}

async fn run_kick_webhook_server(
	bind: SocketAddr,
	state: KickWebhookState,
	mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
	let listener = TcpListener::bind(bind).await?;
	loop {
		tokio::select! {
			biased;
			_ = shutdown_rx.changed() => {
				info!(%bind, "kick webhook server shutting down");
				break;
			}
			accept = listener.accept() => {
				match accept {
					Ok((stream, _addr)) => {
						let io = TokioIo::new(stream);
						let state = state.clone();
						tokio::spawn(async move {
							let service = service_fn(move |req| handle_kick_webhook(req, state.clone()));
							if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
								warn!(error = %err, "kick webhook connection error");
							}
						});
					}
					Err(err) => {
						warn!(error = %err, "kick webhook accept failed");
						tokio::time::sleep(Duration::from_millis(100)).await;
					}
				}
			}
		}
	}
	Ok(())
}

async fn handle_kick_webhook(
	req: Request<Incoming>,
	state: KickWebhookState,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
	let (parts, body) = req.into_parts();

	if parts.method != Method::POST {
		return Ok(Response::builder()
			.status(StatusCode::METHOD_NOT_ALLOWED)
			.body(Full::new(Bytes::new()))
			.unwrap());
	}

	if parts.uri.path() != state.path {
		return Ok(Response::builder()
			.status(StatusCode::NOT_FOUND)
			.body(Full::new(Bytes::new()))
			.unwrap());
	}

	metrics::counter!("chatty_kick_webhook_requests_total").increment(1);

	let headers = parts.headers;
	let event_type = headers.get("Kick-Event-Type").and_then(|v| v.to_str().ok()).unwrap_or("");
	let message_id = headers
		.get("Kick-Event-Message-Id")
		.and_then(|v| v.to_str().ok())
		.unwrap_or("");
	let timestamp = headers
		.get("Kick-Event-Message-Timestamp")
		.and_then(|v| v.to_str().ok())
		.unwrap_or("");
	let signature = headers
		.get("Kick-Event-Signature")
		.and_then(|v| v.to_str().ok())
		.unwrap_or("");

	let body_bytes = match body.collect().await {
		Ok(collected) => collected.to_bytes(),
		Err(err) => {
			warn!(error = %err, "kick webhook body read failed");
			metrics::counter!("chatty_kick_webhook_body_errors_total").increment(1);
			return Ok(Response::builder()
				.status(StatusCode::BAD_REQUEST)
				.body(Full::new(Bytes::new()))
				.unwrap());
		}
	};

	if body_bytes.len() > MAX_KICK_WEBHOOK_BODY_BYTES {
		metrics::counter!("chatty_kick_webhook_body_too_large_total").increment(1);
		return Ok(Response::builder()
			.status(StatusCode::PAYLOAD_TOO_LARGE)
			.body(Full::new(Bytes::new()))
			.unwrap());
	}

	if state.verify_signatures {
		if state.public_key.is_none() {
			warn!("kick webhook signature verification enabled but no public key is configured");
			metrics::counter!("chatty_kick_webhook_signature_missing_total").increment(1);
			return Ok(Response::builder()
				.status(StatusCode::UNAUTHORIZED)
				.body(Full::new(Bytes::new()))
				.unwrap());
		}
		if message_id.is_empty() || timestamp.is_empty() || signature.is_empty() {
			metrics::counter!("chatty_kick_webhook_signature_missing_total").increment(1);
			return Ok(Response::builder()
				.status(StatusCode::UNAUTHORIZED)
				.body(Full::new(Bytes::new()))
				.unwrap());
		}

		match chrono::DateTime::parse_from_rfc3339(timestamp) {
			Ok(parsed_ts) => {
				let parsed_utc = parsed_ts.with_timezone(&chrono::Utc);
				let now = chrono::Utc::now();
				let age_secs = (now.signed_duration_since(parsed_utc)).num_seconds().abs();
				if age_secs > 300 {
					metrics::counter!("chatty_kick_webhook_stale_timestamp_total").increment(1);
					return Ok(Response::builder()
						.status(StatusCode::UNAUTHORIZED)
						.body(Full::new(Bytes::new()))
						.unwrap());
				}
			}
			Err(_) => {
				metrics::counter!("chatty_kick_webhook_timestamp_invalid_total").increment(1);
				return Ok(Response::builder()
					.status(StatusCode::UNAUTHORIZED)
					.body(Full::new(Bytes::new()))
					.unwrap());
			}
		}

		let mut signed = Vec::new();
		signed.extend_from_slice(message_id.as_bytes());
		signed.push(b'.');
		signed.extend_from_slice(timestamp.as_bytes());
		signed.push(b'.');
		signed.extend_from_slice(body_bytes.as_ref());
		let hash = Sha256::digest(&signed);
		let signature_bytes = match BASE64_STANDARD.decode(signature) {
			Ok(sig) => sig,
			Err(_) => {
				metrics::counter!("chatty_kick_webhook_signature_invalid_total").increment(1);
				return Ok(Response::builder()
					.status(StatusCode::UNAUTHORIZED)
					.body(Full::new(Bytes::new()))
					.unwrap());
			}
		};
		if let Some(public_key) = state.public_key.as_ref()
			&& public_key
				.verify(rsa::pkcs1v15::Pkcs1v15Sign::new::<Sha256>(), &hash, &signature_bytes)
				.is_err()
		{
			metrics::counter!("chatty_kick_webhook_signature_invalid_total").increment(1);
			return Ok(Response::builder()
				.status(StatusCode::UNAUTHORIZED)
				.body(Full::new(Bytes::new()))
				.unwrap());
		}
	}

	if event_type != "chat.message.sent" && event_type != "chat.message.deleted" && event_type != "moderation.banned" {
		metrics::counter!("chatty_kick_webhook_ignored_total").increment(1);
		return Ok(Response::builder()
			.status(StatusCode::NO_CONTENT)
			.body(Full::new(Bytes::new()))
			.unwrap());
	}

	let ingest = match event_type {
		"chat.message.sent" => {
			let payload: KickWebhookChatMessage = match serde_json::from_slice(&body_bytes) {
				Ok(v) => v,
				Err(err) => {
					warn!(error = %err, "kick webhook payload parse failed (chat.message.sent)");
					metrics::counter!("chatty_kick_webhook_parse_errors_total").increment(1);
					return Ok(Response::builder()
						.status(StatusCode::BAD_REQUEST)
						.body(Full::new(Bytes::new()))
						.unwrap());
				}
			};

			let channel_slug = payload
				.broadcaster
				.as_ref()
				.map(|b| b.channel_slug.as_str())
				.unwrap_or(payload.sender.channel_slug.as_str());
			let room_id = match RoomId::new(channel_slug.to_string()) {
				Ok(v) => v,
				Err(_) => {
					return Ok(Response::builder()
						.status(StatusCode::BAD_REQUEST)
						.body(Full::new(Bytes::new()))
						.unwrap());
				}
			};
			let room = RoomKey::new(Platform::Kick, room_id);

			let is_joined = {
				let guard = state.joined_rooms.read().await;
				guard.contains(&room)
			};
			if !is_joined {
				metrics::counter!("chatty_kick_webhook_unsubscribed_total").increment(1);
				return Ok(Response::builder()
					.status(StatusCode::NO_CONTENT)
					.body(Full::new(Bytes::new()))
					.unwrap());
			}

			if state.auth_user_ids.read().await.contains(&payload.sender.user_id) {
				let has_mod_badge = payload
					.sender
					.identity
					.as_ref()
					.map(|identity| {
						identity
							.badges
							.iter()
							.any(|badge| badge.badge_type.as_deref() == Some("moderator"))
					})
					.unwrap_or(false);
				let mut guard = state.moderator_rooms.write().await;
				let entry = guard.entry(payload.sender.user_id).or_insert_with(HashSet::new);

				if has_mod_badge {
					entry.insert(room.clone());
				} else {
					entry.remove(&room);
				}
			}

			let author = UserRef {
				id: payload.sender.user_id.to_string(),
				login: payload.sender.username.clone(),
				display: Some(payload.sender.username.clone()),
			};
			let mut chat_message = ChatMessage::new(author, payload.content.clone());
			let room_id = room.room_id.as_str();
			chat_message.badges = payload
				.sender
				.identity
				.as_ref()
				.map(|identity| {
					identity
						.badges
						.iter()
						.filter_map(|badge| {
							badge.badge_type.as_ref().map(|t| match t.as_str() {
								"subscriber" => format!("kick:subscriber:{room_id}"),
								"moderator" => "kick:moderator".to_string(),
								"vip" => "kick:vip".to_string(),
								_ => format!("kick:{t}"),
							})
						})
						.collect::<Vec<_>>()
				})
				.unwrap_or_default();
			chat_message.ids.platform_id = Some(payload.message_id.clone());

			for kick_emote in &payload.emotes {
				if let Some(pos) = kick_emote.positions.first() {
					let start = pos.s as usize;
					let end = pos.e as usize;
					if let Some(name) = payload.content.get(start..=end) {
						chat_message.emotes.push(AssetRef {
							id: format!("kick:emote:{}", kick_emote.emote_id),
							name: name.to_string(),
							images: vec![AssetImage {
								scale: AssetScale::One,
								url: format!("https://files.kick.com/emotes/{}/fullsize", kick_emote.emote_id),
								format: "png".to_string(),
								width: 0,
								height: 0,
							}],
						});
					}
				}
			}

			let mut ingest =
				IngestEvent::new(Platform::Kick, room.room_id.clone(), IngestPayload::ChatMessage(chat_message));
			if let Some(ts) = payload.created_at.as_deref()
				&& let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(ts)
			{
				let utc = parsed.with_timezone(&chrono::Utc);
				let st = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(utc.timestamp_millis() as u64);
				ingest.platform_time = Some(st);
			}

			let emote_ids: Vec<String> = payload.emotes.iter().map(|e| e.emote_id.clone()).collect();
			if !emote_ids.is_empty() {
				let mut guard = state.emote_ids_by_room.write().await;
				let room_emotes = guard.entry(room.clone()).or_insert_with(HashSet::new);
				let old_len = room_emotes.len();
				for id in emote_ids {
					room_emotes.insert(id);
				}
				let new_len = room_emotes.len();
				if new_len > old_len {
					let emote_ids_vec: Vec<String> = room_emotes.iter().cloned().collect();
					drop(guard);
					let room_for_emotes = room.clone();
					let events_tx = state.events_tx.clone();

					tokio::spawn(async move {
						if let Some(bundle) = fetch_kick_emote_bundle(room_for_emotes.room_id.as_str(), &emote_ids_vec).await
						{
							let ingest = IngestEvent::new(
								Platform::Kick,
								room_for_emotes.room_id.clone(),
								IngestPayload::AssetBundle(bundle),
							);
							let _ = events_tx.try_send(AdapterEvent::Ingest(Box::new(ingest)));
						}
					});
				}
			}
			ingest
		}
		"moderation.banned" => {
			let payload: serde_json::Value = match serde_json::from_slice(&body_bytes) {
				Ok(v) => v,
				Err(_) => {
					return Ok(Response::builder()
						.status(StatusCode::BAD_REQUEST)
						.body(Full::new(Bytes::new()))
						.unwrap());
				}
			};

			let banned_user = payload.get("banned_user");
			let user_id = banned_user
				.and_then(|v| v.get("user_id"))
				.and_then(|v| v.as_u64())
				.map(|v| v.to_string())
				.unwrap_or_default();
			let username = banned_user
				.and_then(|v| v.get("username"))
				.and_then(|v| v.as_str())
				.unwrap_or("");

			let moderator = payload.get("moderator");
			let actor_id = moderator
				.and_then(|v| v.get("user_id"))
				.and_then(|v| v.as_u64())
				.map(|v| v.to_string())
				.unwrap_or_default();
			let actor_username = moderator
				.and_then(|v| v.get("username"))
				.and_then(|v| v.as_str())
				.unwrap_or("");

			let metadata = payload.get("metadata");
			let reason = metadata
				.and_then(|v| v.get("reason"))
				.and_then(|v| v.as_str())
				.map(|v| v.to_string());
			let created_at_str = metadata.and_then(|v| v.get("created_at")).and_then(|v| v.as_str());
			let expires_at_str = metadata.and_then(|v| v.get("expires_at")).and_then(|v| v.as_str());

			let created_at = created_at_str
				.and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
				.map(|dt| dt.with_timezone(&chrono::Utc));

			let expires_at = expires_at_str
				.and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
				.map(|dt| dt.with_timezone(&chrono::Utc));

			let duration_seconds = if let (Some(c), Some(e)) = (created_at, expires_at) {
				let diff = e.signed_duration_since(c);
				if diff.num_seconds() > 0 {
					Some(diff.num_seconds() as u64)
				} else {
					None
				}
			} else {
				None
			};

			let channel_slug = payload
				.get("broadcaster")
				.and_then(|v| v.get("channel_slug"))
				.and_then(|v| v.as_str())
				.unwrap_or("");
			let room_id =
				RoomId::new(channel_slug.to_string()).unwrap_or_else(|_| RoomId::new("unknown".to_string()).unwrap());

			let is_timeout = expires_at.is_some();

			let mod_event = ModerationEvent {
				kind: if is_timeout {
					"timeout".to_string()
				} else {
					"ban".to_string()
				},
				actor: Some(UserRef {
					id: actor_id,
					login: actor_username.to_string(),
					display: Some(actor_username.to_string()),
				}),
				target: Some(UserRef {
					id: user_id,
					login: username.to_string(),
					display: Some(username.to_string()),
				}),
				target_message_platform_id: None,
				notes: reason.clone(),
				action: Some(if is_timeout {
					ModerationAction::Timeout {
						duration_seconds,
						expires_at: expires_at.map(std::time::SystemTime::from),
						reason: reason.clone(),
					}
				} else {
					ModerationAction::Ban {
						is_permanent: Some(true),
						reason,
					}
				}),
			};
			IngestEvent::new(Platform::Kick, room_id, IngestPayload::Moderation(Box::new(mod_event)))
		}
		_ => unreachable!(),
	};

	if state.events_tx.send(AdapterEvent::Ingest(Box::new(ingest))).await.is_err() {
		warn!("kick webhook ingest channel closed");
		metrics::counter!("chatty_kick_webhook_ingest_errors_total").increment(1);
	} else {
		metrics::counter!("chatty_kick_webhook_ingest_total").increment(1);
	}

	Ok(Response::builder()
		.status(StatusCode::OK)
		.body(Full::new(Bytes::new()))
		.unwrap())
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

		if this.pick_subscription_token().await.is_none() {
			warn!(
				"kick webhook auto-subscribe enabled but no user access token is available yet; webhooks will not be registered until a user signs in"
			);
		}

		let mut webhook_shutdown_tx: Option<tokio::sync::watch::Sender<bool>> = None;

		if let Some(bind) = this.cfg.webhook_bind {
			let public_key_pem = if let Some(path) = this.cfg.webhook_public_key_path.clone() {
				match std::fs::read_to_string(&path) {
					Ok(contents) => contents,
					Err(err) => {
						warn!(error = %err, path = %path.display(), "failed to read kick webhook public key; using bundled key");
						KICK_PUBLIC_KEY_PEM.to_string()
					}
				}
			} else {
				let client = KickClient::new(this.cfg.base_url.clone(), "");
				match client.fetch_public_key().await {
					Ok(key) => Self::normalize_public_key_pem(&key),
					Err(err) => {
						warn!(error = %err, "failed to fetch kick webhook public key; using bundled key");
						KICK_PUBLIC_KEY_PEM.to_string()
					}
				}
			};
			let public_key = RsaPublicKey::from_public_key_pem(&public_key_pem)
				.or_else(|_| RsaPublicKey::from_public_key_pem(KICK_PUBLIC_KEY_PEM))
				.ok();
			let state = KickWebhookState {
				path: this.cfg.webhook_path.clone(),
				verify_signatures: this.cfg.webhook_verify_signatures,
				public_key,
				joined_rooms: Arc::clone(&this.joined_rooms),
				moderator_rooms: Arc::clone(&this.moderator_rooms),
				auth_user_ids: Arc::clone(&this.auth_user_ids),
				emote_ids_by_room: Arc::new(RwLock::new(HashMap::new())),
				events_tx: events_tx.clone(),
			};

			let status_detail = format!("kick webhook listening on {bind}{}", this.cfg.webhook_path);
			let _ = events_tx.try_send(status(platform, true, status_detail));

			let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
			webhook_shutdown_tx = Some(shutdown_tx.clone());

			tokio::spawn(async move {
				if let Err(err) = run_kick_webhook_server(bind, state, shutdown_rx).await {
					warn!(error = %err, "kick webhook server stopped");
				}
			});
		} else {
			warn!("kick webhook ingestion disabled (no webhook_bind configured)");
		}

		let _ = events_tx.try_send(status(
			platform,
			true,
			format!("kick adapter online (session_id={session_id})"),
		));

		let mut reconcile_interval = tokio::time::interval(Duration::from_secs(KICK_WEBHOOK_RECONCILE_INTERVAL_SECS));

		loop {
			tokio::select! {
				_ = reconcile_interval.tick() => {
					let rooms: Vec<RoomKey> = {
						let guard = this.joined_rooms.read().await;
						guard.iter().cloned().collect()
					};
					for room in rooms {
						if let Err(err) = this.ensure_webhook_subscription(&room).await {
							warn!(error = %err, room=%room, "kick webhook subscription reconcile failed");
							metrics::counter!("chatty_kick_webhook_subscribe_errors_total").increment(1);
						}
					}
				}
				cmd = control_rx.recv() => {
					let Some(cmd) = cmd else {
						info!(%platform, "kick adapter control channel closed; shutting down");
						break;
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
								let room_for_assets = room.clone();
								let events_tx = events_tx.clone();
								let token_for_assets = this.pick_subscription_token().await;
								let broadcaster_id = match token_for_assets.as_ref() {
									Some(token) => this.resolve_broadcaster_id(&room, token).await.ok(),
									None => None,
								};
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
											let _ = events_tx.try_send(AdapterEvent::Ingest(Box::new(ingest)));
										}

										info!(%platform, room=%room_for_assets.room_id, broadcaster_id=%id, "fetching 7tv emote set bundle (kick)");
										if let Ok(bundle) = fetch_7tv_bundle(SevenTvPlatform::Kick, &id.to_string()).await {
											info!(%platform, room=%room_for_assets.room_id, broadcaster_id=%id, cache_key=%bundle.cache_key, "emitting AssetBundle ingest");
											let ingest = IngestEvent::new(
												Platform::Kick,
												room_for_assets.room_id.clone(),
												IngestPayload::AssetBundle(bundle),
											);
											let _ = events_tx.try_send(AdapterEvent::Ingest(Box::new(ingest)));
										}
									} else {
										warn!(%platform, room=%room_for_assets.room_id, "kick broadcaster id unresolved; skipping 7tv/kick asset fetches (check Kick OAuth token or broadcaster overrides)");
									}

									if let Ok(bundle) = fetch_7tv_badges_bundle().await {
										info!(%platform, room=%room_for_assets.room_id, cache_key=%bundle.cache_key, "emitting AssetBundle ingest");
										let ingest = IngestEvent::new(
											Platform::Kick,
											room_for_assets.room_id.clone(),
											IngestPayload::AssetBundle(bundle),
										);
										let _ = events_tx.try_send(AdapterEvent::Ingest(Box::new(ingest)));
									}

									if let Some(bundle) = fetch_kick_badge_bundle(room_for_assets.room_id.as_str()).await {
										info!(%platform, room=%room_for_assets.room_id, cache_key=%bundle.cache_key, "emitting AssetBundle ingest");
										let ingest = IngestEvent::new(
											Platform::Kick,
											room_for_assets.room_id.clone(),
											IngestPayload::AssetBundle(bundle),
										);
										let _ = events_tx.try_send(AdapterEvent::Ingest(Box::new(ingest)));
									}
								});
								if let Err(err) = this.ensure_webhook_subscription(&room).await {
									warn!(error = %err, room=%room, "kick webhook subscription failed");
									metrics::counter!("chatty_kick_webhook_subscribe_errors_total").increment(1);
								}
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
						}
						AdapterControl::UpdateAuth { auth } => {
							this.apply_auth_update(auth).await;
							let has_tokens = !this.user_tokens.read().await.is_empty();
							if !has_tokens {
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

							if let Some(tx) = webhook_shutdown_tx.take() {
								let _ = tx.send(true);
							}
							break;
						}
					}
				}
			}
		}

		let _ = events_tx.try_send(status(platform, false, "kick adapter offline"));
		Ok(())
	}
}
