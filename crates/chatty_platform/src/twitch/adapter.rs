#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::Context;
use chatty_domain::{Platform, RoomKey};
use futures_util::{SinkExt, StreamExt};
use tokio::time::{Instant, sleep};
use tokio_tungstenite::tungstenite::protocol::Message;
use tracing::{debug, info, warn};
use url::Url;

use super::helix::{HelixClient, HelixCreateSubscriptionResponse, HelixSubscriptionData, refresh_user_token};
use super::{eventsub, notifications};
use crate::assets::{
	SevenTvPlatform, ensure_asset_cache_pruner, fetch_7tv_badges_bundle, fetch_7tv_bundle, fetch_7tv_channel_badges_bundle,
	fetch_bttv_badges_bundle, fetch_bttv_bundle, fetch_bttv_global_emotes_bundle, fetch_ffz_badges_bundle, fetch_ffz_bundle,
	fetch_ffz_global_emotes_bundle, fetch_twitch_badges_bundle, fetch_twitch_channel_badges_bundle,
	fetch_twitch_channel_emotes_bundle, fetch_twitch_global_emotes_bundle,
};
use crate::{
	AdapterAuth, AdapterControl, AdapterControlRx, AdapterEvent, AdapterEventTx, AssetBundle, AssetProvider, AssetScope,
	ChatMessage, CommandError, CommandRequest, IngestEvent, IngestMessageIds, IngestPayload, IngestTrace, PermissionsInfo,
	PlatformAdapter, SecretString, new_session_id, status, status_error,
};

pub(crate) type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub(crate) type TwitchWs = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
pub(crate) type WsConnector = Arc<dyn Fn(Url) -> BoxFuture<'static, anyhow::Result<TwitchWs>> + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum TwitchSubscriptionType {
	ChatMessage,
	ChatMessageDelete,
	ChannelBan,
	ChannelModerate,
	ChannelRaid,
	ChannelCheer,
	ChannelSubscribe,
}

impl TwitchSubscriptionType {
	fn as_helix_type(&self) -> &'static str {
		match self {
			Self::ChatMessage => "channel.chat.message",
			Self::ChatMessageDelete => "channel.chat.message_delete",
			Self::ChannelBan => "channel.ban",
			Self::ChannelModerate => "channel.moderate",
			Self::ChannelRaid => "channel.raid",
			Self::ChannelCheer => "channel.cheer",
			Self::ChannelSubscribe => "channel.subscribe",
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HelixErrorCategory {
	Auth,
	RateLimit,
	Conflict,
	NotFound,
	BadRequest,
	ServerError,
	Other,
}

fn condition_value<'a>(condition: &'a serde_json::Value, key: &str) -> &'a str {
	condition.get(key).and_then(|v| v.as_str()).unwrap_or_default()
}

fn condition_matches(
	sub_type: TwitchSubscriptionType,
	condition: &serde_json::Value,
	broadcaster_user_id: &str,
	user_id: &str,
) -> bool {
	match sub_type {
		TwitchSubscriptionType::ChatMessage | TwitchSubscriptionType::ChatMessageDelete => {
			condition_value(condition, "broadcaster_user_id") == broadcaster_user_id
				&& condition_value(condition, "user_id") == user_id
		}
		TwitchSubscriptionType::ChannelModerate => {
			condition_value(condition, "broadcaster_user_id") == broadcaster_user_id
				&& condition_value(condition, "moderator_user_id") == user_id
		}
		TwitchSubscriptionType::ChannelRaid => condition_value(condition, "to_broadcaster_user_id") == broadcaster_user_id,
		TwitchSubscriptionType::ChannelBan
		| TwitchSubscriptionType::ChannelCheer
		| TwitchSubscriptionType::ChannelSubscribe => condition_value(condition, "broadcaster_user_id") == broadcaster_user_id,
	}
}

fn transport_session_id(transport: &Option<serde_json::Value>) -> Option<&str> {
	transport.as_ref().and_then(|t| t.get("session_id")).and_then(|v| v.as_str())
}

struct MigrationState {
	reconnect_url: Option<String>,
	ws2: Option<TwitchWs>,
	last_activity_ws2: Instant,
}

#[derive(Debug)]
struct BackpressureState {
	dropped_ingest: u64,
	dropped_ingest_since_report: u64,
	last_report: Instant,
}

/// Twitch EventSub adapter configuration.
#[derive(Clone)]
pub struct TwitchConfig {
	pub client_id: String,
	pub client_secret: Option<SecretString>,
	pub disable_refresh: bool,
	pub user_access_token: SecretString,
	pub refresh_token: Option<SecretString>,
	pub broadcaster_id_overrides: HashMap<String, String>,
	pub eventsub_ws_url: String,
	pub helix_base_url: String,
	pub reconnect_min_delay: Duration,
	pub reconnect_max_delay: Duration,
	pub refresh_buffer: Duration,
	pub migration_buffer_capacity: usize,
	pub ws_connector: Option<WsConnector>,
	pub mod_status_refresh_interval: Duration,
}

impl TwitchConfig {
	pub fn new(client_id: impl Into<String>, user_access_token: SecretString) -> Self {
		Self {
			client_id: client_id.into(),
			client_secret: None,
			disable_refresh: false,
			user_access_token,
			refresh_token: None,
			broadcaster_id_overrides: HashMap::new(),
			eventsub_ws_url: "wss://eventsub.wss.twitch.tv/ws".to_string(),
			helix_base_url: "https://api.twitch.tv".to_string(),
			reconnect_min_delay: Duration::from_millis(500),
			reconnect_max_delay: Duration::from_secs(30),
			refresh_buffer: Duration::from_secs(60),
			migration_buffer_capacity: 256,
			ws_connector: None,
			mod_status_refresh_interval: Duration::from_secs(60),
		}
	}
}

/// Twitch EventSub adapter.
pub struct TwitchEventSubAdapter {
	cfg: TwitchConfig,
	joined_rooms: HashSet<RoomKey>,
	broadcaster_id_by_room: HashMap<RoomKey, String>,
	token_user_id: Option<String>,
	subscription_id_by_room_and_type: HashMap<(RoomKey, TwitchSubscriptionType), String>,
	is_token_user_mod_by_room: HashMap<RoomKey, bool>,
	last_mod_status_refresh_by_room: HashMap<RoomKey, Instant>,
	auth_expires_at: Option<SystemTime>,
	last_auth_error_notice: Option<String>,
	last_refresh_attempt: Option<Instant>,
	helix_circuit_breaker: CircuitBreaker,
}

#[derive(Debug)]
struct CircuitBreaker {
	last_failure: Option<Instant>,
	failure_count: u32,
	state: CircuitBreakerState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CircuitBreakerState {
	Closed,
	Open,
	HalfOpen,
}

impl CircuitBreaker {
	fn new() -> Self {
		Self {
			last_failure: None,
			failure_count: 0,
			state: CircuitBreakerState::Closed,
		}
	}

	fn record_success(&mut self) {
		self.failure_count = 0;
		self.state = CircuitBreakerState::Closed;
	}

	fn record_failure(&mut self) {
		self.last_failure = Some(Instant::now());
		self.failure_count += 1;

		if self.failure_count >= 5 {
			self.state = CircuitBreakerState::Open;
		}
	}

