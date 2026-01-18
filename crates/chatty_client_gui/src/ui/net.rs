#![forbid(unsafe_code)]
#![allow(dead_code)]

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use chatty_client_core::{ClientConfigV1, ClientCoreError, SessionControl, SessionEvents};
use chatty_domain::RoomKey;
use chatty_protocol::pb;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, MissedTickBehavior};
use tracing::{debug, info, warn};

use crate::ui::app_state::AssetRefUi;

/// Dev auto-connect env flag.
pub const CHATTY_UI_AUTO_CONNECT_ENV: &str = "CHATTY_UI_AUTO_CONNECT";

/// Dev auto-subscribe env list (comma-separated topics).
pub const CHATTY_UI_AUTO_SUBSCRIBE_ENV: &str = "CHATTY_UI_AUTO_SUBSCRIBE";

const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(3);
const KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(10);
const KEEPALIVE_MAX_FAILURES: u32 = 3;

/// UI-level events emitted by the networking layer.
#[derive(Debug, Clone)]
pub enum UiEvent {
	Connecting,
	Reconnecting {
		attempt: u32,
		next_retry_in_ms: u64,
	},
	Connected {
		/// Best-effort; not always provided by the core.
		server_name: String,
		server_instance_id: String,
	},
	Disconnected {
		reason: String,
	},
	Error {
		message: String,
	},

	ChatMessage {
		topic: String,
		cursor: u64,
		author_login: String,
		author_display: Option<String>,
		author_id: Option<String>,
		text: String,
		server_message_id: Option<String>,
		platform_message_id: Option<String>,
		badge_ids: Vec<String>,
	},
	TopicLagged {
		topic: String,
		cursor: u64,
		dropped: u64,
		detail: String,
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

type BoxedSessionControl = Box<dyn SessionControlApi>;
type BoxedSessionEvents = Box<dyn SessionEventsApi>;

trait SessionControlApi: Send {
	fn subscribe<'a>(
		&'a mut self,
		subs: Vec<(String, u64)>,
	) -> Pin<Box<dyn Future<Output = Result<(), ClientCoreError>> + Send + 'a>>;

	fn unsubscribe<'a>(
		&'a mut self,
		topics: Vec<String>,
	) -> Pin<Box<dyn Future<Output = Result<(), ClientCoreError>> + Send + 'a>>;

	fn open_events_stream<'a>(
		&'a mut self,
	) -> Pin<Box<dyn Future<Output = Result<BoxedSessionEvents, ClientCoreError>> + Send + 'a>>;

	fn ping<'a>(
		&'a mut self,
		client_time_unix_ms: i64,
	) -> Pin<Box<dyn Future<Output = Result<pb::Pong, ClientCoreError>> + Send + 'a>>;

	fn send_command<'a>(
		&'a mut self,
		command: pb::Command,
	) -> Pin<Box<dyn Future<Output = Result<pb::CommandResult, ClientCoreError>> + Send + 'a>>;

	fn close(&self, code: u32, reason: &str);
}

trait SessionEventsApi: Send {
	fn run_events_loop<'a>(
		&'a mut self,
		on_event: Box<dyn FnMut(pb::EventEnvelope) + Send + 'a>,
	) -> Pin<Box<dyn Future<Output = Result<(), ClientCoreError>> + Send + 'a>>;
}

impl SessionControlApi for SessionControl {
	fn subscribe<'a>(
		&'a mut self,
		subs: Vec<(String, u64)>,
	) -> Pin<Box<dyn Future<Output = Result<(), ClientCoreError>> + Send + 'a>> {
		Box::pin(async move { SessionControl::subscribe_with_cursors(self, subs).await.map(|_| ()) })
	}

	fn unsubscribe<'a>(
		&'a mut self,
		topics: Vec<String>,
	) -> Pin<Box<dyn Future<Output = Result<(), ClientCoreError>> + Send + 'a>> {
		Box::pin(async move { SessionControl::unsubscribe(self, topics).await.map(|_| ()) })
	}

	fn open_events_stream<'a>(
		&'a mut self,
	) -> Pin<Box<dyn Future<Output = Result<BoxedSessionEvents, ClientCoreError>> + Send + 'a>> {
		Box::pin(async move {
			SessionControl::open_events_stream(self)
				.await
				.map(|e| Box::new(e) as BoxedSessionEvents)
		})
	}

	fn ping<'a>(
		&'a mut self,
		client_time_unix_ms: i64,
	) -> Pin<Box<dyn Future<Output = Result<pb::Pong, ClientCoreError>> + Send + 'a>> {
		Box::pin(async move { SessionControl::ping(self, client_time_unix_ms).await })
	}

	fn send_command<'a>(
		&'a mut self,
		command: pb::Command,
	) -> Pin<Box<dyn Future<Output = Result<pb::CommandResult, ClientCoreError>> + Send + 'a>> {
		Box::pin(async move { SessionControl::send_command(self, command).await })
	}

	fn close(&self, code: u32, reason: &str) {
		SessionControl::close(self, code, reason);
	}
}

