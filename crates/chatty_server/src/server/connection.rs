#![forbid(unsafe_code)]

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use anyhow::{Context as _, anyhow};
use chatty_domain::{Platform, RoomKey, RoomTopic};
use chatty_platform::kick::validate_user_token as validate_kick_user_token;
use chatty_platform::twitch::{refresh_user_token, validate_user_token};
use chatty_platform::{
	AdapterAuth, AssetBundle, AssetProvider, AssetScale, AssetScope, CommandError, CommandRequest, IngestPayload,
	SecretString,
};
use chatty_protocol::framing::{DEFAULT_MAX_FRAME_SIZE, encode_frame};
use chatty_protocol::pb;
use prost::Message;
use tokio::sync::{Mutex, RwLock, mpsc};
use tracing::{debug, error, info, warn};

use crate::server::adapter_manager::AdapterManager;
use crate::server::audit::AuditService;
use crate::server::auth::{AuthClaims, verify_hmac_token};
use crate::server::replay::ReplayService;
use crate::server::room_hub::{RoomHub, RoomHubItem};
use crate::server::state::GlobalState;
use crate::util::time::unix_ms_now;

/// v1 protocol version written into `pb::Envelope.version`.
pub const PROTOCOL_VERSION: u32 = 1;

fn map_asset_scale(scale: AssetScale) -> i32 {
	match scale {
		AssetScale::One => pb::AssetScale::AssetScale1x as i32,
		AssetScale::Two => pb::AssetScale::AssetScale2x as i32,
		AssetScale::Three => pb::AssetScale::AssetScale3x as i32,
		AssetScale::Four => pb::AssetScale::AssetScale4x as i32,
	}
}

fn compute_asset_bundle_etag(bundle: &AssetBundle) -> String {
	let mut keys = Vec::with_capacity(bundle.emotes.len().saturating_add(bundle.badges.len()));
	for emote in &bundle.emotes {
		let mut images = emote.images.clone();
		images.sort_by_key(|img| img.scale.as_u8());
		let image_key = images
			.iter()
			.map(|img| {
				format!(
					"{}:{}:{}:{}:{}",
					img.scale.as_u8(),
					img.url,
					img.format,
					img.width,
					img.height
				)
			})
			.collect::<Vec<_>>()
			.join("|");
		keys.push(format!("e:{}:{}:{}", emote.id, emote.name, image_key));
	}
	for badge in &bundle.badges {
		let mut images = badge.images.clone();
		images.sort_by_key(|img| img.scale.as_u8());
		let image_key = images
			.iter()
			.map(|img| {
				format!(
					"{}:{}:{}:{}:{}",
					img.scale.as_u8(),
					img.url,
					img.format,
					img.width,
					img.height
				)
			})
			.collect::<Vec<_>>()
			.join("|");
		keys.push(format!("b:{}:{}:{}", badge.id, badge.name, image_key));
	}

	keys.sort();
	let mut hasher = DefaultHasher::new();
	for key in keys {
		key.hash(&mut hasher);
	}
	format!("{:016x}", hasher.finish())
}

fn asset_bundle_envelope_len(topic: &str, assets: &pb::AssetBundleEvent) -> usize {
	let env = pb::Envelope {
		version: PROTOCOL_VERSION,
		request_id: String::new(),
		msg: Some(pb::envelope::Msg::Event(pb::EventEnvelope {
			topic: topic.to_string(),
			cursor: 0,
			server_time_unix_ms: 0,
			event: Some(pb::event_envelope::Event::AssetBundle(assets.clone())),
		})),
	};

	env.encoded_len()
}

struct AssetBundleChunkResult {
	events: Vec<pb::AssetBundleEvent>,
	dropped_emotes: usize,
	dropped_badges: usize,
}

fn chunk_asset_refs_for_frame(
	topic: &str,
	base: &pb::AssetBundleEvent,
	refs: &[pb::AssetRef],
	is_emotes: bool,
	max_frame_size: usize,
) -> (Vec<Vec<pb::AssetRef>>, usize) {
	let mut chunks: Vec<Vec<pb::AssetRef>> = Vec::new();
	let mut current: Vec<pb::AssetRef> = Vec::new();
	let mut dropped = 0usize;

	for asset in refs.iter().cloned() {
		current.push(asset.clone());
		let mut candidate = base.clone();
		if is_emotes {
			candidate.emotes = current.clone();
			candidate.badges.clear();
		} else {
			candidate.badges = current.clone();
			candidate.emotes.clear();
		}

		if asset_bundle_envelope_len(topic, &candidate) > max_frame_size {
			current.pop();
			if current.is_empty() {
				dropped += 1;
				continue;
			}

			chunks.push(current);
			current = vec![asset];
			let mut candidate = base.clone();
			if is_emotes {
				candidate.emotes = current.clone();
				candidate.badges.clear();
			} else {
				candidate.badges = current.clone();
				candidate.emotes.clear();
			}

			if asset_bundle_envelope_len(topic, &candidate) > max_frame_size {
				current.clear();
				dropped += 1;
			}
		}
	}

	if !current.is_empty() {
		chunks.push(current);
	}

	(chunks, dropped)
}

fn build_asset_bundle_chunks(topic: &str, assets: &pb::AssetBundleEvent, max_frame_size: usize) -> AssetBundleChunkResult {
	if asset_bundle_envelope_len(topic, assets) <= max_frame_size {
		return AssetBundleChunkResult {
			events: vec![assets.clone()],
			dropped_emotes: 0,
			dropped_badges: 0,
		};
	}

	let base = pb::AssetBundleEvent {
		origin: assets.origin.clone(),
		provider: assets.provider,
		scope: assets.scope,
		cache_key: assets.cache_key.clone(),
		etag: assets.etag.clone(),
		emotes: Vec::new(),
		badges: Vec::new(),
	};

	let (badge_chunks, dropped_badges) = chunk_asset_refs_for_frame(topic, &base, &assets.badges, false, max_frame_size);
	let (emote_chunks, dropped_emotes) = chunk_asset_refs_for_frame(topic, &base, &assets.emotes, true, max_frame_size);

	let mut events = Vec::with_capacity(badge_chunks.len().saturating_add(emote_chunks.len()));
	for chunk in badge_chunks {
		let mut out = base.clone();
		out.badges = chunk;
		events.push(out);
	}
	for chunk in emote_chunks {
		let mut out = base.clone();
		out.emotes = chunk;
		events.push(out);
	}

	AssetBundleChunkResult {
		events,
		dropped_emotes,
		dropped_badges,
	}
}

/// Per-connection server settings.
#[derive(Debug, Clone)]
pub struct ConnectionSettings {
	pub max_frame_bytes: u32,

	pub fan_in_channel_capacity: usize,

	pub auth_token: Option<chatty_platform::SecretString>,
	pub auth_hmac_secret: Option<chatty_platform::SecretString>,

	pub twitch_client_id: Option<String>,
	pub twitch_client_secret: Option<chatty_platform::SecretString>,

	pub command_rate_limit_per_conn_burst: u32,
	pub command_rate_limit_per_conn_per_minute: u32,
	pub command_rate_limit_per_topic_burst: u32,
	pub command_rate_limit_per_topic_per_minute: u32,
}