	fn should_attempt(&mut self) -> bool {
		match self.state {
			CircuitBreakerState::Closed => true,
			CircuitBreakerState::Open => {
				if let Some(last_failure) = self.last_failure {
					if last_failure.elapsed() > Duration::from_secs(30) {
						self.state = CircuitBreakerState::HalfOpen;
						true
					} else {
						false
					}
				} else {
					false
				}
			}
			CircuitBreakerState::HalfOpen => true,
		}
	}
}

impl TwitchEventSubAdapter {
	pub fn new(cfg: TwitchConfig) -> Self {
		Self {
			cfg,
			joined_rooms: HashSet::new(),
			broadcaster_id_by_room: HashMap::new(),
			token_user_id: None,
			subscription_id_by_room_and_type: HashMap::new(),
			is_token_user_mod_by_room: HashMap::new(),
			last_mod_status_refresh_by_room: HashMap::new(),
			auth_expires_at: None,
			last_auth_error_notice: None,
			last_refresh_attempt: None,
			helix_circuit_breaker: CircuitBreaker::new(),
		}
	}

	fn trace(session_id: &str) -> IngestTrace {
		IngestTrace {
			session_id: Some(session_id.to_string()),
			..IngestTrace::default()
		}
	}

	fn topic_for_room(room: &RoomKey) -> String {
		format!("room:{}/{}", room.platform.as_str(), room.room_id.as_str())
	}

	fn ws_url_from_string(&self, s: &str) -> anyhow::Result<Url> {
		Url::parse(s).context("parse eventsub ws url")
	}

	fn helix_base_url(&self) -> anyhow::Result<Url> {
		Url::parse(&self.cfg.helix_base_url).context("parse helix_base_url")
	}

	fn helix_client(&self) -> anyhow::Result<HelixClient> {
		if self.cfg.client_id.trim().is_empty() {
			return Err(anyhow::anyhow!("missing twitch client_id"));
		}

		HelixClient::new(
			self.helix_base_url()?,
			self.cfg.client_id.clone(),
			self.cfg.user_access_token.expose().to_string(),
		)
	}

	fn has_auth(&self) -> bool {
		if self.cfg.client_id.trim().is_empty() || self.cfg.user_access_token.expose().trim().is_empty() {
			return false;
		}
		match self.auth_expires_at {
			Some(deadline) => SystemTime::now().duration_since(deadline).is_err(),
			None => true,
		}
	}

	fn apply_auth_update(&mut self, auth: AdapterAuth) {
		match auth {
			AdapterAuth::UserAccessToken {
				access_token,
				user_id: _,
				expires_in,
			} => {
				self.cfg.user_access_token = access_token;
				self.token_user_id = None;
				self.is_token_user_mod_by_room.clear();
				self.last_mod_status_refresh_by_room.clear();
				self.auth_expires_at = expires_in.and_then(|d| SystemTime::now().checked_add(d));
				self.last_auth_error_notice = None;
			}
			AdapterAuth::TwitchUser {
				client_id,
				access_token,
				user_id,
				username: _,
				expires_in,
			} => {
				self.cfg.client_id = client_id;
				self.cfg.user_access_token = access_token;
				self.token_user_id = user_id;
				self.auth_expires_at = expires_in
					.and_then(|d| SystemTime::now().checked_add(d))
					.or(self.auth_expires_at);
				self.last_auth_error_notice = None;

				self.is_token_user_mod_by_room.clear();
				self.last_mod_status_refresh_by_room.clear();
			}
			AdapterAuth::AppAccessToken { .. } => {}
			AdapterAuth::OpaqueJson(_) => {}
			AdapterAuth::None => {}
		}
	}

	fn invalidate_auth(&mut self, reason: &str, events_tx: &AdapterEventTx) {
		self.cfg.user_access_token = SecretString::new("");
		self.token_user_id = None;
		self.is_token_user_mod_by_room.clear();
		self.last_mod_status_refresh_by_room.clear();
		self.auth_expires_at = None;
		self.last_auth_error_notice = Some(reason.to_string());
		let _ = events_tx.try_send(status(Platform::Twitch, false, reason.to_string()));
	}

	fn maybe_notice_auth_issue(&mut self, reason: &str, events_tx: &AdapterEventTx) {
		if self.last_auth_error_notice.as_deref() != Some(reason) {
			self.last_auth_error_notice = Some(reason.to_string());
			let _ = events_tx.try_send(status(Platform::Twitch, false, reason.to_string()));
		}
	}

	async fn refresh_auth_if_needed(&mut self, events_tx: &AdapterEventTx) -> bool {
		if self.cfg.disable_refresh {
			return false;
		}

		let Some(expires_at) = self.auth_expires_at else {
			return false;
		};

		let buffer = self.cfg.refresh_buffer;
		let should_refresh = match SystemTime::now().checked_add(buffer) {
			Some(next) => next >= expires_at,
			None => true,
		};

		if !should_refresh {
			return false;
		}

		let Some(client_secret) = self.cfg.client_secret.as_ref() else {
			self.maybe_notice_auth_issue("token expiring; missing client_secret for refresh", events_tx);
			return false;
		};
		let Some(refresh_token) = self.cfg.refresh_token.as_ref() else {
			self.maybe_notice_auth_issue("token expiring; missing refresh_token", events_tx);
			return false;
		};

		if let Some(last) = self.last_refresh_attempt
			&& last.elapsed() < Duration::from_secs(30)
		{
			return false;
		}
		self.last_refresh_attempt = Some(Instant::now());

		match refresh_user_token(&self.cfg.client_id, client_secret.expose(), refresh_token.expose()).await {
			Ok(resp) => {
				self.cfg.user_access_token = SecretString::new(resp.access_token);
				if let Some(new_refresh) = resp.refresh_token {
					self.cfg.refresh_token = Some(SecretString::new(new_refresh));
				}
				self.auth_expires_at = SystemTime::now().checked_add(Duration::from_secs(resp.expires_in));
				self.last_auth_error_notice = None;
				let _ = events_tx.try_send(status(Platform::Twitch, true, "refreshed user OAuth token".to_string()));
				true
			}
			Err(e) => {
				self.maybe_notice_auth_issue(&format!("token refresh failed: {e}"), events_tx);
				false
			}
		}
	}

	fn is_helix_auth_error(err: &anyhow::Error) -> bool {
		err.to_string().to_ascii_lowercase().contains("helix auth failed")
	}

	fn categorize_helix_error(err: &anyhow::Error) -> HelixErrorCategory {
		let err_str = err.to_string().to_ascii_lowercase();
		if err_str.contains("helix auth failed") {
			HelixErrorCategory::Auth
		} else if err_str.contains("too many requests") || err_str.contains("rate limit") {
			HelixErrorCategory::RateLimit
		} else if err_str.contains("conflict") {
			HelixErrorCategory::Conflict
		} else if err_str.contains("not found") || err_str.contains("404") {
			HelixErrorCategory::NotFound
		} else if err_str.contains("bad request") || err_str.contains("400") {
			HelixErrorCategory::BadRequest
		} else if err_str.contains("internal server error") || err_str.contains("500") {
			HelixErrorCategory::ServerError
		} else {
			HelixErrorCategory::Other
		}
	}