impl SessionEventsApi for SessionEvents {
	fn run_events_loop<'a>(
		&'a mut self,
		mut on_event: Box<dyn FnMut(pb::EventEnvelope) + Send + 'a>,
	) -> Pin<Box<dyn Future<Output = Result<(), ClientCoreError>> + Send + 'a>> {
		Box::pin(async move { SessionEvents::run_events_loop(self, &mut (on_event)).await })
	}
}

/// UI -> networking commands.
#[derive(Debug)]
pub enum NetCommand {
	Connect {
		cfg: Box<ClientConfigV1>,
	},
	Disconnect {
		reason: String,
	},
	SubscribeTopic {
		topic: String,
	},
	SubscribeRoomKey {
		room: RoomKey,
	},
	UnsubscribeRoomKey {
		room: RoomKey,
	},
	SendCommand {
		command: pb::Command,
	},
}

/// UI command handle for the networking task.
#[derive(Clone)]
pub struct NetController {
	cmd_tx: mpsc::Sender<NetCommand>,
}

impl NetController {
	pub async fn connect(&self, cfg: ClientConfigV1) -> Result<(), String> {
		self.cmd_tx
			.send(NetCommand::Connect { cfg: Box::new(cfg) })
			.await
			.map_err(|_| "network task is not running".to_string())
	}

	pub async fn disconnect(&self, reason: impl Into<String>) -> Result<(), String> {
		self.cmd_tx
			.send(NetCommand::Disconnect { reason: reason.into() })
			.await
			.map_err(|_| "network task is not running".to_string())
	}

	pub async fn subscribe_topic(&self, topic: impl Into<String>) -> Result<(), String> {
		self.cmd_tx
			.send(NetCommand::SubscribeTopic { topic: topic.into() })
			.await
			.map_err(|_| "network task is not running".to_string())
	}

	pub async fn subscribe_room_key(&self, room: RoomKey) -> Result<(), String> {
		self.cmd_tx
			.send(NetCommand::SubscribeRoomKey { room })
			.await
			.map_err(|_| "network task is not running".to_string())
	}

	pub async fn unsubscribe_room_key(&self, room: RoomKey) -> Result<(), String> {
		self.cmd_tx
			.send(NetCommand::UnsubscribeRoomKey { room })
			.await
			.map_err(|_| "network task is not running".to_string())
	}

	pub async fn send_command(&self, command: pb::Command) -> Result<(), String> {
		self.cmd_tx
			.send(NetCommand::SendCommand { command })
			.await
			.map_err(|_| "network task is not running".to_string())
	}
}

/// Shutdown handle for the networking task.
pub struct ShutdownHandle {
	shutdown_tx: oneshot::Sender<()>,
}

impl ShutdownHandle {
	pub fn shutdown(self) {
		let _ = self.shutdown_tx.send(());
	}
}

/// Start networking runtime.
pub fn start_networking() -> (NetController, mpsc::UnboundedReceiver<UiEvent>, ShutdownHandle) {
	let (cmd_tx, cmd_rx) = mpsc::channel::<NetCommand>(128);
	let (ui_tx, ui_rx) = mpsc::unbounded_channel::<UiEvent>();
	let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

	let controller = NetController { cmd_tx };
	let shutdown = ShutdownHandle { shutdown_tx };

	std::thread::Builder::new()
		.name("chatty-network".to_string())
		.spawn(move || {
			let rt = tokio::runtime::Builder::new_multi_thread()
				.enable_all()
				.worker_threads(2)
				.thread_name("chatty-network-worker")
				.build()
				.expect("failed to build tokio runtime for networking");
			rt.block_on(run_network_task(cmd_rx, ui_tx, shutdown_rx));
		})
		.expect("failed to spawn network thread");

	(controller, ui_rx, shutdown)
}

