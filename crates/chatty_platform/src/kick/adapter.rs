#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chatty_domain::{Platform, RoomId, RoomKey};
use tracing::{debug, info, warn};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use bytes::Bytes;
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

use crate::assets::{
	SevenTvPlatform, ensure_asset_cache_pruner, fetch_7tv_badges_bundle, fetch_7tv_bundle, fetch_7tv_channel_badges_bundle,
	fetch_kick_badge_bundle, fetch_kick_emote_bundle,
};
use crate::{
	AdapterAuth, AdapterControl, AdapterControlRx, AdapterEvent, AdapterEventTx, AssetBundle, AssetProvider, AssetScope,
	ChatMessage, CommandError, CommandRequest, IngestEvent, IngestPayload, PermissionsInfo, PlatformAdapter, SecretString,
	UserRef, new_session_id, status,
};

use super::client::KickClient;

#[derive(Clone)]
pub struct KickConfig {
	pub base_url: String,
	pub access_token: SecretString,
	pub user_id: Option<String>,
	pub broadcaster_id_overrides: HashMap<String, String>,
	pub resolve_cache_ttl: Duration,
	pub webhook_bind: Option<SocketAddr>,
	pub webhook_path: String,
	pub webhook_public_key_path: Option<PathBuf>,
	pub webhook_verify_signatures: bool,
	pub webhook_auto_subscribe: bool,
	pub webhook_events: Vec<String>,
}

impl KickConfig {
	pub fn new(access_token: SecretString) -> Self {
		Self {
			base_url: "https://api.kick.com".to_string(),
			access_token,
			user_id: None,
			broadcaster_id_overrides: HashMap::new(),
			resolve_cache_ttl: Duration::from_secs(300),
			webhook_bind: None,
			webhook_path: "/kick/events".to_string(),
			webhook_public_key_path: None,
			webhook_verify_signatures: true,
			webhook_auto_subscribe: false,
			webhook_events: vec!["chat.message.sent".to_string()],
		}
	}
}

pub struct KickEventAdapter {
	cfg: KickConfig,
	client: KickClient,
	joined_rooms: Arc<RwLock<HashSet<RoomKey>>>,
	moderator_rooms: Arc<RwLock<HashSet<RoomKey>>>,
	auth_user_id: Arc<RwLock<Option<u64>>>,
	broadcaster_id_by_room: HashMap<RoomKey, (u64, std::time::Instant)>,
	#[allow(dead_code)]
	emote_ids_by_room: Arc<RwLock<HashMap<RoomKey, HashSet<String>>>>,
	last_auth_error_notice: Option<String>,
}