impl Default for ConnectionSettings {
	fn default() -> Self {
		Self {
			max_frame_bytes: DEFAULT_MAX_FRAME_SIZE as u32,
			fan_in_channel_capacity: 1024,
			auth_token: None,
			auth_hmac_secret: None,
			twitch_client_id: None,
			twitch_client_secret: None,
			command_rate_limit_per_conn_burst: 0,
			command_rate_limit_per_conn_per_minute: 0,
			command_rate_limit_per_topic_burst: 0,
			command_rate_limit_per_topic_per_minute: 0,
		}
	}
}

#[derive(Debug, Clone)]
struct TokenBucket {
	capacity: f64,
	tokens: f64,
	refill_per_sec: f64,
	last: Instant,
}

impl TokenBucket {
	fn new(capacity: u32, refill_per_minute: u32) -> Option<Self> {
		if capacity == 0 || refill_per_minute == 0 {
			return None;
		}
		Some(Self {
			capacity: capacity as f64,
			tokens: capacity as f64,
			refill_per_sec: refill_per_minute as f64 / 60.0,
			last: Instant::now(),
		})
	}

	fn allow(&mut self) -> bool {
		let now = Instant::now();
		let elapsed = now.duration_since(self.last).as_secs_f64();
		if elapsed > 0.0 {
			self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
			self.last = now;
		}
		if self.tokens >= 1.0 {
			self.tokens -= 1.0;
			true
		} else {
			false
		}
	}
}

#[derive(Debug)]
struct CommandRateLimiter {
	per_connection: Option<TokenBucket>,
	per_topic: HashMap<RoomKey, TokenBucket>,
	per_topic_burst: u32,
	per_topic_per_minute: u32,
	max_topics: usize,
}

impl CommandRateLimiter {
	fn new(settings: &ConnectionSettings) -> Self {
		Self {
			per_connection: TokenBucket::new(
				settings.command_rate_limit_per_conn_burst,
				settings.command_rate_limit_per_conn_per_minute,
			),
			per_topic: HashMap::new(),
			per_topic_burst: settings.command_rate_limit_per_topic_burst,
			per_topic_per_minute: settings.command_rate_limit_per_topic_per_minute,
			max_topics: 1024,
		}
	}

	fn allow_connection(&mut self) -> bool {
		match self.per_connection.as_mut() {
			Some(bucket) => bucket.allow(),
			None => true,
		}
	}

	fn allow_topic(&mut self, room: &RoomKey) -> bool {
		let Some(mut bucket) = TokenBucket::new(self.per_topic_burst, self.per_topic_per_minute) else {
			return true;
		};

		if self.per_topic.len() >= self.max_topics {
			self.per_topic.clear();
		}

		let entry = self.per_topic.entry(room.clone()).or_insert_with(|| {
			bucket.tokens = bucket.capacity;
			bucket
		});
		entry.allow()
	}
}