fn is_truthy_env(v: &str) -> bool {
	matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

fn should_dev_auto_connect() -> bool {
	if !cfg!(debug_assertions) {
		return false;
	}
	std::env::var(CHATTY_UI_AUTO_CONNECT_ENV)
		.map(|v| is_truthy_env(&v))
		.unwrap_or(true)
}

fn dev_default_topics() -> Vec<String> {
	if let Ok(val) = std::env::var(CHATTY_UI_AUTO_SUBSCRIBE_ENV) {
		let topics: Vec<String> = val
			.split(',')
			.map(|s| s.trim())
			.filter(|s| !s.is_empty())
			.map(|s| s.to_string())
			.collect();
		if !topics.is_empty() {
			return topics;
		}
	}

	Vec::new()
}

fn topic_for_room(room: &RoomKey) -> String {
	format!("room:{}/{}", room.platform.as_str(), room.room_id.as_str())
}

fn map_core_err(e: ClientCoreError) -> String {
	match e {
		ClientCoreError::Endpoint(s) => s,
		ClientCoreError::Connect(s) => s,
		ClientCoreError::Framing(e) => e.to_string(),
		ClientCoreError::Protocol(s) => s,
		ClientCoreError::Io(s) => s,
		ClientCoreError::Other(s) => s,
	}
}

async fn run_network_task(
	cmd_rx: mpsc::Receiver<NetCommand>,
	ui_tx: mpsc::UnboundedSender<UiEvent>,
	shutdown_rx: oneshot::Receiver<()>,
) {
	run_network_task_with_session_factory(cmd_rx, ui_tx, shutdown_rx, |cfg, ui_tx| {
		Box::pin(connect_session(*cfg, ui_tx))
	})
	.await;
}

async fn run_network_task_with_session_factory<F>(
	mut cmd_rx: mpsc::Receiver<NetCommand>,
	ui_tx: mpsc::UnboundedSender<UiEvent>,
	mut shutdown_rx: oneshot::Receiver<()>,
	mut connect_fn: F,
) where
	F: FnMut(
		Box<ClientConfigV1>,
		mpsc::UnboundedSender<UiEvent>,
	) -> Pin<Box<dyn Future<Output = Option<BoxedSessionControl>> + Send>>,
{
	let mut session: Option<BoxedSessionControl> = None;

	let mut events_task: Option<tokio::task::JoinHandle<()>> = None;

	let mut topics_refcounts: HashMap<String, usize> = HashMap::new();
	let cursor_by_topic: Arc<Mutex<HashMap<String, u64>>> = Arc::new(Mutex::new(HashMap::new()));
	let mut last_connect_cfg: Option<ClientConfigV1> = None;
	let mut reconnect_attempt: u32 = 0;
	let mut reconnect_deadline: Option<Instant> = None;
	let mut keepalive_failures: u32 = 0;

	let mut keepalive_tick = tokio::time::interval(KEEPALIVE_INTERVAL);
	keepalive_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

	let mut dev_auto_connect_fired = false;

	loop {
		tokio::select! {
			_ = &mut shutdown_rx => {
				let _ = ui_tx.send(UiEvent::Disconnected { reason: "shutdown".to_string() });
				if let Some(s) = session.as_ref() {
					s.close(0, "shutdown");
				}
				if let Some(t) = events_task.take() {
					t.abort();
				}
				break;
			}

			_ = keepalive_tick.tick(), if session.is_some() => {
				if let Some(s) = session.as_mut() {
					let client_time_unix_ms = SystemTime::now()
						.duration_since(SystemTime::UNIX_EPOCH)
						.map(|d| d.as_millis() as i64)
						.unwrap_or(0);

					let ping_res = tokio::time::timeout(KEEPALIVE_TIMEOUT, s.ping(client_time_unix_ms)).await;
					match ping_res {
						Ok(Ok(_)) => {
							keepalive_failures = 0;
						}
						Ok(Err(e)) => {
							keepalive_failures = keepalive_failures.saturating_add(1);
							warn!(failure = keepalive_failures, error = %map_core_err(e), "keepalive failed");
						}
						Err(_) => {
							keepalive_failures = keepalive_failures.saturating_add(1);
							warn!(failure = keepalive_failures, "keepalive timeout");
						}
					}

					if keepalive_failures >= KEEPALIVE_MAX_FAILURES {
						let _ = ui_tx.send(UiEvent::Disconnected {
							reason: "keepalive failed; reconnecting".to_string(),
						});

						if let Some(t) = events_task.take() {
							t.abort();
						}

						if let Some(s) = session.as_ref() {
							s.close(0, "keepalive failed");
						}

						session = None;
						keepalive_failures = 0;

						if let Some(cfg) = last_connect_cfg.clone() {
							reconnect_attempt = reconnect_attempt.saturating_add(1).max(1);
							let (deadline, ms) = schedule_reconnect(reconnect_attempt);
							reconnect_deadline = Some(deadline);
							let _ = ui_tx
								.send(UiEvent::Reconnecting {
									attempt: reconnect_attempt,
									next_retry_in_ms: ms,
								});
							let _ = cfg;
						}
					}
				}
			}

			cmd = cmd_rx.recv() => {
				let Some(cmd) = cmd else {
					let _ = ui_tx.send(UiEvent::Disconnected { reason: "ui dropped controller".to_string() });
					if let Some(s) = session.as_ref() {
						s.close(0, "ui dropped controller");
					}
					if let Some(t) = events_task.take() {
						t.abort();
					}
					break;
				};

				match cmd {
					NetCommand::Connect { cfg } => {
						last_connect_cfg = Some(*cfg.clone());
						reconnect_attempt = 0;
						reconnect_deadline = None;
						let _ = ui_tx.send(UiEvent::Connecting);
						if let Some(t) = events_task.take() { t.abort(); }
						if let Some(s) = session.as_ref() { s.close(0, "reconnect"); }
						session = connect_fn(cfg.clone(), ui_tx.clone()).await;

						if let Some(s) = session.as_mut() {
							if cfg!(debug_assertions) {
								for topic in dev_default_topics() {
									*topics_refcounts.entry(topic).or_insert(0) += 1;
								}
							}
							if let Err(e) =
								reconcile_subscriptions_on_connect(s, &topics_refcounts, &cursor_by_topic, &ui_tx, &mut events_task)
								.await
							{
								let _ = ui_tx.send(UiEvent::Error { message: e });
								s.close(0, "subscribe failed");
								session = None;
							}
							reconnect_attempt = 0;
							reconnect_deadline = None;
						} else if let Some(cfg) = last_connect_cfg.clone() {
							reconnect_attempt = 1;
							let (deadline, ms) = schedule_reconnect(reconnect_attempt);
							reconnect_deadline = Some(deadline);
							let _ = ui_tx
								.send(UiEvent::Reconnecting {
									attempt: reconnect_attempt,
									next_retry_in_ms: ms,
								})
								;
							let _ = cfg;
						}
					}

					NetCommand::Disconnect { reason } => {
						if let Some(t) = events_task.take() { t.abort(); }
						if let Some(s) = session.as_ref() { s.close(0, &reason); }
						session = None;
						last_connect_cfg = None;
						reconnect_attempt = 0;
						reconnect_deadline = None;
						let _ = ui_tx.send(UiEvent::Disconnected { reason });
					}

					NetCommand::SubscribeTopic { topic } => {
						if let Some(s) = session.as_mut() {
							if let Err(e) = subscribe_topics(s, vec![topic], &cursor_by_topic, &ui_tx, &mut events_task).await {
								let _ = ui_tx.send(UiEvent::Error { message: e });
							}
						} else {
							let _ = ui_tx.send(UiEvent::Error { message: "not connected".to_string() });
						}
					}

					NetCommand::SubscribeRoomKey { room } => {
						let topic = topic_for_room(&room);

						let count = topics_refcounts.entry(topic.clone()).or_insert(0);
						let was_zero = *count == 0;
						*count += 1;

						if was_zero
							&& let Some(s) = session.as_mut()
								&& let Err(e) = subscribe_topics(s, vec![topic], &cursor_by_topic, &ui_tx, &mut events_task).await {
									let _ = ui_tx.send(UiEvent::Error { message: e });
								}
					}

					NetCommand::UnsubscribeRoomKey { room } => {
						let topic = topic_for_room(&room);

						let mut became_zero = false;
						if let Some(count) = topics_refcounts.get_mut(&topic) {
							if *count > 1 {
								*count -= 1;
							} else {
								topics_refcounts.remove(&topic);
								became_zero = true;
							}
						}

						if became_zero
							&& let Some(s) = session.as_mut()
								&& let Err(e) = unsubscribe_topics(s, vec![topic], &ui_tx).await {
									let _ = ui_tx.send(UiEvent::Error { message: e });
								}
					}

					NetCommand::SendCommand { command } => {
						if let Some(s) = session.as_mut() {
							match s.send_command(command).await {
								Ok(result) => {
									let _ = ui_tx
										.send(UiEvent::CommandResult {
											status: result.status,
											detail: result.detail,
										})
										;
								}
								Err(e) => {
									let _ = ui_tx.send(UiEvent::Error { message: map_core_err(e) });
								}
							}
						} else {
							let _ = ui_tx.send(UiEvent::Error { message: "not connected".to_string() });
						}
					}
				}
			}

			_ = tokio::time::sleep(Duration::from_millis(200)), if cfg!(debug_assertions) && !dev_auto_connect_fired && should_dev_auto_connect() => {
				dev_auto_connect_fired = true;
				let _ = ui_tx.send(UiEvent::Connecting);
				if let Some(t) = events_task.take() { t.abort(); }
				if let Some(s) = session.as_ref() { s.close(0, "dev auto-connect"); }
				session = connect_fn(Box::default(), ui_tx.clone()).await;

				if let Some(s) = session.as_mut() {
					if cfg!(debug_assertions) {
						for topic in dev_default_topics() {
							*topics_refcounts.entry(topic).or_insert(0) += 1;
						}
					}
					if let Err(e) =
						reconcile_subscriptions_on_connect(s, &topics_refcounts, &cursor_by_topic, &ui_tx, &mut events_task)
						.await
					{
						let _ = ui_tx.send(UiEvent::Error { message: e });
						s.close(0, "subscribe failed");
						session = None;
					}
				}
			}

			_ = async {
				if let Some(deadline) = reconnect_deadline {
					tokio::time::sleep_until(deadline).await;
				}
			}, if reconnect_deadline.is_some() => {
				if let Some(cfg) = last_connect_cfg.clone() {
					let _ = ui_tx.send(UiEvent::Connecting);
					if let Some(t) = events_task.take() { t.abort(); }
					if let Some(s) = session.as_ref() { s.close(0, "reconnect"); }
					session = connect_fn(Box::new(cfg), ui_tx.clone()).await;
					if let Some(s) = session.as_mut() {
						if let Err(e) =
							reconcile_subscriptions_on_connect(s, &topics_refcounts, &cursor_by_topic, &ui_tx, &mut events_task)
							.await
						{
							let _ = ui_tx.send(UiEvent::Error { message: e });
							s.close(0, "subscribe failed");
							session = None;
						}
						reconnect_attempt = 0;
						reconnect_deadline = None;
					} else {
						reconnect_attempt = reconnect_attempt.saturating_add(1).max(1);
						let (deadline, ms) = schedule_reconnect(reconnect_attempt);
						reconnect_deadline = Some(deadline);
						let _ = ui_tx
							.send(UiEvent::Reconnecting {
								attempt: reconnect_attempt,
								next_retry_in_ms: ms,
							})
							;
					}
				}
			}
		}
	}
}

fn schedule_reconnect(attempt: u32) -> (Instant, u64) {
	let base_ms = 500u64;
	let max_ms = 30_000u64;
	let pow = 2u64.saturating_pow(attempt.saturating_sub(1).min(6));
	let delay_ms = (base_ms.saturating_mul(pow)).min(max_ms);
	(Instant::now() + Duration::from_millis(delay_ms), delay_ms)
}

fn spawn_events_loop(
	mut events: BoxedSessionEvents,
	ui_tx: mpsc::UnboundedSender<UiEvent>,
	cursor_by_topic: Arc<Mutex<HashMap<String, u64>>>,
) -> tokio::task::JoinHandle<()> {
	tokio::spawn(async move {
		let res = events
			.run_events_loop(Box::new(|ev| {
				let topic = ev.topic.clone();
				let cursor = ev.cursor;
				let event_kind = match ev.event.as_ref() {
					Some(pb::event_envelope::Event::ChatMessage(_)) => "chat_message",
					Some(pb::event_envelope::Event::TopicLagged(_)) => "topic_lagged",
					Some(pb::event_envelope::Event::Permissions(_)) => "permissions",
					Some(pb::event_envelope::Event::AssetBundle(_)) => "asset_bundle",
					None => "empty",
				};
				debug!(%topic, cursor, %event_kind, "events stream received");
				{
					let mut cursors = cursor_by_topic.lock().unwrap();
					let entry = cursors.entry(topic.clone()).or_insert(0);
					if cursor > *entry {
						*entry = cursor;
					}
				}
				if let Some(ui_ev) = map_event_envelope_to_ui_event(ev) {
					let _ = ui_tx.send(ui_ev);
				} else {
					debug!(%topic, cursor, %event_kind, "event not mapped to UiEvent");
				}
			}))
			.await;

		match res {
			Ok(()) => {
				let _ = ui_tx.send(UiEvent::Disconnected {
					reason: "events stream closed".to_string(),
				});
			}
			Err(e) => {
				let msg = map_core_err(e);
				let _ = ui_tx.send(UiEvent::Disconnected { reason: msg });
			}
		}
	})
}

async fn connect_session(cfg: ClientConfigV1, ui_tx: mpsc::UnboundedSender<UiEvent>) -> Option<BoxedSessionControl> {
	info!(
		server_host = %cfg.server_host,
		server_port = cfg.server_port,
		"connecting..."
	);

	let session = match SessionControl::connect(cfg).await {
		Ok(s) => s,
		Err(e) => {
			let msg = map_core_err(e);
			let _ = ui_tx.send(UiEvent::Error { message: msg.clone() });
			let _ = ui_tx.send(UiEvent::Disconnected { reason: msg });
			return None;
		}
	};

	let _ = ui_tx.send(UiEvent::Connected {
		server_name: "chatty_server".to_string(),
		server_instance_id: "unknown".to_string(),
	});

	Some(Box::new(session))
}

async fn reconcile_subscriptions_on_connect(
	session: &mut BoxedSessionControl,
	topics_refcounts: &HashMap<String, usize>,
	cursor_by_topic: &Arc<Mutex<HashMap<String, u64>>>,
	ui_tx: &mpsc::UnboundedSender<UiEvent>,
	events_task: &mut Option<tokio::task::JoinHandle<()>>,
) -> Result<(), String> {
	let topics: Vec<String> = topics_refcounts
		.iter()
		.filter(|(_, c)| **c > 0)
		.map(|(t, _)| t.clone())
		.collect();

	if topics.is_empty() {
		return Ok(());
	}

	debug!(topics = ?topics, "subscribing topics on connect");
	if topics.is_empty() {
		debug!("no topics to subscribe on connect");
	}

	let mut subs = Vec::with_capacity(topics.len());
	{
		let cursors = cursor_by_topic.lock().unwrap();
		for topic in &topics {
			subs.push((topic.clone(), *cursors.get(topic).unwrap_or(&0)));
		}
	}

	session
		.subscribe(subs)
		.await
		.map_err(|e| format!("subscribe failed: {}", map_core_err(e)))?;

	ensure_events_loop_started(session, events_task, ui_tx, cursor_by_topic).await
}

async fn subscribe_topics(
	session: &mut BoxedSessionControl,
	topics: Vec<String>,
	cursor_by_topic: &Arc<Mutex<HashMap<String, u64>>>,
	ui_tx: &mpsc::UnboundedSender<UiEvent>,
	events_task: &mut Option<tokio::task::JoinHandle<()>>,
) -> Result<(), String> {
	debug!(topics = ?topics, "subscribing topics");
	let mut subs = Vec::with_capacity(topics.len());
	{
		let cursors = cursor_by_topic.lock().unwrap();
		for topic in &topics {
			subs.push((topic.clone(), *cursors.get(topic).unwrap_or(&0)));
		}
	}

	session
		.subscribe(subs)
		.await
		.map_err(|e| format!("subscribe failed: {}", map_core_err(e)))?;

	ensure_events_loop_started(session, events_task, ui_tx, cursor_by_topic).await
}

async fn unsubscribe_topics(
	session: &mut BoxedSessionControl,
	topics: Vec<String>,
	_ui_tx: &mpsc::UnboundedSender<UiEvent>,
) -> Result<(), String> {
	debug!(topics = ?topics, "unsubscribing topics");

	session
		.unsubscribe(topics)
		.await
		.map_err(|e| format!("unsubscribe failed: {}", map_core_err(e)))?;

	Ok(())
}

async fn ensure_events_loop_started(
	session: &mut BoxedSessionControl,
	events_task: &mut Option<tokio::task::JoinHandle<()>>,
	ui_tx: &mpsc::UnboundedSender<UiEvent>,
	cursor_by_topic: &Arc<Mutex<HashMap<String, u64>>>,
) -> Result<(), String> {
	if events_task.is_some() {
		return Ok(());
	}

	let events = session
		.open_events_stream()
		.await
		.map_err(|e| format!("open events stream failed: {}", map_core_err(e)))?;

	*events_task = Some(spawn_events_loop(events, ui_tx.clone(), Arc::clone(cursor_by_topic)));

	Ok(())
}

fn map_event_envelope_to_ui_event(ev: pb::EventEnvelope) -> Option<UiEvent> {
	let topic = ev.topic;
	let cursor = ev.cursor;

	match ev.event {
		Some(pb::event_envelope::Event::ChatMessage(cm)) => {
			let (author_login, author_display, text, badge_ids) = cm
				.message
				.as_ref()
				.map(|m| {
					let login = if m.author_login.is_empty() {
						"unknown".to_string()
					} else {
						m.author_login.clone()
					};
					let display = if m.author_display.is_empty() {
						None
					} else {
						Some(m.author_display.clone())
					};
					let text = m.text.clone();
					let badges = m.badge_ids.clone();
					(login, display, text, badges)
				})
				.unwrap_or_else(|| ("unknown".to_string(), None, "".to_string(), Vec::new()));

			Some(UiEvent::ChatMessage {
				topic,
				cursor,
				author_login,
				author_display,
				author_id: cm.message.as_ref().and_then(|m| {
					if m.author_id.is_empty() {
						None
					} else {
						Some(m.author_id.clone())
					}
				}),
				text,
				server_message_id: if cm.server_message_id.is_empty() {
					None
				} else {
					Some(cm.server_message_id)
				},
				platform_message_id: if cm.platform_message_id.is_empty() {
					None
				} else {
					Some(cm.platform_message_id)
				},
				badge_ids,
			})
		}
		Some(pb::event_envelope::Event::TopicLagged(lag)) => Some(UiEvent::TopicLagged {
			topic,
			cursor,
			dropped: lag.dropped,
			detail: if lag.detail.is_empty() {
				"lagged".to_string()
			} else {
				lag.detail
			},
		}),
		Some(pb::event_envelope::Event::Permissions(perms)) => Some(UiEvent::RoomPermissions {
			topic,
			can_send: perms.can_send,
			can_reply: perms.can_reply,
			can_delete: perms.can_delete,
			can_timeout: perms.can_timeout,
			can_ban: perms.can_ban,
			is_moderator: perms.is_moderator,
			is_broadcaster: perms.is_broadcaster,
		}),
		Some(pb::event_envelope::Event::AssetBundle(bundle)) => {
			let cache_key = if bundle.cache_key.is_empty() {
				format!("provider:{}:origin:{}", bundle.provider, topic)
			} else {
				bundle.cache_key
			};
			let etag = if bundle.etag.is_empty() { None } else { Some(bundle.etag) };
			let emotes = bundle
				.emotes
				.into_iter()
				.map(|emote| AssetRefUi {
					id: emote.id,
					name: emote.name,
					image_url: emote.image_url,
					image_format: emote.image_format,
					width: emote.width,
					height: emote.height,
				})
				.collect();
			let badges = bundle
				.badges
				.into_iter()
				.map(|badge| AssetRefUi {
					id: badge.id,
					name: badge.name,
					image_url: badge.image_url,
					image_format: badge.image_format,
					width: badge.width,
					height: badge.height,
				})
				.collect();

			Some(UiEvent::AssetBundle {
				topic,
				cache_key,
				etag,
				provider: bundle.provider,
				scope: bundle.scope,
				emotes,
				badges,
			})
		}
		None => None,
	}
}

#[cfg(all(test, feature = "gpui"))]
mod tests {
	use super::*;
	use std::sync::{Arc, Mutex};
	use tokio::sync::oneshot;

	#[derive(Default)]
	struct MockState {
		subscribe_calls: Vec<Vec<(String, u64)>>,
		unsubscribe_calls: Vec<Vec<String>>,
		open_events: usize,
		close_calls: Vec<(u32, String)>,
		_events_tx: Option<oneshot::Sender<()>>,
	}

	struct MockSessionControl {
		state: Arc<Mutex<MockState>>,
	}

	struct MockSessionEvents {
		rx: oneshot::Receiver<()>,
	}

	impl MockSessionControl {
		fn new(state: Arc<Mutex<MockState>>) -> Self {
			Self { state }
		}
	}

	impl SessionControlApi for MockSessionControl {
		fn subscribe<'a>(
			&'a mut self,
			subs: Vec<(String, u64)>,
		) -> Pin<Box<dyn Future<Output = Result<(), ClientCoreError>> + Send + 'a>> {
			let state = Arc::clone(&self.state);
			Box::pin(async move {
				state.lock().unwrap().subscribe_calls.push(subs);
				Ok(())
			})
		}

		fn unsubscribe<'a>(
			&'a mut self,
			topics: Vec<String>,
		) -> Pin<Box<dyn Future<Output = Result<(), ClientCoreError>> + Send + 'a>> {
			let state = Arc::clone(&self.state);
			Box::pin(async move {
				state.lock().unwrap().unsubscribe_calls.push(topics);
				Ok(())
			})
		}

		fn open_events_stream<'a>(
			&'a mut self,
		) -> Pin<Box<dyn Future<Output = Result<BoxedSessionEvents, ClientCoreError>> + Send + 'a>> {
			let state = Arc::clone(&self.state);
			Box::pin(async move {
				let (tx, rx) = oneshot::channel();
				let mut st = state.lock().unwrap();
				st.open_events += 1;
				st._events_tx = Some(tx);
				Ok(Box::new(MockSessionEvents { rx }) as BoxedSessionEvents)
			})
		}

		fn ping<'a>(
			&'a mut self,
			client_time_unix_ms: i64,
		) -> Pin<Box<dyn Future<Output = Result<pb::Pong, ClientCoreError>> + Send + 'a>> {
			Box::pin(async move {
				Ok(pb::Pong {
					client_time_unix_ms,
					server_time_unix_ms: client_time_unix_ms,
				})
			})
		}

		fn send_command<'a>(
			&'a mut self,
			_command: pb::Command,
		) -> Pin<Box<dyn Future<Output = Result<pb::CommandResult, ClientCoreError>> + Send + 'a>> {
			Box::pin(async move {
				Ok(pb::CommandResult {
					status: pb::command_result::Status::Ok as i32,
					detail: "ok".to_string(),
				})
			})
		}

		fn close(&self, code: u32, reason: &str) {
			self.state.lock().unwrap().close_calls.push((code, reason.to_string()));
		}
	}

	impl SessionEventsApi for MockSessionEvents {
		fn run_events_loop<'a>(
			&'a mut self,
			_on_event: Box<dyn FnMut(pb::EventEnvelope) + Send + 'a>,
		) -> Pin<Box<dyn Future<Output = Result<(), ClientCoreError>> + Send + 'a>> {
			let rx = &mut self.rx;
			Box::pin(async move {
				let _ = rx.await;
				Ok(())
			})
		}
	}
}