impl KickEventAdapter {
	pub fn new(cfg: KickConfig) -> Self {
		let client = KickClient::new(cfg.base_url.clone(), cfg.access_token.expose().to_string());
		Self {
			cfg,
			client,
			joined_rooms: Arc::new(RwLock::new(HashSet::new())),
			moderator_rooms: Arc::new(RwLock::new(HashSet::new())),
			auth_user_id: Arc::new(RwLock::new(None)),
			broadcaster_id_by_room: HashMap::new(),
			emote_ids_by_room: Arc::new(RwLock::new(HashMap::new())),
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

	async fn apply_auth_update(&mut self, auth: AdapterAuth) {
		if let AdapterAuth::UserAccessToken {
			access_token, user_id, ..
		} = auth
		{
			self.cfg.access_token = access_token.clone();
			self.cfg.user_id = user_id;
			self.client.set_access_token(access_token.expose().to_string());
			self.last_auth_error_notice = None;

			let parsed_user_id = self.cfg.user_id.as_deref().and_then(|id| id.trim().parse::<u64>().ok());
			{
				let mut guard = self.auth_user_id.write().await;
				*guard = parsed_user_id;
			}
			self.moderator_rooms.write().await.clear();
		}
	}

	async fn resolve_broadcaster_id(&mut self, room: &RoomKey) -> Result<u64, CommandError> {
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

		let resolved = self
			.client
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

	fn ensure_auth(&self) -> Result<(), CommandError> {
		if self.cfg.access_token.expose().trim().is_empty() {
			return Err(CommandError::NotAuthorized(Some(
				"kick auth missing access token".to_string(),
			)));
		}
		Ok(())
	}

	async fn execute_command(&mut self, request: CommandRequest) -> Result<(), CommandError> {
		let room = match &request {
			CommandRequest::SendChat { room, .. }
			| CommandRequest::DeleteMessage { room, .. }
			| CommandRequest::TimeoutUser { room, .. }
			| CommandRequest::BanUser { room, .. } => room,
		};

		if room.platform != Platform::Kick {
			return Err(CommandError::InvalidTopic(None));
		}

		self.ensure_auth()?;
		let broadcaster_id = self.resolve_broadcaster_id(room).await?;
		let is_broadcaster = self
			.cfg
			.user_id
			.as_deref()
			.and_then(|id| if id.trim().is_empty() { None } else { Some(id) })
			.and_then(|id| parse_numeric_id(id).ok())
			.map(|id| id == broadcaster_id)
			.unwrap_or(false);
		let is_moderator = self.is_moderator_for_room(room).await;
		let can_moderate = is_moderator || is_broadcaster;

		match request {
			CommandRequest::SendChat {
				text,
				reply_to_platform_message_id,
				..
			} => self
				.client
				.send_chat_message(broadcaster_id, &text, reply_to_platform_message_id.as_deref())
				.await
				.map_err(map_kick_error)?,
			CommandRequest::DeleteMessage { platform_message_id, .. } => {
				if !can_moderate {
					return Err(CommandError::NotAuthorized(None));
				}
				self.client
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
				self.client
					.ban_user(broadcaster_id, parsed_user_id, Some(duration_seconds), reason.as_deref())
					.await
					.map_err(map_kick_error)?
			}
			CommandRequest::BanUser { user_id, reason, .. } => {
				if !can_moderate {
					return Err(CommandError::NotAuthorized(None));
				}
				let parsed_user_id = parse_numeric_id(&user_id)?;
				self.client
					.ban_user(broadcaster_id, parsed_user_id, None, reason.as_deref())
					.await
					.map_err(map_kick_error)?
			}
		};

		Ok(())
	}

	async fn permissions_for_room(&mut self, room: &RoomKey) -> PermissionsInfo {
		if room.platform != Platform::Kick {
			return PermissionsInfo::default();
		}
		if self.ensure_auth().is_err() {
			return PermissionsInfo::default();
		}
		let broadcaster_id = match self.resolve_broadcaster_id(room).await {
			Ok(id) => id,
			Err(_) => return PermissionsInfo::default(),
		};
		let is_broadcaster = self
			.cfg
			.user_id
			.as_deref()
			.and_then(|id| if id.trim().is_empty() { None } else { Some(id) })
			.and_then(|id| parse_numeric_id(id).ok())
			.map(|id| id == broadcaster_id)
			.unwrap_or(false);
		let is_moderator = self.is_moderator_for_room(room).await;
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

	async fn is_moderator_for_room(&self, room: &RoomKey) -> bool {
		let guard = self.moderator_rooms.read().await;
		guard.contains(room)
	}

	async fn ensure_webhook_subscription(&mut self, room: &RoomKey) -> Result<(), CommandError> {
		if !self.cfg.webhook_auto_subscribe {
			return Ok(());
		}
		let broadcaster_id = self.resolve_broadcaster_id(room).await?;
		let desired_events: Vec<super::client::KickEventSpec> = self
			.cfg
			.webhook_events
			.iter()
			.map(|name| super::client::KickEventSpec::new(name.clone(), 1))
			.collect();

		let existing = self
			.client
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
		self.client
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
	moderator_rooms: Arc<RwLock<HashSet<RoomKey>>>,
	auth_user_id: Arc<RwLock<Option<u64>>>,
	emote_ids_by_room: Arc<RwLock<HashMap<RoomKey, HashSet<String>>>>,
	events_tx: AdapterEventTx,
}

async fn run_kick_webhook_server(bind: SocketAddr, state: KickWebhookState) -> anyhow::Result<()> {
	let listener = TcpListener::bind(bind).await?;
	loop {
		let (stream, _addr) = listener.accept().await?;
		let io = TokioIo::new(stream);
		let state = state.clone();
		tokio::spawn(async move {
			let service = service_fn(move |req| handle_kick_webhook(req, state.clone()));
			if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
				warn!(error = %err, "kick webhook connection error");
			}
		});
	}
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

	if event_type != "chat.message.sent" {
		metrics::counter!("chatty_kick_webhook_ignored_total").increment(1);
		return Ok(Response::builder()
			.status(StatusCode::NO_CONTENT)
			.body(Full::new(Bytes::new()))
			.unwrap());
	}

	let payload: KickWebhookChatMessage = match serde_json::from_slice(&body_bytes) {
		Ok(v) => v,
		Err(err) => {
			warn!(error = %err, "kick webhook payload parse failed");
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

	if let Some(auth_user_id) = *state.auth_user_id.read().await
		&& auth_user_id == payload.sender.user_id
	{
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
		if has_mod_badge {
			guard.insert(room.clone());
		} else {
			guard.remove(&room);
		}
	}

	let author = UserRef {
		id: payload.sender.user_id.to_string(),
		login: payload.sender.username.clone(),
		display: Some(payload.sender.username.clone()),
	};
	let mut chat_message = ChatMessage::new(author, payload.content);
	chat_message.badges = payload
		.sender
		.identity
		.as_ref()
		.map(|identity| {
			identity
				.badges
				.iter()
				.filter_map(|badge| badge.badge_type.as_ref().map(|t| format!("kick:{t}")))
				.collect::<Vec<_>>()
		})
		.unwrap_or_default();
	chat_message.ids.platform_id = Some(payload.message_id.clone());

	let mut ingest = IngestEvent::new(Platform::Kick, room.room_id.clone(), IngestPayload::ChatMessage(chat_message));
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
				if let Some(bundle) = fetch_kick_emote_bundle(room_for_emotes.room_id.as_str(), &emote_ids_vec).await {
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
				KICK_PUBLIC_KEY_PEM.to_string()
			};
			let public_key = RsaPublicKey::from_public_key_pem(&public_key_pem).ok();
			let state = KickWebhookState {
				path: this.cfg.webhook_path.clone(),
				verify_signatures: this.cfg.webhook_verify_signatures,
				public_key,
				joined_rooms: Arc::clone(&this.joined_rooms),
				moderator_rooms: Arc::clone(&this.moderator_rooms),
				auth_user_id: Arc::clone(&this.auth_user_id),
				emote_ids_by_room: Arc::new(RwLock::new(HashMap::new())),
				events_tx: events_tx.clone(),
			};

			let status_detail = format!("kick webhook listening on {bind}{}", this.cfg.webhook_path);
			let _ = events_tx.try_send(status(platform, true, status_detail));

			tokio::spawn(async move {
				if let Err(err) = run_kick_webhook_server(bind, state).await {
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

		loop {
			let cmd = control_rx.recv().await;
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
						let ingest = IngestEvent::new(
							platform,
							room.room_id.clone(),
							IngestPayload::AssetBundle(AssetBundle {
								provider: AssetProvider::Kick,
								scope: AssetScope::Channel,
								cache_key: format!("kick:channel:{}:native", room.room_id.as_str()),
								etag: Some("empty".to_string()),
								emotes: Vec::new(),
								badges: Vec::new(),
							}),
						);
						let _ = events_tx.try_send(AdapterEvent::Ingest(Box::new(ingest)));
						drop(guard);
						let room_for_assets = room.clone();
						let events_tx = events_tx.clone();
						let broadcaster_id = this.resolve_broadcaster_id(&room).await.ok();
						tokio::spawn(async move {
							if let Some(id) = broadcaster_id {
								if let Ok(bundle) =
									fetch_7tv_channel_badges_bundle(SevenTvPlatform::Kick, &id.to_string()).await
								{
									let ingest = IngestEvent::new(
										Platform::Kick,
										room_for_assets.room_id.clone(),
										IngestPayload::AssetBundle(bundle),
									);
									let _ = events_tx.try_send(AdapterEvent::Ingest(Box::new(ingest)));
								}

								if let Ok(bundle) = fetch_7tv_bundle(SevenTvPlatform::Kick, &id.to_string()).await {
									let ingest = IngestEvent::new(
										Platform::Kick,
										room_for_assets.room_id.clone(),
										IngestPayload::AssetBundle(bundle),
									);
									let _ = events_tx.try_send(AdapterEvent::Ingest(Box::new(ingest)));
								}
							}

							if let Ok(bundle) = fetch_7tv_badges_bundle().await {
								let ingest = IngestEvent::new(
									Platform::Kick,
									room_for_assets.room_id.clone(),
									IngestPayload::AssetBundle(bundle),
								);
								let _ = events_tx.try_send(AdapterEvent::Ingest(Box::new(ingest)));
							}

							if let Some(bundle) = fetch_kick_badge_bundle(room_for_assets.room_id.as_str()).await {
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
					if this.cfg.access_token.expose().trim().is_empty() {
						this.maybe_notice_auth_issue("kick auth missing access token", &events_tx);
					} else {
						let _ = events_tx.try_send(status(platform, true, "kick auth updated"));
					}
				}
				AdapterControl::Command { request, resp } => {
					let result = this.execute_command(request).await;
					let _ = resp.send(result);
				}

				AdapterControl::QueryPermissions { room, resp } => {
					let result = this.permissions_for_room(&room).await;
					let _ = resp.send(result);
				}
				AdapterControl::Shutdown => {
					info!(%platform, "kick adapter received Shutdown");
					break;
				}
			}
		}

		let _ = events_tx.try_send(status(platform, false, "kick adapter offline"));
		Ok(())
	}
}