	fn backoff_delay(attempt: u32, min: Duration, max: Duration) -> Duration {
		let pow = attempt.min(16);
		let ms = min.as_millis().saturating_mul(1u128 << pow);
		let d = Duration::from_millis(ms.min(u64::MAX as u128) as u64);
		d.min(max).max(min)
	}

	async fn connect_eventsub_ws(url: Url) -> anyhow::Result<TwitchWs> {
		let (ws, _resp) = tokio_tungstenite::connect_async(url.as_str())
			.await
			.context("connect_async to eventsub ws")?;
		Ok(ws)
	}

	fn ws_connector(&self) -> WsConnector {
		if let Some(c) = &self.cfg.ws_connector {
			return c.clone();
		}

		Arc::new(|url: Url| {
			Box::pin(async move { Self::connect_eventsub_ws(url).await }) as BoxFuture<'static, anyhow::Result<TwitchWs>>
		})
	}

	async fn connect_ws(&self, url: Url) -> anyhow::Result<TwitchWs> {
		(self.ws_connector())(url).await
	}

	async fn execute_command(&mut self, request: CommandRequest, _auth: Option<AdapterAuth>) -> Result<(), CommandError> {
		let room = match &request {
			CommandRequest::SendChat { room, .. }
			| CommandRequest::DeleteMessage { room, .. }
			| CommandRequest::TimeoutUser { room, .. }
			| CommandRequest::BanUser { room, .. } => room.clone(),
		};

		if room.platform != Platform::Twitch {
			return Err(CommandError::InvalidTopic(None));
		}
		if !self.has_auth() {
			return Err(CommandError::NotAuthorized(Some(
				"twitch auth missing access token".to_string(),
			)));
		}
		let broadcaster_id = self
			.resolve_broadcaster_id(&room)
			.await
			.map_err(|e| CommandError::Internal(format!("twitch {e}")))?;
		let token_user_id = self
			.resolve_token_user_id()
			.await
			.map_err(|e| CommandError::Internal(format!("twitch {e}")))?;
		let helix = self
			.helix_client()
			.map_err(|e| CommandError::Internal(format!("twitch {e}")))?;

		match request {
			CommandRequest::SendChat {
				text,
				reply_to_platform_message_id,
				..
			} => helix
				.send_chat_message(
					&broadcaster_id,
					&token_user_id,
					&text,
					reply_to_platform_message_id.as_deref(),
				)
				.await
				.map_err(|e| CommandError::Internal(format!("twitch {e}"))),
			CommandRequest::DeleteMessage { platform_message_id, .. } => {
				let is_mod = self.refresh_mod_status_if_needed(&room).await;
				if !is_mod && token_user_id != broadcaster_id {
					return Err(CommandError::NotAuthorized(Some(
						"twitch moderator or broadcaster required".to_string(),
					)));
				}

				helix
					.delete_chat_message(&broadcaster_id, &token_user_id, &platform_message_id)
					.await
					.map_err(|e| CommandError::Internal(format!("twitch {e}")))
			}
			CommandRequest::TimeoutUser {
				user_id,
				duration_seconds,
				reason,
				..
			} => {
				let is_mod = self.refresh_mod_status_if_needed(&room).await;
				if !is_mod && token_user_id != broadcaster_id {
					return Err(CommandError::NotAuthorized(Some(
						"twitch moderator or broadcaster required".to_string(),
					)));
				}

				helix
					.ban_user(
						&broadcaster_id,
						&token_user_id,
						&user_id,
						Some(duration_seconds),
						reason.as_deref(),
					)
					.await
					.map_err(|e| CommandError::Internal(format!("twitch {e}")))
			}
			CommandRequest::BanUser { user_id, reason, .. } => {
				let is_mod = self.refresh_mod_status_if_needed(&room).await;
				if !is_mod && token_user_id != broadcaster_id {
					return Err(CommandError::NotAuthorized(Some(
						"twitch moderator or broadcaster required".to_string(),
					)));
				}

				helix
					.ban_user(&broadcaster_id, &token_user_id, &user_id, None, reason.as_deref())
					.await
					.map_err(|e| CommandError::Internal(format!("twitch {e}")))
			}
		}
	}

	async fn read_until_welcome(ws: &mut TwitchWs) -> anyhow::Result<eventsub::EventSubWelcomeSession> {
		loop {
			let Some(msg) = ws.next().await else {
				return Err(anyhow::anyhow!("ws closed before welcome"));
			};
			let msg = msg.context("ws read")?;

			match msg {
				Message::Text(t) => {
					let ty = eventsub::peek_message_type(&t)?;
					if ty == "session_welcome" {
						let welcome = eventsub::parse_welcome(&t)?;
						return Ok(welcome.payload.session);
					}
				}
				Message::Ping(p) => {
					let _ = ws.send(Message::Pong(p)).await;
				}
				Message::Close(c) => {
					anyhow::bail!("ws closed before welcome: close={c:?}");
				}
				_ => {}
			}
		}
	}

	async fn resolve_token_user_id(&mut self) -> anyhow::Result<String> {
		if let Some(id) = &self.token_user_id {
			return Ok(id.clone());
		}
		let helix = self.helix_client()?;
		let u = helix.get_token_user().await?;
		self.token_user_id = Some(u.id.clone());
		Ok(u.id)
	}

	async fn refresh_mod_status_if_needed(&mut self, room: &RoomKey) -> bool {
		let now = Instant::now();

		let should_refresh = self
			.last_mod_status_refresh_by_room
			.get(room)
			.map(|last| now.duration_since(*last) >= self.cfg.mod_status_refresh_interval)
			.unwrap_or(true);

		if !should_refresh {
			return *self.is_token_user_mod_by_room.get(room).unwrap_or(&false);
		}

		self.last_mod_status_refresh_by_room.insert(room.clone(), now);

		let broadcaster_id = match self.resolve_broadcaster_id(room).await {
			Ok(id) => id,
			Err(e) => {
				debug!(%room, error=%e, "failed to resolve broadcaster id; keeping previous mod cache value");
				return *self.is_token_user_mod_by_room.get(room).unwrap_or(&false);
			}
		};

		let token_user_id = match self.resolve_token_user_id().await {
			Ok(id) => id,
			Err(e) => {
				debug!(%room, error=%e, "failed to resolve token user id; keeping previous mod cache value");
				return *self.is_token_user_mod_by_room.get(room).unwrap_or(&false);
			}
		};

		let helix = match self.helix_client() {
			Ok(h) => h,
			Err(e) => {
				debug!(%room, error=%e, "failed to build helix client; keeping previous mod cache value");
				return *self.is_token_user_mod_by_room.get(room).unwrap_or(&false);
			}
		};

		match helix.is_user_moderator_in_channel(&broadcaster_id, &token_user_id).await {
			Ok(is_mod) => {
				self.is_token_user_mod_by_room.insert(room.clone(), is_mod);
				is_mod
			}
			Err(e) => {
				debug!(%room, error=%e, "failed to refresh mod status; keeping previous mod cache value");
				*self.is_token_user_mod_by_room.get(room).unwrap_or(&false)
			}
		}
	}