#[allow(clippy::too_many_arguments)]
pub async fn handle_connection(
	conn_id: u64,
	connection: quinn::Connection,
	state: Arc<RwLock<GlobalState>>,
	adapter_manager: Arc<AdapterManager>,
	room_hub: RoomHub,
	replay_service: Arc<ReplayService>,
	audit_service: Arc<AuditService>,
	settings: ConnectionSettings,
) -> anyhow::Result<()> {
	struct ConnectionGaugeGuard;
	impl Drop for ConnectionGaugeGuard {
		fn drop(&mut self) {
			metrics::gauge!("chatty_server_active_connections").decrement(1.0);
		}
	}

	metrics::gauge!("chatty_server_active_connections").increment(1.0);
	let _conn_guard = ConnectionGaugeGuard;

	let (mut control_send, mut control_recv) =
		connection.accept_bi().await.context("accept control bidirectional stream")?;

	let (ctrl_tx, mut ctrl_rx) = mpsc::unbounded_channel::<pb::Envelope>();
	let mut rate_limiter = CommandRateLimiter::new(&settings);
	let reader_task = tokio::spawn(async move {
		let mut buf = Vec::<u8>::with_capacity(16 * 1024);
		let mut tmp = [0u8; 8192];

		loop {
			let n = match control_recv.read(&mut tmp).await {
				Ok(Some(n)) => n,
				Ok(None) => return Ok::<(), anyhow::Error>(()),
				Err(e) => return Err(anyhow!(e).context("control stream read failed")),
			};

			metrics::counter!("chatty_server_control_bytes_in_total").increment(n as u64);

			buf.extend_from_slice(&tmp[..n]);

			loop {
				match chatty_protocol::decode_frame::<pb::Envelope>(&buf, DEFAULT_MAX_FRAME_SIZE) {
					Ok((msg, used)) => {
						buf.drain(0..used);
						metrics::counter!("chatty_server_envelopes_in_total").increment(1);

						if ctrl_tx.send(msg).is_err() {
							return Ok(());
						}
					}
					Err(chatty_protocol::FramingError::InsufficientData { .. }) => break,
					Err(e) => {
						metrics::counter!("chatty_server_control_decode_errors_total").increment(1);
						return Err(anyhow!(e).context("failed to decode control frame"));
					}
				}
			}
		}
	});

	let hello = wait_for_hello(&mut ctrl_rx).await?;
	let client_auth_token = hello.auth_token.clone();
	let selected_codec = match negotiate_codec(&hello) {
		Ok(c) => c,
		Err(msg) => {
			let _ = send_envelope(
				&mut control_send,
				pb::Envelope {
					version: PROTOCOL_VERSION,
					request_id: String::new(),
					msg: Some(pb::envelope::Msg::Error(pb::Error {
						code: "UNSUPPORTED_CODEC".to_string(),
						message: msg,
						topic: String::new(),
						request_id: String::new(),
					})),
				},
			)
			.await;
			return Err(anyhow!("unsupported codec"));
		}
	};
	let client_instance_id = if hello.client_instance_id.trim().is_empty() {
		format!("conn-{conn_id}")
	} else {
		hello.client_instance_id.clone()
	};

	info!(
		conn_id,
		client_name = %hello.client_name,
		client_instance_id = %hello.client_instance_id,
		"received Hello"
	);
	metrics::counter!("chatty_server_hello_total").increment(1);

	let mut auth_claims: Option<AuthClaims> = None;
	if settings.auth_token.is_some() || settings.auth_hmac_secret.is_some() {
		let provided = hello.auth_token.trim();
		let mut authorized = false;
		if let Some(expected) = settings.auth_token.as_ref()
			&& !provided.is_empty()
			&& provided == expected.expose()
		{
			authorized = true;
		}

		if !authorized
			&& let Some(secret) = settings.auth_hmac_secret.as_ref()
			&& !provided.is_empty()
		{
			match verify_hmac_token(provided, secret.expose()) {
				Ok(claims) => {
					authorized = true;
					auth_claims = Some(claims);
				}
				Err(e) => {
					warn!(conn_id, error = %e, "auth token rejected");
				}
			}
		}

		if !authorized {
			warn!(conn_id, "unauthorized: missing/invalid auth token");
			send_envelope(
				&mut control_send,
				pb::Envelope {
					version: PROTOCOL_VERSION,
					request_id: String::new(),
					msg: Some(pb::envelope::Msg::Error(pb::Error {
						code: "UNAUTHORIZED".to_string(),
						message: "invalid auth token".to_string(),
						topic: String::new(),
						request_id: String::new(),
					})),
				},
			)
			.await
			.ok();
			return Ok(());
		}
	}

	let mut user_oauth = hello.user_oauth_token.trim().to_string();
	if !user_oauth.is_empty() {
		let hello_client_id = hello.twitch_client_id.trim();
		let hello_user_id = hello.twitch_user_id.trim();
		let hello_username = hello.twitch_username.trim();
		let mut refresh_token = hello.twitch_refresh_token.trim().to_string();

		let validated = match validate_user_token(&user_oauth).await {
			Ok(v) => v,
			Err(e) => {
				let refresh_client_id = if !hello_client_id.is_empty() {
					Some(hello_client_id.to_string())
				} else {
					settings.twitch_client_id.clone()
				};

				if !refresh_token.is_empty()
					&& let Some(client_id) = refresh_client_id
					&& let Some(client_secret) = settings.twitch_client_secret.as_ref()
				{
					match refresh_user_token(&client_id, client_secret.expose(), &refresh_token).await {
						Ok(resp) => {
							user_oauth = resp.access_token;
							if let Some(new_refresh) = resp.refresh_token {
								refresh_token = new_refresh;
							}

							match validate_user_token(&user_oauth).await {
								Ok(v) => v,
								Err(e) => {
									warn!(conn_id, error = %e, "invalid twitch oauth token after refresh");
									send_envelope(
										&mut control_send,
										pb::Envelope {
											version: PROTOCOL_VERSION,
											request_id: String::new(),
											msg: Some(pb::envelope::Msg::Error(pb::Error {
												code: "UNAUTHORIZED".to_string(),
												message: "invalid twitch oauth token".to_string(),
												topic: String::new(),
												request_id: String::new(),
											})),
										},
									)
									.await
									.ok();
									return Ok(());
								}
							}
						}
						Err(e) => {
							warn!(conn_id, error = %e, "twitch oauth refresh failed");
							send_envelope(
								&mut control_send,
								pb::Envelope {
									version: PROTOCOL_VERSION,
									request_id: String::new(),
									msg: Some(pb::envelope::Msg::Error(pb::Error {
										code: "UNAUTHORIZED".to_string(),
										message: "invalid twitch oauth token".to_string(),
										topic: String::new(),
										request_id: String::new(),
									})),
								},
							)
							.await
							.ok();
							return Ok(());
						}
					}
				} else {
					warn!(conn_id, error = %e, "invalid twitch oauth token");
					send_envelope(
						&mut control_send,
						pb::Envelope {
							version: PROTOCOL_VERSION,
							request_id: String::new(),
							msg: Some(pb::envelope::Msg::Error(pb::Error {
								code: "UNAUTHORIZED".to_string(),
								message: "invalid twitch oauth token".to_string(),
								topic: String::new(),
								request_id: String::new(),
							})),
						},
					)
					.await
					.ok();
					return Ok(());
				}
			}
		};

		let client_id = if !hello_client_id.is_empty() {
			hello_client_id.to_string()
		} else {
			validated.client_id.clone()
		};
		let user_id = if !hello_user_id.is_empty() {
			hello_user_id.to_string()
		} else {
			validated.user_id.clone()
		};
		let username = if !hello_username.is_empty() {
			hello_username.to_string()
		} else {
			validated.login.clone()
		};

		let refresh_token = if refresh_token.trim().is_empty() {
			None
		} else {
			Some(SecretString::new(refresh_token))
		};

		let updated = adapter_manager
			.update_auth(
				Platform::Twitch,
				AdapterAuth::TwitchUser {
					client_id,
					access_token: SecretString::new(user_oauth.to_string()),
					refresh_token,
					user_id: Some(user_id),
					username: Some(username),
					expires_in: Some(std::time::Duration::from_secs(validated.expires_in)),
				},
			)
			.await;
		if updated {
			info!(conn_id, "applied user OAuth token to twitch adapter");
		} else {
			warn!(conn_id, "no twitch adapter available for user OAuth token");
		}
	}

	let mut kick_auth: Option<AdapterAuth> = None;
	let kick_oauth = hello.kick_user_oauth_token.trim();
	if !kick_oauth.is_empty() {
		let kick_user_id = hello.kick_user_id.trim();
		if let Err(e) = validate_kick_user_token(kick_oauth).await {
			warn!(conn_id, error = %e, "invalid kick oauth token");
			send_envelope(
				&mut control_send,
				pb::Envelope {
					version: PROTOCOL_VERSION,
					request_id: String::new(),
					msg: Some(pb::envelope::Msg::Error(pb::Error {
						code: "UNAUTHORIZED".to_string(),
						message: "invalid kick oauth token".to_string(),
						topic: String::new(),
						request_id: String::new(),
					})),
				},
			)
			.await
			.ok();
			return Ok(());
		}

		let auth = AdapterAuth::UserAccessToken {
			access_token: SecretString::new(kick_oauth.to_string()),
			user_id: if kick_user_id.is_empty() {
				None
			} else {
				Some(kick_user_id.to_string())
			},
			expires_in: None,
		};
		kick_auth = Some(auth.clone());

		let updated = adapter_manager.update_auth(Platform::Kick, auth).await;

		if updated {
			info!(conn_id, "applied user OAuth token to kick adapter");
		} else {
			warn!(conn_id, "no kick adapter available for user OAuth token");
		}
	}

	let welcome = pb::Welcome {
		server_name: format!("chatty-server/{}", env!("CARGO_PKG_VERSION")),
		server_instance_id: format!("conn-{conn_id}"),
		server_time_unix_ms: unix_ms_now(),
		max_frame_bytes: settings.max_frame_bytes,
		selected_codec: selected_codec as i32,
	};

	send_envelope(
		&mut control_send,
		pb::Envelope {
			version: PROTOCOL_VERSION,
			request_id: String::new(),
			msg: Some(pb::envelope::Msg::Welcome(welcome)),
		},
	)
	.await
	.context("send Welcome")?;

	let events_send: Arc<Mutex<Option<quinn::SendStream>>> = Arc::new(Mutex::new(None));
	let pending_replay: Arc<Mutex<Vec<pb::EventEnvelope>>> = Arc::new(Mutex::new(Vec::new()));

	let room_hub_for_events = room_hub.clone();

	let state_for_events = Arc::clone(&state);
	let events_send_for_task = Arc::clone(&events_send);
	let pending_replay_for_task = Arc::clone(&pending_replay);
	let replay_service_for_task = Arc::clone(&replay_service);
	let client_id_for_task = client_instance_id.clone();

	let events_task = tokio::spawn(async move {
		let mut first_event_sent = false;

		let (fan_in_tx, mut fan_in_rx) = mpsc::channel::<(String, RoomHubItem)>(settings.fan_in_channel_capacity);

		let mut room_tasks: HashMap<String, tokio::task::JoinHandle<()>> = HashMap::new();

		async fn ensure_room_task(
			topic: &str,
			room_hub: &RoomHub,
			fan_in_tx: &mpsc::Sender<(String, RoomHubItem)>,
			room_tasks: &mut HashMap<String, tokio::task::JoinHandle<()>>,
		) {
			if room_tasks.contains_key(topic) {
				return;
			}

			let room = match RoomTopic::parse(topic) {
				Ok(room) => room,
				Err(_) => return,
			};
			let mut rx = room_hub.subscribe_room(room).await;

			let topic_s = topic.to_string();
			let tx = fan_in_tx.clone();

			let handle = tokio::spawn(async move {
				while let Some(item) = rx.recv().await {
					if tx.send((topic_s.clone(), item)).await.is_err() {
						break;
					}
				}
			});

			room_tasks.insert(topic.to_string(), handle);
		}

		async fn reconcile_room_tasks(
			conn_id: u64,
			state_for_events: &Arc<RwLock<GlobalState>>,
			room_hub_for_events: &RoomHub,
			fan_in_tx: &mpsc::Sender<(String, RoomHubItem)>,
			room_tasks: &mut HashMap<String, tokio::task::JoinHandle<()>>,
		) -> HashSet<String> {
			let topics: HashSet<String> = {
				let st = state_for_events.read().await;
				st.topics_for_conn(conn_id)
			};

			for topic in topics.iter() {
				ensure_room_task(topic, room_hub_for_events, fan_in_tx, room_tasks).await;
			}

			room_tasks.retain(|topic, handle| {
				if topics.contains(topic) {
					true
				} else {
					handle.abort();
					false
				}
			});

			topics
		}

		let mut current_topics =
			reconcile_room_tasks(conn_id, &state_for_events, &room_hub_for_events, &fan_in_tx, &mut room_tasks).await;

		loop {
			if current_topics.is_empty() {
				tokio::time::sleep(std::time::Duration::from_millis(25)).await;
				current_topics =
					reconcile_room_tasks(conn_id, &state_for_events, &room_hub_for_events, &fan_in_tx, &mut room_tasks)
						.await;
				continue;
			}

			let (topic, item) = match fan_in_rx.recv().await {
				Some(v) => v,
				None => return Ok::<(), anyhow::Error>(()),
			};

			if !current_topics.contains(&topic) {
				continue;
			}

			let mut guard = events_send_for_task.lock().await;
			let events_ready = guard.is_some();
			if let Some(events_send) = guard.as_mut() {
				let mut pending = pending_replay_for_task.lock().await;
				if !pending.is_empty() {
					for env in pending.drain(..) {
						let frame = match encode_frame(
							&pb::Envelope {
								version: PROTOCOL_VERSION,
								request_id: String::new(),
								msg: Some(pb::envelope::Msg::Event(env)),
							},
							DEFAULT_MAX_FRAME_SIZE,
						) {
							Ok(f) => f,
							Err(e) => {
								error!(conn_id, error = %e, "failed to encode pending replay frame");
								return Err::<(), anyhow::Error>(anyhow!(e));
							}
						};

						if let Err(e) = events_send.write_all(&frame).await {
							return Err(anyhow!(e).context("events stream write failed (pending replay)"));
						}
					}
				}
			}

			match item {
				RoomHubItem::Ingest(ingest) => match ingest.payload {
					IngestPayload::ChatMessage(m) => {
						let Some(events_send) = guard.as_mut() else {
							continue;
						};

						let origin = pb::Origin {
							platform: match ingest.room.platform {
								Platform::Twitch => 1,
								Platform::Kick => 2,
								Platform::YouTube => 3,
							},
							channel: ingest.room.room_id.as_str().to_string(),
							channel_display: ingest.room.room_id.as_str().to_string(),
						};

						let platform_time_unix_ms = ingest
							.platform_time
							.and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
							.map(|d| d.as_millis() as i64)
							.unwrap_or(0);

						let emotes: Vec<pb::AssetRef> = m
							.emotes
							.into_iter()
							.map(|emote| pb::AssetRef {
								id: emote.id,
								name: emote.name,
								images: emote
									.images
									.into_iter()
									.map(|img| pb::AssetImage {
										scale: map_asset_scale(img.scale),
										url: img.url,
										format: img.format,
										width: img.width,
										height: img.height,
									})
									.collect(),
							})
							.collect();

						let message = pb::ChatMessage {
							author_id: m.author.id,
							author_login: m.author.login,
							author_display: m.author.display.unwrap_or_default(),
							text: m.text,
							platform_time_unix_ms,
							badge_ids: m.badges,
							emotes,
						};

						let chat_message_event = pb::ChatMessageEvent {
							origin: Some(origin),
							message: Some(message),
							server_message_id: m.ids.server_id.to_string(),
							platform_message_id: m.ids.platform_id.unwrap_or_default(),
							reply: m.reply.map(|reply| pb::Reply {
								server_message_id: reply.server_message_id.unwrap_or_default(),
								platform_message_id: reply.platform_message_id.unwrap_or_default(),
								user_id: reply.user_id.unwrap_or_default(),
								user_login: reply.user_login,
								user_display: reply.user_display.unwrap_or_default(),
								message: reply.message,
							}),
						};

						let env = pb::EventEnvelope {
							topic: topic.clone(),
							cursor: 0,
							server_time_unix_ms: unix_ms_now(),
							event: Some(pb::event_envelope::Event::ChatMessage(chat_message_event)),
						};

						let env = replay_service_for_task
							.push_event(&client_id_for_task, &topic, env)
							.await
							.context("persist replay event")?;

						let cursor = env.cursor;
						let frame = match encode_frame(
							&pb::Envelope {
								version: PROTOCOL_VERSION,
								request_id: String::new(),
								msg: Some(pb::envelope::Msg::Event(env)),
							},
							DEFAULT_MAX_FRAME_SIZE,
						) {
							Ok(f) => f,
							Err(e) => {
								error!(conn_id, error = %e, "failed to encode event frame");
								return Err::<(), anyhow::Error>(anyhow!(e));
							}
						};

						if !first_event_sent {
							first_event_sent = true;
							info!(
								conn_id,
								topic = %topic,
								cursor,
								frame_len = frame.len(),
								"writing first ingest-driven event frame to events stream"
							);
						} else {
							debug!(
								conn_id,
								topic = %topic,
								cursor,
								frame_len = frame.len(),
								"writing ingest-driven event frame to events stream"
							);
						}

						if let Err(e) = events_send.write_all(&frame).await {
							return Err(anyhow!(e).context("events stream write failed"));
						}
					}
					IngestPayload::AssetBundle(bundle) => {
						if !events_ready {
							let origin = pb::Origin {
								platform: match ingest.room.platform {
									Platform::Twitch => 1,
									Platform::Kick => 2,
									Platform::YouTube => 3,
								},
								channel: ingest.room.room_id.as_str().to_string(),
								channel_display: ingest.room.room_id.as_str().to_string(),
							};

							let provider = match bundle.provider {
								AssetProvider::Twitch => 1,
								AssetProvider::Kick => 2,
								AssetProvider::SevenTv => 3,
								AssetProvider::Ffz => 4,
								AssetProvider::Bttv => 5,
							};

							let scope = match bundle.scope {
								AssetScope::Global => 1,
								AssetScope::Channel => 2,
							};

							let etag = bundle.etag.clone().unwrap_or_else(|| compute_asset_bundle_etag(&bundle));
							let emotes: Vec<pb::AssetRef> = bundle
								.emotes
								.into_iter()
								.map(|emote| pb::AssetRef {
									id: emote.id,
									name: emote.name,
									images: emote
										.images
										.into_iter()
										.map(|img| pb::AssetImage {
											scale: map_asset_scale(img.scale),
											url: img.url,
											format: img.format,
											width: img.width,
											height: img.height,
										})
										.collect(),
								})
								.collect();
							let badges: Vec<pb::AssetRef> = bundle
								.badges
								.into_iter()
								.map(|badge| pb::AssetRef {
									id: badge.id,
									name: badge.name,
									images: badge
										.images
										.into_iter()
										.map(|img| pb::AssetImage {
											scale: map_asset_scale(img.scale),
											url: img.url,
											format: img.format,
											width: img.width,
											height: img.height,
										})
										.collect(),
								})
								.collect();

							let assets = pb::AssetBundleEvent {
								origin: Some(origin),
								provider,
								scope,
								cache_key: bundle.cache_key,
								etag,
								emotes,
								badges,
							};

							let original_emotes = assets.emotes.len();
							let original_badges = assets.badges.len();
							let chunks = build_asset_bundle_chunks(&topic, &assets, DEFAULT_MAX_FRAME_SIZE);
							if chunks.events.is_empty() {
								warn!(
									conn_id,
									topic = %topic,
									cache_key = %assets.cache_key,
									provider = assets.provider,
									scope = assets.scope,
									original_emotes,
									original_badges,
									dropped_emotes = chunks.dropped_emotes,
									dropped_badges = chunks.dropped_badges,
									"dropping AssetBundle; no chunk fits within max frame size"
								);
								continue;
							}

							if chunks.dropped_emotes > 0 || chunks.dropped_badges > 0 {
								warn!(
									conn_id,
									topic = %topic,
									cache_key = %assets.cache_key,
									provider = assets.provider,
									scope = assets.scope,
									original_emotes,
									original_badges,
									dropped_emotes = chunks.dropped_emotes,
									dropped_badges = chunks.dropped_badges,
									"some AssetBundle entries dropped; too large for max frame size"
								);
							}

							for assets in chunks.events {
								info!(
									conn_id,
									topic = %topic,
									cache_key = %assets.cache_key,
									provider = assets.provider,
									scope = assets.scope,
									emote_count = assets.emotes.len(),
									badge_count = assets.badges.len(),
									"buffering AssetBundle event until events stream opens"
								);

								let env = pb::EventEnvelope {
									topic: topic.clone(),
									cursor: 0,
									server_time_unix_ms: unix_ms_now(),
									event: Some(pb::event_envelope::Event::AssetBundle(assets)),
								};

								let env = replay_service_for_task
									.push_event(&client_id_for_task, &topic, env)
									.await
									.context("persist replay event")?;

								let mut pending = pending_replay_for_task.lock().await;
								pending.push(env);
							}
							continue;
						}

						let Some(events_send) = guard.as_mut() else {
							continue;
						};

						let origin = pb::Origin {
							platform: match ingest.room.platform {
								Platform::Twitch => 1,
								Platform::Kick => 2,
								Platform::YouTube => 3,
							},
							channel: ingest.room.room_id.as_str().to_string(),
							channel_display: ingest.room.room_id.as_str().to_string(),
						};

						let provider = match bundle.provider {
							AssetProvider::Twitch => 1,
							AssetProvider::Kick => 2,
							AssetProvider::SevenTv => 3,
							AssetProvider::Ffz => 4,
							AssetProvider::Bttv => 5,
						};
						let scope = match bundle.scope {
							AssetScope::Global => 1,
							AssetScope::Channel => 2,
						};

						let etag = bundle.etag.clone().unwrap_or_else(|| compute_asset_bundle_etag(&bundle));
						let emotes: Vec<pb::AssetRef> = bundle
							.emotes
							.into_iter()
							.map(|emote| pb::AssetRef {
								id: emote.id,
								name: emote.name,
								images: emote
									.images
									.into_iter()
									.map(|img| pb::AssetImage {
										scale: map_asset_scale(img.scale),
										url: img.url,
										format: img.format,
										width: img.width,
										height: img.height,
									})
									.collect(),
							})
							.collect();

						let badges: Vec<pb::AssetRef> = bundle
							.badges
							.into_iter()
							.map(|badge| pb::AssetRef {
								id: badge.id,
								name: badge.name,
								images: badge
									.images
									.into_iter()
									.map(|img| pb::AssetImage {
										scale: map_asset_scale(img.scale),
										url: img.url,
										format: img.format,
										width: img.width,
										height: img.height,
									})
									.collect(),
							})
							.collect();

						let assets = pb::AssetBundleEvent {
							origin: Some(origin),
							provider,
							scope,
							cache_key: bundle.cache_key,
							etag,
							emotes,
							badges,
						};

						let original_emotes = assets.emotes.len();
						let original_badges = assets.badges.len();
						let chunks = build_asset_bundle_chunks(&topic, &assets, DEFAULT_MAX_FRAME_SIZE);
						if chunks.events.is_empty() {
							warn!(
								conn_id,
								topic = %topic,
								cache_key = %assets.cache_key,
								provider = assets.provider,
								scope = assets.scope,
								original_emotes,
								original_badges,
								dropped_emotes = chunks.dropped_emotes,
								dropped_badges = chunks.dropped_badges,
								"dropping AssetBundle; no chunk fits within max frame size"
							);
							continue;
						}

						if chunks.dropped_emotes > 0 || chunks.dropped_badges > 0 {
							warn!(
								conn_id,
								topic = %topic,
								cache_key = %assets.cache_key,
								provider = assets.provider,
								scope = assets.scope,
								original_emotes,
								original_badges,
								dropped_emotes = chunks.dropped_emotes,
								dropped_badges = chunks.dropped_badges,
								"some AssetBundle entries dropped; too large for max frame size"
							);
						}

						for assets in chunks.events {
							info!(
								conn_id,
								topic = %topic,
								cache_key = %assets.cache_key,
								provider = assets.provider,
								scope = assets.scope,
								emote_count = assets.emotes.len(),
								badge_count = assets.badges.len(),
								"emitting AssetBundle event"
							);

							let env = pb::EventEnvelope {
								topic: topic.clone(),
								cursor: 0,
								server_time_unix_ms: unix_ms_now(),
								event: Some(pb::event_envelope::Event::AssetBundle(assets)),
							};

							let env = replay_service_for_task
								.push_event(&client_id_for_task, &topic, env)
								.await
								.context("persist replay event")?;

							let frame = match encode_frame(
								&pb::Envelope {
									version: PROTOCOL_VERSION,
									request_id: String::new(),
									msg: Some(pb::envelope::Msg::Event(env)),
								},
								DEFAULT_MAX_FRAME_SIZE,
							) {
								Ok(f) => f,
								Err(e) => {
									error!(conn_id, error = %e, "failed to encode asset bundle frame");
									return Err::<(), anyhow::Error>(anyhow!(e));
								}
							};

							if let Err(e) = events_send.write_all(&frame).await {
								return Err(anyhow!(e).context("events stream write failed"));
							}
						}
					}
					IngestPayload::RoomState(state) => {
						let origin = pb::Origin {
							platform: match ingest.room.platform {
								Platform::Twitch => 1,
								Platform::Kick => 2,
								Platform::YouTube => 3,
							},
							channel: ingest.room.room_id.as_str().to_string(),
							channel_display: ingest.room.room_id.as_str().to_string(),
						};

						let settings = pb::RoomChatSettings {
							emote_only: state.settings.emote_only,
							subscribers_only: state.settings.subscribers_only,
							unique_chat: state.settings.unique_chat,
							slow_mode: state.settings.slow_mode,
							slow_mode_wait_time_seconds: state.settings.slow_mode_wait_time_seconds,
							followers_only: state.settings.followers_only,
							followers_only_duration_minutes: state.settings.followers_only_duration_minutes,
						};

						let room_state = pb::RoomStateEvent {
							origin: Some(origin),
							settings: Some(settings),
							flags: state.flags.into_iter().collect(),
							notes: state.notes.unwrap_or_default(),
						};

						let env = pb::EventEnvelope {
							topic: topic.clone(),
							cursor: 0,
							server_time_unix_ms: unix_ms_now(),
							event: Some(pb::event_envelope::Event::RoomState(room_state)),
						};

						if !events_ready {
							let env = replay_service_for_task
								.push_event(&client_id_for_task, &topic, env)
								.await
								.context("persist room state event")?;
							let mut pending = pending_replay_for_task.lock().await;
							pending.push(env);
							continue;
						}

						let Some(events_send) = guard.as_mut() else {
							continue;
						};

						let env = replay_service_for_task
							.push_event(&client_id_for_task, &topic, env)
							.await
							.context("persist room state event")?;

						let frame = match encode_frame(
							&pb::Envelope {
								version: PROTOCOL_VERSION,
								request_id: String::new(),
								msg: Some(pb::envelope::Msg::Event(env)),
							},
							DEFAULT_MAX_FRAME_SIZE,
						) {
							Ok(f) => f,
							Err(e) => {
								error!(conn_id, error = %e, "failed to encode room state frame");
								return Err::<(), anyhow::Error>(anyhow!(e));
							}
						};

						if let Err(e) = events_send.write_all(&frame).await {
							return Err(anyhow!(e).context("events stream write failed"));
						}
					}
					_ => {}
				},
				RoomHubItem::Lagged { dropped } => {
					let Some(events_send) = guard.as_mut() else {
						continue;
					};

					let lagged = pb::TopicLaggedEvent {
						dropped,
						detail: "room subscriber queue full".to_string(),
					};

					let env = pb::EventEnvelope {
						topic: topic.clone(),
						cursor: 0,
						server_time_unix_ms: unix_ms_now(),
						event: Some(pb::event_envelope::Event::TopicLagged(lagged)),
					};

					let env = replay_service_for_task
						.push_event(&client_id_for_task, &topic, env)
						.await
						.context("persist replay event")?;

					let frame = match encode_frame(
						&pb::Envelope {
							version: PROTOCOL_VERSION,
							request_id: String::new(),
							msg: Some(pb::envelope::Msg::Event(env)),
						},
						DEFAULT_MAX_FRAME_SIZE,
					) {
						Ok(f) => f,
						Err(e) => {
							error!(conn_id, error = %e, "failed to encode lagged event frame");
							return Err::<(), anyhow::Error>(anyhow!(e));
						}
					};

					if let Err(e) = events_send.write_all(&frame).await {
						return Err(anyhow!(e).context("events stream write failed (lagged event)"));
					}

					warn!(
						conn_id,
						topic = %topic,
						dropped,
						"room subscription lagged; events were dropped"
					);
				}
				RoomHubItem::Status(_st) => {}
			}

			current_topics =
				reconcile_room_tasks(conn_id, &state_for_events, &room_hub_for_events, &fan_in_tx, &mut room_tasks).await;
		}
	});

	let loop_result = async {
		while let Some(env) = ctrl_rx.recv().await {
			let Some(msg) = env.msg else { continue };

			match msg {
				pb::envelope::Msg::Ping(ping) => {
					let pong = pb::Pong {
						client_time_unix_ms: ping.client_time_unix_ms,
						server_time_unix_ms: unix_ms_now(),
					};

					send_envelope(
						&mut control_send,
						pb::Envelope {
							version: PROTOCOL_VERSION,
							request_id: env.request_id,
							msg: Some(pb::envelope::Msg::Pong(pong)),
						},
					)
					.await?;
				}

				pb::envelope::Msg::Subscribe(sub) => {
					let last_cursor_by_topic: HashMap<String, u64> =
						sub.subs.iter().map(|s| (s.topic.clone(), s.last_cursor)).collect();
					debug!(conn_id, topics = ?sub.subs.iter().map(|s| &s.topic).collect::<Vec<_>>(), "received Subscribe");
					let (mut results, topics_to_join) = handle_subscribe(conn_id, &state, sub).await;
					debug!(conn_id, topics_to_join = ?topics_to_join, "Subscribe processed, topics_to_join determined");

					let mut pending = pending_replay.lock().await;
					for result in &mut results {
						let last_cursor = *last_cursor_by_topic.get(&result.topic).unwrap_or(&0);
						let outcome = {
							replay_service
								.replay(&client_instance_id, &result.topic, last_cursor)
								.await
								.context("replay events")?
						};

						result.status = outcome.status as i32;
						result.current_cursor = outcome.current_cursor;
						if !outcome.items.is_empty() {
							pending.extend(outcome.items);
						}

						if outcome.status == pb::subscription_result::Status::ReplayNotAvailable && last_cursor > 0 {
							let lagged = pb::TopicLaggedEvent {
								dropped: outcome.current_cursor.saturating_sub(last_cursor),
								detail: "replay buffer exhausted".to_string(),
							};
							let env = pb::EventEnvelope {
								topic: result.topic.clone(),
								cursor: 0,
								server_time_unix_ms: unix_ms_now(),
								event: Some(pb::event_envelope::Event::TopicLagged(lagged)),
							};

							let env = replay_service
								.push_event(&client_instance_id, &result.topic, env)
								.await
								.context("persist replay event")?;
							result.current_cursor = env.cursor;
							pending.push(env);
						}
					}
					drop(pending);

					let permission_results = results.clone();
					send_envelope(
						&mut control_send,
						pb::Envelope {
							version: PROTOCOL_VERSION,
							request_id: env.request_id,
							msg: Some(pb::envelope::Msg::Subscribed(pb::Subscribed { results })),
						},
					)
					.await?;

					adapter_manager.apply_global_joins_leaves(&topics_to_join, &[]).await;

					let mut permission_events: Vec<pb::EventEnvelope> = Vec::new();
					for result in &permission_results {
						if result.status != pb::subscription_result::Status::Ok as i32 {
							continue;
						}

						let Ok(room) = RoomTopic::parse(&result.topic) else {
							continue;
						};

						let perms_auth = if room.platform == Platform::Kick {
							kick_auth.clone()
						} else {
							None
						};

						if let Some(perms) = adapter_manager.query_permissions(&room, perms_auth).await {
							let env = pb::EventEnvelope {
								topic: result.topic.clone(),
								cursor: 0,
								server_time_unix_ms: unix_ms_now(),
								event: Some(pb::event_envelope::Event::Permissions(pb::PermissionsEvent {
									can_send: perms.can_send,
									can_reply: perms.can_reply,
									can_delete: perms.can_delete,
									can_timeout: perms.can_timeout,
									can_ban: perms.can_ban,
									is_moderator: perms.is_moderator,
									is_broadcaster: perms.is_broadcaster,
								})),
							};

							let env = replay_service
								.push_event(&client_instance_id, &result.topic, env)
								.await
								.context("persist permissions event")?;
							permission_events.push(env);
						}
					}

					if !permission_events.is_empty() {
						let mut pending = pending_replay.lock().await;
						pending.extend(permission_events);
					}

					let mut guard = events_send.lock().await;
					if guard.is_none() {
						info!(
							conn_id,
							"waiting to accept events bidirectional stream (client-opened; after Subscribed)"
						);
						let (send, _recv) = connection.accept_bi().await.context("accept events bidirectional stream")?;
						info!(conn_id, "accepted events bidirectional stream (server will only write)");
						*guard = Some(send);
					}

					if let Some(events_send) = guard.as_mut() {
						let mut pending = pending_replay.lock().await;
						for env in pending.drain(..) {
							let frame = encode_frame(
								&pb::Envelope {
									version: PROTOCOL_VERSION,
									request_id: String::new(),
									msg: Some(pb::envelope::Msg::Event(env)),
								},
								DEFAULT_MAX_FRAME_SIZE,
							)?;
							events_send.write_all(&frame).await?;
						}
					}
				}

				pb::envelope::Msg::Unsubscribe(unsub) => {
					let (results, topics_to_leave) = handle_unsubscribe(conn_id, &state, unsub).await;

					send_envelope(
						&mut control_send,
						pb::Envelope {
							version: PROTOCOL_VERSION,
							request_id: env.request_id,
							msg: Some(pb::envelope::Msg::Unsubscribed(pb::Unsubscribed { results })),
						},
					)
					.await?;

					adapter_manager.apply_global_joins_leaves(&[], &topics_to_leave).await;
				}

				pb::envelope::Msg::Command(cmd) => {
					let result = handle_command(
						conn_id,
						&settings,
						&client_auth_token,
						auth_claims.as_ref(),
						&mut rate_limiter,
						&cmd,
						&adapter_manager,
						&audit_service,
						kick_auth.clone(),
					)
					.await;
					send_envelope(
						&mut control_send,
						pb::Envelope {
							version: PROTOCOL_VERSION,
							request_id: env.request_id,
							msg: Some(pb::envelope::Msg::CommandResult(result)),
						},
					)
					.await?;
				}

				pb::envelope::Msg::Hello(_) => {
					debug!(conn_id, "ignoring duplicate Hello");
				}

				other => {
					warn!(conn_id, "unhandled control message: {:?}", other);
				}
			}
		}
		Ok::<(), anyhow::Error>(())
	}
	.await;

	{
		let topics_to_leave = {
			let mut st = state.write().await;
			let topics = st.topics_for_conn(conn_id);
			debug!(conn_id, topics = ?topics.iter().collect::<Vec<_>>(), "connection closing, removing subscriptions");
			st.remove_conn(conn_id)
		};
		if !topics_to_leave.is_empty() {
			debug!(conn_id, topics_to_leave = ?topics_to_leave, "connection closed, leaving rooms");
			adapter_manager.apply_global_joins_leaves(&[], &topics_to_leave).await;
		}
	}

	let _ = reader_task.await;
	let _ = events_task.await;

	loop_result
}