	async fn resolve_broadcaster_id(&mut self, room: &RoomKey) -> anyhow::Result<String> {
		let login = room.room_id.as_str().to_string();

		if let Some(id) = self.cfg.broadcaster_id_overrides.get(&login) {
			return Ok(id.clone());
		}
		if let Some(id) = self.broadcaster_id_by_room.get(room) {
			return Ok(id.clone());
		}

		let helix = self.helix_client()?;
		let u = helix
			.get_user_by_login(login.as_str())
			.await?
			.with_context(|| format!("no helix user for broadcaster login={login}"))?;

		self.broadcaster_id_by_room.insert(room.clone(), u.id.clone());
		Ok(u.id)
	}

	async fn ensure_subscription_for_room(&mut self, session_id: &str, room: &RoomKey) -> anyhow::Result<()> {
		let perms = self.permissions_for_room(room).await;
		let can_moderate = perms.is_moderator || perms.is_broadcaster;

		for sub_type in [
			TwitchSubscriptionType::ChatMessage,
			TwitchSubscriptionType::ChatMessageDelete,
			TwitchSubscriptionType::ChannelBan,
			TwitchSubscriptionType::ChannelModerate,
			TwitchSubscriptionType::ChannelRaid,
			TwitchSubscriptionType::ChannelCheer,
			TwitchSubscriptionType::ChannelSubscribe,
		] {
			if matches!(
				sub_type,
				TwitchSubscriptionType::ChannelBan
					| TwitchSubscriptionType::ChannelModerate
					| TwitchSubscriptionType::ChatMessageDelete
			) && !can_moderate
			{
				debug!(room=%room, sub_type=?sub_type, "skipping subscription; missing moderator/broadcaster permissions");
				continue;
			}

			self.ensure_subscription_for_room_and_type(session_id, room, sub_type).await?;
		}

		Ok(())
	}