async fn wait_for_hello(ctrl_rx: &mut mpsc::UnboundedReceiver<pb::Envelope>) -> anyhow::Result<pb::Hello> {
	while let Some(env) = ctrl_rx.recv().await {
		let Some(msg) = env.msg else { continue };
		if let pb::envelope::Msg::Hello(h) = msg {
			return Ok(h);
		}
	}
	Err(anyhow!("connection closed before Hello"))
}

fn negotiate_codec(hello: &pb::Hello) -> Result<pb::Codec, String> {
	let mut supported: Vec<pb::Codec> = hello
		.supported_codecs
		.iter()
		.filter_map(|c| pb::Codec::try_from(*c).ok())
		.collect();

	if supported.is_empty() {
		supported.push(pb::Codec::Protobuf);
	}

	let preferred = pb::Codec::try_from(hello.preferred_codec).ok();
	if let Some(pref) = preferred
		&& supported.contains(&pref)
	{
		return Ok(pref);
	}

	if supported.contains(&pb::Codec::Protobuf) {
		return Ok(pb::Codec::Protobuf);
	}

	Err("server supports only protobuf".to_string())
}

#[allow(clippy::too_many_arguments)]
async fn handle_command(
	conn_id: u64,
	settings: &ConnectionSettings,
	client_auth_token: &str,
	auth_claims: Option<&AuthClaims>,
	rate_limiter: &mut CommandRateLimiter,
	cmd: &pb::Command,
	adapter_manager: &AdapterManager,
	audit_service: &AuditService,
	kick_auth: Option<AdapterAuth>,
) -> pb::CommandResult {
	if let Some(expected) = settings.auth_token.as_ref()
		&& (client_auth_token.trim().is_empty() || client_auth_token != expected.expose())
	{
		metrics::counter!("chatty_server_commands_not_authorized_total").increment(1);
		return pb::CommandResult {
			status: pb::command_result::Status::NotAuthorized as i32,
			detail: "missing/invalid auth token".to_string(),
		};
	}
	if settings.auth_hmac_secret.is_some() {
		let Some(claims) = auth_claims else {
			metrics::counter!("chatty_server_commands_not_authorized_total").increment(1);
			return pb::CommandResult {
				status: pb::command_result::Status::NotAuthorized as i32,
				detail: "missing/invalid auth token".to_string(),
			};
		};
		let _ = claims;
	}

	if !rate_limiter.allow_connection() {
		metrics::counter!("chatty_server_commands_rate_limited_total").increment(1);
		metrics::counter!("chatty_server_commands_rate_limited_connection_total").increment(1);
		return pb::CommandResult {
			status: pb::command_result::Status::NotAuthorized as i32,
			detail: "rate limited".to_string(),
		};
	}

	let Some(cmd) = &cmd.command else {
		metrics::counter!("chatty_server_commands_invalid_payload_total").increment(1);
		return pb::CommandResult {
			status: pb::command_result::Status::InvalidCommand as i32,
			detail: "missing command payload".to_string(),
		};
	};

	let (kind, topic) = match cmd {
		pb::command::Command::SendChat(c) => ("send_chat", c.topic.as_str()),
		pb::command::Command::DeleteMessage(c) => ("delete_message", c.topic.as_str()),
		pb::command::Command::TimeoutUser(c) => ("timeout_user", c.topic.as_str()),
		pb::command::Command::BanUser(c) => ("ban_user", c.topic.as_str()),
	};

	let room: RoomKey = match RoomTopic::parse(topic) {
		Ok(r) => r,
		Err(e) => {
			metrics::counter!("chatty_server_commands_invalid_topic_total").increment(1);
			return pb::CommandResult {
				status: pb::command_result::Status::InvalidTopic as i32,
				detail: format!("invalid topic: {e}"),
			};
		}
	};
	let room_for_log = room.clone();

	if !rate_limiter.allow_topic(&room) {
		metrics::counter!("chatty_server_commands_rate_limited_total").increment(1);
		metrics::counter!("chatty_server_commands_rate_limited_topic_total").increment(1);
		return pb::CommandResult {
			status: pb::command_result::Status::NotAuthorized as i32,
			detail: "rate limited".to_string(),
		};
	}

	let (request, target_user_id, target_message_id) = match cmd {
		pb::command::Command::SendChat(c) => {
			if c.text.trim().is_empty() {
				metrics::counter!("chatty_server_commands_invalid_command_total").increment(1);
				return pb::CommandResult {
					status: pb::command_result::Status::InvalidCommand as i32,
					detail: "empty message".to_string(),
				};
			}
			(
				CommandRequest::SendChat {
					room: room.clone(),
					text: c.text.clone(),
					reply_to_platform_message_id: if c.reply_to_platform_message_id.trim().is_empty() {
						None
					} else {
						Some(c.reply_to_platform_message_id.clone())
					},
				},
				None,
				None,
			)
		}
		pb::command::Command::DeleteMessage(c) => {
			if c.platform_message_id.trim().is_empty() {
				metrics::counter!("chatty_server_commands_invalid_command_total").increment(1);
				return pb::CommandResult {
					status: pb::command_result::Status::InvalidCommand as i32,
					detail: "missing platform_message_id".to_string(),
				};
			}
			(
				CommandRequest::DeleteMessage {
					room: room.clone(),
					platform_message_id: c.platform_message_id.clone(),
				},
				None,
				Some(c.platform_message_id.as_str()),
			)
		}
		pb::command::Command::TimeoutUser(c) => {
			if c.user_id.trim().is_empty() || c.duration_seconds == 0 {
				metrics::counter!("chatty_server_commands_invalid_command_total").increment(1);
				return pb::CommandResult {
					status: pb::command_result::Status::InvalidCommand as i32,
					detail: "missing user_id or duration".to_string(),
				};
			}
			(
				CommandRequest::TimeoutUser {
					room: room.clone(),
					user_id: c.user_id.clone(),
					duration_seconds: c.duration_seconds,
					reason: if c.reason.trim().is_empty() {
						None
					} else {
						Some(c.reason.clone())
					},
				},
				Some(c.user_id.as_str()),
				None,
			)
		}
		pb::command::Command::BanUser(c) => {
			if c.user_id.trim().is_empty() {
				metrics::counter!("chatty_server_commands_invalid_command_total").increment(1);
				return pb::CommandResult {
					status: pb::command_result::Status::InvalidCommand as i32,
					detail: "missing user_id".to_string(),
				};
			}
			(
				CommandRequest::BanUser {
					room: room.clone(),
					user_id: c.user_id.clone(),
					reason: if c.reason.trim().is_empty() {
						None
					} else {
						Some(c.reason.clone())
					},
				},
				Some(c.user_id.as_str()),
				None,
			)
		}
	};

	if let Err(e) = audit_service
		.record_command(&format!("conn-{conn_id}"), topic, kind, target_user_id, target_message_id)
		.await
	{
		metrics::counter!("chatty_server_command_audit_failures_total").increment(1);
		warn!(conn_id, error = %e, "failed to persist command audit");
	}

	tracing::info!(
		conn_id,
		command = kind,
		topic = %topic,
		platform = %room_for_log.platform,
		room_id = %room_for_log.room_id,
		"executing command"
	);

	metrics::counter!("chatty_server_commands_total").increment(1);

	let command_auth = if room.platform == Platform::Kick { kick_auth } else { None };
	match adapter_manager.execute_command(request, command_auth).await {
		Ok(()) => {
			metrics::counter!("chatty_server_commands_ok_total").increment(1);
			pb::CommandResult {
				status: pb::command_result::Status::Ok as i32,
				detail: "command executed".to_string(),
			}
		}
		Err(CommandError::NotSupported(detail)) => {
			metrics::counter!("chatty_server_commands_not_supported_total").increment(1);
			pb::CommandResult {
				status: pb::command_result::Status::NotSupported as i32,
				detail: detail.unwrap_or_else(|| "command not supported by adapter".to_string()),
			}
		}
		Err(CommandError::NotAuthorized(detail)) => {
			metrics::counter!("chatty_server_commands_not_authorized_total").increment(1);
			pb::CommandResult {
				status: pb::command_result::Status::NotAuthorized as i32,
				detail: detail.unwrap_or_else(|| "not authorized".to_string()),
			}
		}
		Err(CommandError::InvalidTopic(detail)) => {
			metrics::counter!("chatty_server_commands_invalid_topic_total").increment(1);
			pb::CommandResult {
				status: pb::command_result::Status::InvalidTopic as i32,
				detail: detail.unwrap_or_else(|| "invalid topic".to_string()),
			}
		}
		Err(CommandError::InvalidCommand(detail)) => {
			metrics::counter!("chatty_server_commands_invalid_command_total").increment(1);
			pb::CommandResult {
				status: pb::command_result::Status::InvalidCommand as i32,
				detail: detail.unwrap_or_else(|| "invalid command".to_string()),
			}
		}
		Err(CommandError::Internal(msg)) => {
			metrics::counter!("chatty_server_commands_internal_error_total").increment(1);
			pb::CommandResult {
				status: pb::command_result::Status::InternalError as i32,
				detail: msg,
			}
		}
	}
}

async fn send_envelope(send: &mut quinn::SendStream, env: pb::Envelope) -> anyhow::Result<()> {
	let frame = encode_frame(&env, DEFAULT_MAX_FRAME_SIZE).map_err(|e| anyhow!(e))?;
	metrics::counter!("chatty_server_envelopes_out_total").increment(1);
	metrics::counter!("chatty_server_control_bytes_out_total").increment(frame.len() as u64);

	send.write_all(&frame).await.context("stream write")?;
	Ok(())
}

async fn handle_subscribe(
	conn_id: u64,
	state: &Arc<RwLock<GlobalState>>,
	sub: pb::Subscribe,
) -> (Vec<pb::SubscriptionResult>, Vec<String>) {
	metrics::counter!("chatty_server_subscribe_requests_total").increment(1);
	metrics::counter!("chatty_server_subscribe_topics_total").increment(sub.subs.len() as u64);
	let mut st = state.write().await;
	st.handle_subscribe(conn_id, sub)
}

async fn handle_unsubscribe(
	conn_id: u64,
	state: &Arc<RwLock<GlobalState>>,
	unsub: pb::Unsubscribe,
) -> (Vec<pb::UnsubscribeResult>, Vec<String>) {
	metrics::counter!("chatty_server_unsubscribe_requests_total").increment(1);
	metrics::counter!("chatty_server_unsubscribe_topics_total").increment(unsub.topics.len() as u64);
	let mut st = state.write().await;
	st.handle_unsubscribe(conn_id, unsub)
}