	async fn permissions_for_room(&mut self, room: &RoomKey) -> PermissionsInfo {
		if room.platform != Platform::Twitch {
			return PermissionsInfo::default();
		}
		if !self.has_auth() {
			return PermissionsInfo::default();
		}
		let broadcaster_id = match self.resolve_broadcaster_id(room).await {
			Ok(id) => id,
			Err(_) => return PermissionsInfo::default(),
		};
		let token_user_id = match self.resolve_token_user_id().await {
			Ok(id) => id,
			Err(_) => return PermissionsInfo::default(),
		};
		let is_moderator = self.refresh_mod_status_if_needed(room).await;
		let is_broadcaster = token_user_id == broadcaster_id;
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

	async fn ensure_subscription_for_room_and_type(
		&mut self,
		session_id: &str,
		room: &RoomKey,
		sub_type: TwitchSubscriptionType,
	) -> anyhow::Result<()> {
		let key = (room.clone(), sub_type);
		if self.subscription_id_by_room_and_type.contains_key(&key) {
			return Ok(());
		}

		if !self.helix_circuit_breaker.should_attempt() {
			anyhow::bail!("helix circuit breaker is open; skipping subscription creation");
		}

		let broadcaster_user_id = self.resolve_broadcaster_id(room).await?;
		let user_id = self.resolve_token_user_id().await?;

		let helix = self.helix_client()?;

		for attempt in 0..2 {
			let created: anyhow::Result<HelixCreateSubscriptionResponse> = match sub_type {
				TwitchSubscriptionType::ChatMessage => helix
					.create_chat_message_subscription(session_id, &broadcaster_user_id, &user_id)
					.await
					.with_context(|| format!("create subscription type={} room={room}", sub_type.as_helix_type())),

				TwitchSubscriptionType::ChatMessageDelete => helix
					.create_chat_message_delete_subscription(session_id, &broadcaster_user_id, &user_id)
					.await
					.with_context(|| format!("create subscription type={} room={room}", sub_type.as_helix_type())),

				TwitchSubscriptionType::ChannelBan => helix
					.create_channel_ban_subscription(session_id, &broadcaster_user_id)
					.await
					.with_context(|| format!("create subscription type={} room={room}", sub_type.as_helix_type())),

				TwitchSubscriptionType::ChannelModerate => helix
					.create_channel_moderate_subscription(session_id, &broadcaster_user_id, &user_id)
					.await
					.with_context(|| format!("create subscription type={} room={room}", sub_type.as_helix_type())),

				TwitchSubscriptionType::ChannelRaid => helix
					.create_channel_raid_to_subscription(session_id, &broadcaster_user_id)
					.await
					.with_context(|| format!("create subscription type={} room={room}", sub_type.as_helix_type())),

				TwitchSubscriptionType::ChannelCheer => helix
					.create_channel_cheer_subscription(session_id, &broadcaster_user_id)
					.await
					.with_context(|| format!("create subscription type={} room={room}", sub_type.as_helix_type())),

				TwitchSubscriptionType::ChannelSubscribe => helix
					.create_channel_subscribe_subscription(session_id, &broadcaster_user_id)
					.await
					.with_context(|| format!("create subscription type={} room={room}", sub_type.as_helix_type())),
			};

			let created = match created {
				Ok(created) => {
					self.helix_circuit_breaker.record_success();
					created
				}
				Err(e) => {
					let category = Self::categorize_helix_error(&e);
					match category {
						HelixErrorCategory::Auth => {
							// Auth errors should trigger token refresh, not circuit breaker
							return Err(e);
						}
						HelixErrorCategory::RateLimit => {
							// Rate limits are handled by send_with_retry, but we should be cautious
							if attempt == 0 {
								tokio::time::sleep(Duration::from_secs(1)).await;
								continue;
							}
							self.helix_circuit_breaker.record_failure();
							return Err(e);
						}
						HelixErrorCategory::Conflict => {
							// Handle conflict by finding and reconciling existing subscriptions
							let subs = helix
								.list_all_eventsub_subscriptions_by_type(sub_type.as_helix_type())
								.await
								.context("list eventsub subscriptions for reconcile")?;

							let mut matching: Vec<HelixSubscriptionData> = subs
								.into_iter()
								.filter(|s| condition_matches(sub_type, &s.condition, &broadcaster_user_id, &user_id))
								.collect();

							let Some(existing) = matching.pop() else {
								return Err(e).context(
									"create subscription conflict but could not find existing subscription to reconcile",
								);
							};

							if let Some(existing_session_id) = transport_session_id(&existing.transport)
								&& existing_session_id == session_id
							{
								self.helix_circuit_breaker.record_success();
								self.subscription_id_by_room_and_type.insert(key, existing.id);
								return Ok(());
							}

							let mut to_delete = vec![existing];
							to_delete.extend(matching.into_iter());

							for sub in to_delete {
								let _ = helix.delete_subscription(&sub.id).await;
							}

							continue;
						}
						_ => {
							// Other errors might indicate service issues
							self.helix_circuit_breaker.record_failure();
							return Err(e);
						}
					}
				}
			};

			let sub = created
				.data
				.into_iter()
				.next()
				.context("helix create subscription returned empty data")?;

			self.subscription_id_by_room_and_type.insert(key, sub.id);
			return Ok(());
		}

		Err(anyhow::anyhow!(
			"failed to create twitch subscription after conflict reconciliation"
		))
	}

	async fn remove_subscription_for_room(&mut self, room: &RoomKey) -> anyhow::Result<()> {
		let helix = self.helix_client()?;

		for sub_type in [
			TwitchSubscriptionType::ChatMessage,
			TwitchSubscriptionType::ChatMessageDelete,
			TwitchSubscriptionType::ChannelBan,
			TwitchSubscriptionType::ChannelModerate,
			TwitchSubscriptionType::ChannelRaid,
			TwitchSubscriptionType::ChannelCheer,
			TwitchSubscriptionType::ChannelSubscribe,
		] {
			let key = (room.clone(), sub_type);
			let Some(sub_id) = self.subscription_id_by_room_and_type.remove(&key) else {
				continue;
			};
			helix.delete_subscription(&sub_id).await?;
		}

		Ok(())
	}

	async fn ensure_subscriptions_for_joined_rooms(&mut self, session_id: &str, events_tx: &AdapterEventTx) {
		let platform = Platform::Twitch;
		let rooms: Vec<RoomKey> = self.joined_rooms.iter().cloned().collect();

		for room in rooms {
			if room.platform != platform {
				continue;
			}
			if let Err(e) = self.ensure_subscription_for_room(session_id, &room).await {
				warn!(error = ?e, room=%room, "twitch ensure subscription failed");
				if Self::is_helix_auth_error(&e) {
					self.invalidate_auth("twitch auth failed; waiting for refreshed OAuth", events_tx);
					return;
				}

				let _ = events_tx.try_send(status_error(
					platform,
					format!("failed to ensure subscription for {}", Self::topic_for_room(&room)),
					e,
				));
			} else {
				let _ = events_tx.try_send(status(platform, true, format!("subscribed {}", Self::topic_for_room(&room))));
			}
		}
	}

	async fn handle_control_message(
		&mut self,
		cmd: AdapterControl,
		current_session_id: Option<&str>,
		events_tx: &AdapterEventTx,
	) {
		let platform = Platform::Twitch;

		match cmd {
			AdapterControl::Join { room } => {
				if room.platform != platform {
					debug!(%platform, room=%room, "ignoring Join for non-twitch room");
					return;
				}

				let inserted = self.joined_rooms.insert(room.clone());
				if inserted {
					let cache_key = format!("twitch:channel:{}:native", room.room_id.as_str());
					info!(%platform, room=%room.room_id, cache_key=%cache_key, "emitting AssetBundle ingest");
					let mut ingest = IngestEvent::new(
						platform,
						room.room_id.clone(),
						IngestPayload::AssetBundle(AssetBundle {
							provider: AssetProvider::Twitch,
							scope: AssetScope::Channel,
							cache_key: cache_key.clone(),
							etag: Some("empty".to_string()),
							emotes: Vec::new(),
							badges: Vec::new(),
						}),
					);
					ingest.trace.session_id = current_session_id.map(|s| s.to_string());
					let _ = events_tx.try_send(AdapterEvent::Ingest(Box::new(ingest)));

					let room_for_assets = room.clone();
					let events_tx_clone = events_tx.clone();
					let session_id = current_session_id.map(|s| s.to_string());
					let broadcaster_id = self.resolve_broadcaster_id(&room).await.ok();
					let client_id = self.cfg.client_id.clone();
					let bearer_token = self.cfg.user_access_token.expose().to_string();
					tokio::spawn(async move {
						if let Ok(bundle) = fetch_twitch_badges_bundle(&client_id, &bearer_token).await {
							info!(%platform, room=%room_for_assets.room_id, cache_key=%bundle.cache_key, "emitting AssetBundle ingest");
							let mut ingest = IngestEvent::new(
								Platform::Twitch,
								room_for_assets.room_id.clone(),
								IngestPayload::AssetBundle(bundle),
							);
							ingest.trace.session_id = session_id.clone();
							let _ = events_tx_clone.try_send(AdapterEvent::Ingest(Box::new(ingest)));
						}

						if let Ok(bundle) = fetch_twitch_global_emotes_bundle(&client_id, &bearer_token).await {
							info!(%platform, room=%room_for_assets.room_id, cache_key=%bundle.cache_key, "emitting AssetBundle ingest");
							let mut ingest = IngestEvent::new(
								Platform::Twitch,
								room_for_assets.room_id.clone(),
								IngestPayload::AssetBundle(bundle),
							);
							ingest.trace.session_id = session_id.clone();
							let _ = events_tx_clone.try_send(AdapterEvent::Ingest(Box::new(ingest)));
						}

						if let Ok(bundle) = fetch_ffz_bundle(room_for_assets.room_id.as_str()).await {
							info!(%platform, room=%room_for_assets.room_id, cache_key=%bundle.cache_key, "emitting AssetBundle ingest");
							let mut ingest = IngestEvent::new(
								Platform::Twitch,
								room_for_assets.room_id.clone(),
								IngestPayload::AssetBundle(bundle),
							);
							ingest.trace.session_id = session_id.clone();
							let _ = events_tx_clone.try_send(AdapterEvent::Ingest(Box::new(ingest)));
						}

						if let Ok(bundle) = fetch_ffz_global_emotes_bundle().await {
							info!(%platform, room=%room_for_assets.room_id, cache_key=%bundle.cache_key, "emitting AssetBundle ingest");
							let mut ingest = IngestEvent::new(
								Platform::Twitch,
								room_for_assets.room_id.clone(),
								IngestPayload::AssetBundle(bundle),
							);
							ingest.trace.session_id = session_id.clone();
							let _ = events_tx_clone.try_send(AdapterEvent::Ingest(Box::new(ingest)));
						}

						if let Ok(bundle) = fetch_bttv_global_emotes_bundle().await {
							info!(%platform, room=%room_for_assets.room_id, cache_key=%bundle.cache_key, "emitting AssetBundle ingest");
							let mut ingest = IngestEvent::new(
								Platform::Twitch,
								room_for_assets.room_id.clone(),
								IngestPayload::AssetBundle(bundle),
							);
							ingest.trace.session_id = session_id.clone();
							let _ = events_tx_clone.try_send(AdapterEvent::Ingest(Box::new(ingest)));
						}

						if let Ok(bundle) = fetch_bttv_badges_bundle("twitch").await {
							info!(%platform, room=%room_for_assets.room_id, cache_key=%bundle.cache_key, "emitting AssetBundle ingest");
							let mut ingest = IngestEvent::new(
								Platform::Twitch,
								room_for_assets.room_id.clone(),
								IngestPayload::AssetBundle(bundle),
							);
							ingest.trace.session_id = session_id.clone();
							let _ = events_tx_clone.try_send(AdapterEvent::Ingest(Box::new(ingest)));
						}

						if let Ok(bundle) = fetch_ffz_badges_bundle().await {
							info!(%platform, room=%room_for_assets.room_id, cache_key=%bundle.cache_key, "emitting AssetBundle ingest");
							let mut ingest = IngestEvent::new(
								Platform::Twitch,
								room_for_assets.room_id.clone(),
								IngestPayload::AssetBundle(bundle),
							);
							ingest.trace.session_id = session_id.clone();
							let _ = events_tx_clone.try_send(AdapterEvent::Ingest(Box::new(ingest)));
						}

						if let Ok(bundle) = fetch_7tv_badges_bundle().await {
							info!(%platform, room=%room_for_assets.room_id, cache_key=%bundle.cache_key, etag=?bundle.etag, "fetched 7tv global badges bundle");
							let mut ingest = IngestEvent::new(
								Platform::Twitch,
								room_for_assets.room_id.clone(),
								IngestPayload::AssetBundle(bundle),
							);
							ingest.trace.session_id = session_id.clone();
							let _ = events_tx_clone.try_send(AdapterEvent::Ingest(Box::new(ingest)));
						}

						if let Some(id) = broadcaster_id {
							if let Ok(bundle) = fetch_twitch_channel_badges_bundle(&client_id, &bearer_token, &id).await {
								info!(%platform, room=%room_for_assets.room_id, broadcaster_id=%id, cache_key=%bundle.cache_key, "emitting AssetBundle ingest");
								let mut ingest = IngestEvent::new(
									Platform::Twitch,
									room_for_assets.room_id.clone(),
									IngestPayload::AssetBundle(bundle),
								);
								ingest.trace.session_id = session_id.clone();
								let _ = events_tx_clone.try_send(AdapterEvent::Ingest(Box::new(ingest)));
							}

							if let Ok(bundle) = fetch_twitch_channel_emotes_bundle(&client_id, &bearer_token, &id).await {
								info!(%platform, room=%room_for_assets.room_id, broadcaster_id=%id, cache_key=%bundle.cache_key, "emitting AssetBundle ingest");
								let mut ingest = IngestEvent::new(
									Platform::Twitch,
									room_for_assets.room_id.clone(),
									IngestPayload::AssetBundle(bundle),
								);
								ingest.trace.session_id = session_id.clone();
								let _ = events_tx_clone.try_send(AdapterEvent::Ingest(Box::new(ingest)));
							}

							if let Ok(bundle) = fetch_bttv_bundle("twitch", &id).await {
								info!(%platform, room=%room_for_assets.room_id, broadcaster_id=%id, cache_key=%bundle.cache_key, "emitting AssetBundle ingest");
								let mut ingest = IngestEvent::new(
									Platform::Twitch,
									room_for_assets.room_id.clone(),
									IngestPayload::AssetBundle(bundle),
								);
								ingest.trace.session_id = session_id.clone();
								let _ = events_tx_clone.try_send(AdapterEvent::Ingest(Box::new(ingest)));
							}

							info!(%platform, room=%room_for_assets.room_id, broadcaster_id=%id, "fetching 7tv channel badges bundle");
							match fetch_7tv_channel_badges_bundle(SevenTvPlatform::Twitch, &id).await {
								Ok(bundle) => {
									info!(%platform, room=%room_for_assets.room_id, broadcaster_id=%id, cache_key=%bundle.cache_key, "emitting AssetBundle ingest");
									let mut ingest = IngestEvent::new(
										Platform::Twitch,
										room_for_assets.room_id.clone(),
										IngestPayload::AssetBundle(bundle),
									);
									ingest.trace.session_id = session_id.clone();
									let _ = events_tx_clone.try_send(AdapterEvent::Ingest(Box::new(ingest)));
								}
								Err(error) => {
									info!(%platform, room=%room_for_assets.room_id, broadcaster_id=%id, error=?error, "failed to fetch 7tv channel badges bundle");
								}
							}

							info!(%platform, room=%room_for_assets.room_id, broadcaster_id=%id, "fetching 7tv emote set bundle");
							match fetch_7tv_bundle(SevenTvPlatform::Twitch, &id).await {
								Ok(bundle) => {
									info!(%platform, room=%room_for_assets.room_id, broadcaster_id=%id, cache_key=%bundle.cache_key, "emitting AssetBundle ingest");
									let mut ingest = IngestEvent::new(
										Platform::Twitch,
										room_for_assets.room_id.clone(),
										IngestPayload::AssetBundle(bundle),
									);
									ingest.trace.session_id = session_id.clone();
									let _ = events_tx_clone.try_send(AdapterEvent::Ingest(Box::new(ingest)));
								}
								Err(error) => {
									info!(%platform, room=%room_for_assets.room_id, broadcaster_id=%id, error=?error, "failed to fetch 7tv emote set bundle");
								}
							}
						}
					});
					let _ = events_tx.try_send(status(
						platform,
						true,
						format!("requested join {}", Self::topic_for_room(&room)),
					));
				}

				if let Some(session_id) = current_session_id
					&& let Err(e) = self.ensure_subscription_for_room(session_id, &room).await
				{
					if Self::is_helix_auth_error(&e) {
						self.invalidate_auth("twitch auth failed; waiting for refreshed OAuth", events_tx);
						return;
					}

					let _ = events_tx.try_send(status_error(
						platform,
						format!("failed to ensure subscription for {}", Self::topic_for_room(&room)),
						e,
					));
				}
			}

			AdapterControl::Leave { room } => {
				if room.platform != platform {
					debug!(%platform, room=%room, "ignoring Leave for non-twitch room");
					return;
				}

				let removed = self.joined_rooms.remove(&room);
				if removed {
					let _ = events_tx.try_send(status(
						platform,
						true,
						format!("requested leave {}", Self::topic_for_room(&room)),
					));
				}

				if let Err(e) = self.remove_subscription_for_room(&room).await {
					let _ = events_tx.try_send(status_error(
						platform,
						format!("failed to delete subscription for {}", Self::topic_for_room(&room)),
						e,
					));
				}
			}

			AdapterControl::UpdateAuth { auth } => {
				self.apply_auth_update(auth);
				let _ = events_tx.try_send(status(platform, true, "updated auth (bearer token)"));
			}

			AdapterControl::Command { request, auth, resp } => {
				let result = self.execute_command(request, auth).await;
				let _ = resp.send(result);
			}

			AdapterControl::QueryPermissions { room, auth: _, resp } => {
				let result = self.permissions_for_room(&room).await;
				let _ = resp.send(result);
			}

			AdapterControl::Shutdown => {}
		}
	}

	fn ingest_from_normalized_chat(
		&self,
		session_id: &str,
		n: eventsub::NormalizedChatNotification,
	) -> anyhow::Result<IngestEvent> {
		let mut ingest = IngestEvent::new(
			Platform::Twitch,
			n.room.room_id.clone(),
			IngestPayload::ChatMessage(ChatMessage {
				ids: IngestMessageIds {
					server_id: uuid::Uuid::new_v4(),
					platform_id: Some(n.platform_message_id.clone().into_string()),
				},
				author: crate::UserRef {
					id: n.chatter_user_id,
					login: n.chatter_user_login,
					display: Some(n.chatter_user_name),
				},
				text: n.text,
				badges: n.badge_ids,
				emotes: n.emotes.clone(),
			}),
		);

		ingest.room = n.room;
		ingest.ingest_time = SystemTime::now();
		ingest.platform_time = Some(n.platform_time);
		ingest.platform_message_id = Some(n.platform_message_id);

		let mut trace = Self::trace(session_id);
		trace.fields.insert("twitch_ws_message_id".to_string(), n.ws_message_id);
		trace.fields.insert("twitch_subscription_id".to_string(), n.subscription_id);
		ingest.trace = trace;

		Ok(ingest)
	}

	async fn run_loop(mut self, mut control_rx: AdapterControlRx, events_tx: AdapterEventTx) -> anyhow::Result<()> {
		let platform = Platform::Twitch;
		let adapter_session_id = new_session_id();

		let _ = events_tx.try_send(status(
			platform,
			true,
			format!("twitch adapter starting (session_id={adapter_session_id})"),
		));

		let mut reconnect_attempt: u32 = 0;
		let mut current_ws_url = self.cfg.eventsub_ws_url.clone();

		'outer: loop {
			while !self.has_auth() {
				if self.refresh_auth_if_needed(&events_tx).await {
					continue;
				}

				if let Some(deadline) = self.auth_expires_at {
					if SystemTime::now().duration_since(deadline).is_ok() {
						if self.cfg.disable_refresh {
							let msg = "user OAuth expired; refresh disabled";
							let _ = events_tx.try_send(status_error(platform, msg.to_string(), anyhow::anyhow!(msg)));
							return Err(anyhow::anyhow!(msg));
						}

						self.maybe_notice_auth_issue("user OAuth expired; waiting for refresh", &events_tx);
					} else {
						self.maybe_notice_auth_issue("waiting for user OAuth (client_id + token)", &events_tx);
					}
				} else {
					self.maybe_notice_auth_issue("waiting for user OAuth (client_id + token)", &events_tx);
				}

				match control_rx.recv().await {
					Some(cmd) => {
						self.handle_control_message(cmd, None, &events_tx).await;
					}
					None => return Ok(()),
				}
			}

			if self.joined_rooms.is_empty() {
				let _ = events_tx.try_send(status(
					platform,
					false,
					"no joined rooms; deferring eventsub connect".to_string(),
				));

				match tokio::time::timeout(Duration::from_secs(15), control_rx.recv()).await {
					Ok(Some(cmd)) => {
						if matches!(cmd, AdapterControl::Shutdown) {
							info!(%platform, "twitch adapter received Shutdown");
							break 'outer;
						}
						self.handle_control_message(cmd, None, &events_tx).await;
						continue;
					}
					Ok(None) => return Ok(()),
					Err(_) => {
						continue;
					}
				}
			}

			let ws_url = match self.ws_url_from_string(&current_ws_url) {
				Ok(u) => u,
				Err(e) => {
					let _ = events_tx.try_send(status_error(
						platform,
						format!("invalid eventsub ws url: {current_ws_url}"),
						e,
					));
					sleep(self.cfg.reconnect_min_delay).await;
					continue;
				}
			};

			let delay = if reconnect_attempt == 0 {
				Duration::from_millis(0)
			} else {
				Self::backoff_delay(reconnect_attempt, self.cfg.reconnect_min_delay, self.cfg.reconnect_max_delay)
			};

			if delay > Duration::from_millis(0) {
				let _ = events_tx.try_send(status(
					platform,
					false,
					format!("reconnecting in {:?} (attempt={reconnect_attempt})", delay),
				));
				sleep(delay).await;
			}

			let mut ws: TwitchWs = match self.connect_ws(ws_url.clone()).await {
				Ok(ws) => ws,
				Err(e) => {
					reconnect_attempt = reconnect_attempt.saturating_add(1);
					let _ = events_tx.try_send(status_error(platform, "failed to connect eventsub ws", e));
					continue;
				}
			};

			let welcome = match Self::read_until_welcome(&mut ws).await {
				Ok(w) => w,
				Err(e) => {
					reconnect_attempt = reconnect_attempt.saturating_add(1);
					let _ = events_tx.try_send(status_error(platform, "failed to read session_welcome", e));
					continue;
				}
			};

			reconnect_attempt = 0;

			let mut session_id = welcome.id.clone();
			let mut keepalive_secs = welcome.keepalive_timeout_seconds.unwrap_or(10);
			let mut keepalive_timeout = Duration::from_secs(keepalive_secs);

			let _ = events_tx.try_send(status(
				platform,
				true,
				format!("eventsub connected (session_id={session_id}, keepalive={keepalive_secs}s)"),
			));

			self.subscription_id_by_room_and_type.clear();
			self.ensure_subscriptions_for_joined_rooms(&session_id, &events_tx).await;

			let mut last_activity_main = Instant::now();

			let mut migrating: Option<MigrationState> = None;
			let mut buffered_secondary: VecDeque<String> = VecDeque::new();

			let mut backpressure = BackpressureState {
				dropped_ingest: 0,
				dropped_ingest_since_report: 0,
				last_report: Instant::now(),
			};
			let backpressure_report_interval = Duration::from_secs(5);

			loop {
				let mig_should_connect = migrating
					.as_ref()
					.is_some_and(|m| m.reconnect_url.is_some() && m.ws2.is_none());
				let mig_is_connected = migrating.as_ref().is_some_and(|m| m.ws2.is_some());

				tokio::select! {
					Some(cmd) = control_rx.recv() => {
						if matches!(cmd, AdapterControl::Shutdown) {
							info!(%platform, "twitch adapter received Shutdown");
							break 'outer;
						}
						self.handle_control_message(cmd, Some(&session_id), &events_tx).await;

						if self.joined_rooms.is_empty() {
							let _ = events_tx.try_send(status(platform, false, "no joined rooms; closing eventsub socket".to_string()));
							let _ = ws.close(None).await;
							current_ws_url = self.cfg.eventsub_ws_url.clone();
							break;
						}
					}

					msg = ws.next() => {
						let Some(msg) = msg else {
							let _ = events_tx.try_send(status(platform, false, "eventsub ws ended"));
							current_ws_url = self.cfg.eventsub_ws_url.clone();
							break;
						};

						let msg = match msg {
							Ok(m) => m,
							Err(e) => {
								let _ = events_tx.try_send(status_error(platform, "eventsub ws read error", e));
								current_ws_url = self.cfg.eventsub_ws_url.clone();
								break;
							}
						};

						match msg {
							Message::Text(t) => {
								last_activity_main = Instant::now();

								if let Ok(peek) = serde_json::from_str::<eventsub::EventSubMetadataPeek>(&t) {
									match peek.metadata.message_type.as_str() {
										"session_keepalive" => {
											debug!(%platform, "eventsub keepalive");
										}
										"session_reconnect" => {
											if migrating.is_none()
												&& let Ok(reconnect_msg) = eventsub::parse_reconnect(&t)
											{
												let url = reconnect_msg.payload.session.reconnect_url;
												let _ = events_tx.try_send(status(platform, true, "received session_reconnect; starting migration"));

												migrating = Some(MigrationState {
													reconnect_url: Some(url),
													ws2: None,
													last_activity_ws2: Instant::now(),
												});
												buffered_secondary.clear();
											}
										}
										"notification" => {
											let now = SystemTime::now();

											match notifications::handle_notification_json(&t, &session_id, now) {
												Ok((room_for_mod_check, events)) => {
													let token_user_is_mod = match room_for_mod_check.as_ref() {
														Some(room) => self.refresh_mod_status_if_needed(room).await,
														None => false,
													};

													for ev in events {
														let should_emit = match &ev {
															AdapterEvent::Ingest(ing) => {
																notifications::should_emit_payload(token_user_is_mod, &ing.payload)
															}
															_ => true,
														};

														if !should_emit {
															continue;
														}

														if events_tx.try_send(ev).is_err() {
															backpressure.dropped_ingest = backpressure.dropped_ingest.saturating_add(1);
															backpressure.dropped_ingest_since_report =
																backpressure.dropped_ingest_since_report.saturating_add(1);
														}
													}
												}
												Err(e) => {
													let _ = events_tx.try_send(status_error(platform, "failed to handle twitch notification", e));
												}
											}
										}
										_ => {}
									}
								}
							}

							Message::Ping(p) => {
								last_activity_main = Instant::now();
								let _ = ws.send(Message::Pong(p)).await;
							}

							Message::Pong(_) => {
								last_activity_main = Instant::now();
							}

							Message::Close(frame) => {
								let _ = events_tx.try_send(status(platform, false, format!("eventsub ws closed: {frame:?}")));
								current_ws_url = self.cfg.eventsub_ws_url.clone();
								break;
							}

							_ => {}
						}
					}

					_ = sleep(Duration::from_millis(0)), if mig_should_connect => {
						let reconnect_url = migrating
							.as_mut()
							.and_then(|m| m.reconnect_url.take())
							.expect("mig_should_connect implies reconnect_url Some");

						let url = match self.ws_url_from_string(&reconnect_url) {
							Ok(u) => u,
							Err(e) => {
								let _ = events_tx.try_send(status_error(platform, format!("invalid reconnect_url: {reconnect_url}"), e));
								current_ws_url = self.cfg.eventsub_ws_url.clone();
								break;
							}
						};

						match self.connect_ws(url).await {
							Ok(new_ws) => {
								let _ = events_tx.try_send(status(platform, true, "migration: connected to reconnect_url; waiting for welcome"));
								if let Some(m) = &mut migrating {
									m.ws2 = Some(new_ws);
									m.last_activity_ws2 = Instant::now();
								}
							}
							Err(e) => {
								let _ = events_tx.try_send(status_error(platform, "migration: failed to connect to reconnect_url", e));
								current_ws_url = self.cfg.eventsub_ws_url.clone();
								break;
							}
						}
					}

					msg2 = async {
						if let Some(m) = &mut migrating
							&& let Some(ws2) = &mut m.ws2
						{
							let next: Option<Result<Message, tokio_tungstenite::tungstenite::Error>> = ws2.next().await;
							return next;
						}
						None
					}, if mig_is_connected => {
						let Some(msg2) = msg2 else {
							let _ = events_tx.try_send(status(platform, true, "migration: secondary socket ended; continuing on primary"));
							migrating = None;
							buffered_secondary.clear();
							continue;
						};

						let msg2 = match msg2 {
							Ok(m) => m,
							Err(e) => {
								let _ = events_tx.try_send(status_error(platform, "migration: secondary ws read error; continuing on primary", e));
								migrating = None;
								buffered_secondary.clear();
								continue;
							}
						};

						if let Some(m) = &mut migrating {
							match msg2 {
								Message::Text(t) => {
									m.last_activity_ws2 = Instant::now();

									let ty = eventsub::peek_message_type(&t).unwrap_or_default();

									if ty == "session_welcome" {
										let welcome2 = match eventsub::parse_welcome(&t) {
											Ok(w) => w,
											Err(e) => {
												let _ = events_tx.try_send(status_error(platform, "migration: failed to parse session_welcome on secondary", e));
												migrating = None;
												buffered_secondary.clear();
												continue;
											}
										};

										session_id = welcome2.payload.session.id;
										keepalive_secs = welcome2.payload.session.keepalive_timeout_seconds.unwrap_or(10);
										let new_keepalive_timeout = Duration::from_secs(keepalive_secs);
										keepalive_timeout = new_keepalive_timeout;

										let _ = ws.close(None).await;
										if let Some(new_primary) = m.ws2.take() {
											ws = new_primary;
										}
										last_activity_main = Instant::now();

										while let Some(raw) = buffered_secondary.pop_front() {
											match eventsub::try_normalize_channel_chat_message(&raw) {
												Ok(Some(n)) => {
													match self.ingest_from_normalized_chat(&session_id, n) {
														Ok(ingest) => {
															if events_tx.try_send(AdapterEvent::Ingest(Box::new(ingest))).is_err() {
																backpressure.dropped_ingest = backpressure.dropped_ingest.saturating_add(1);
																backpressure.dropped_ingest_since_report = backpressure.dropped_ingest_since_report.saturating_add(1);
															}
														}
														Err(e) => { let _ = events_tx.try_send(status_error(platform, "failed to normalize buffered chat notification", e)); }
													}
												}
												Err(e) => {
													let _ = events_tx.try_send(status_error(platform, "failed to parse buffered notification", e));
												}
												_ => {}
											}
										}

										migrating = None;
										buffered_secondary.clear();

									} else if ty == "notification" {
										if buffered_secondary.len() >= self.cfg.migration_buffer_capacity {
											let _ = buffered_secondary.pop_front();
										}
										buffered_secondary.push_back(t.to_string());
									} else if ty == "session_keepalive" {
									}
								}
								Message::Ping(p) => {
									m.last_activity_ws2 = Instant::now();
									if let Some(ws2) = &mut m.ws2 {
										let _ = ws2.send(Message::Pong(p)).await;
									}
								}
								Message::Pong(_) => {
									m.last_activity_ws2 = Instant::now();
								}
								Message::Close(frame) => {
									let _ = events_tx.try_send(status(
										platform,
										true,
										format!("migration: secondary closed: {frame:?}; continuing on primary"),
									));
									migrating = None;
									buffered_secondary.clear();
								}
								Message::Binary(_) | Message::Frame(_) => {
								}
							}
						}
					}

					_ = sleep(keepalive_timeout) => {
						if last_activity_main.elapsed() > keepalive_timeout {
							let _ = events_tx.try_send(status(platform, false, "keepalive watchdog triggered; reconnecting"));
							current_ws_url = self.cfg.eventsub_ws_url.clone();
							break;
						}
					}
				}

				if backpressure.dropped_ingest_since_report > 0
					&& backpressure.last_report.elapsed() >= backpressure_report_interval
				{
					let dropped = backpressure.dropped_ingest_since_report;
					backpressure.dropped_ingest_since_report = 0;
					backpressure.last_report = Instant::now();

					let _ = events_tx.try_send(status(
						platform,
						true,
						format!(
							"backpressure: dropped {dropped} ingest messages (total_dropped={})",
							backpressure.dropped_ingest
						),
					));
				}
			}

			reconnect_attempt = reconnect_attempt.saturating_add(1);
		}

		let _ = events_tx.try_send(status(platform, false, "twitch adapter stopped"));
		Ok(())
	}
}

#[async_trait::async_trait]
impl PlatformAdapter for TwitchEventSubAdapter {
	fn platform(&self) -> Platform {
		Platform::Twitch
	}

	async fn run(self: Box<Self>, control_rx: AdapterControlRx, events_tx: AdapterEventTx) -> anyhow::Result<()> {
		ensure_asset_cache_pruner();
		self.run_loop(control_rx, events_tx).await
	}
}
