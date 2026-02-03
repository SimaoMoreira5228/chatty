#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

const SEVENTV_EVENT_API_URL: &str = "wss://events.7tv.io/v3";
const SEVENTV_EVENT_API_RECONNECT_DELAY: Duration = Duration::from_secs(1);

static SEVENTV_EVENT_API: OnceLock<SevenTvEventApi> = OnceLock::new();
static SEVENTV_EVENT_API_HANDLER_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct SevenTvEventApi {
	command_tx: mpsc::UnboundedSender<Command>,
}

pub fn ensure_seventv_event_api() -> SevenTvEventApi {
	SEVENTV_EVENT_API
		.get_or_init(|| {
			let (command_tx, command_rx) = mpsc::unbounded_channel();
			tokio::spawn(run_event_api(command_rx));
			SevenTvEventApi { command_tx }
		})
		.clone()
}

impl SevenTvEventApi {
	pub fn subscribe(
		&self,
		dispatch_type: DispatchType,
		object_id: impl Into<String>,
	) -> (SevenTvSubscription, mpsc::UnboundedReceiver<DispatchPayload>) {
		let object_id = object_id.into();
		let (tx, rx) = mpsc::unbounded_channel();
		let handler_id = SEVENTV_EVENT_API_HANDLER_ID.fetch_add(1, Ordering::Relaxed);
		let key = SubscriptionKey {
			dispatch_type: dispatch_type.clone(),
			object_id,
		};

		let _ = self.command_tx.send(Command::Subscribe {
			key: key.clone(),
			handler_id,
			handler: tx,
		});

		(
			SevenTvSubscription {
				handler_id,
				key,
				command_tx: self.command_tx.clone(),
			},
			rx,
		)
	}
}

pub struct SevenTvSubscription {
	handler_id: u64,
	key: SubscriptionKey,
	command_tx: mpsc::UnboundedSender<Command>,
}

impl Drop for SevenTvSubscription {
	fn drop(&mut self) {
		let _ = self.command_tx.send(Command::Unsubscribe {
			key: self.key.clone(),
			handler_id: self.handler_id,
		});
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SubscriptionKey {
	dispatch_type: DispatchType,
	object_id: String,
}

#[derive(Debug)]
enum Command {
	Subscribe {
		key: SubscriptionKey,
		handler_id: u64,
		handler: mpsc::UnboundedSender<DispatchPayload>,
	},
	Unsubscribe {
		key: SubscriptionKey,
		handler_id: u64,
	},
}

#[derive(Debug, Default)]
struct ManagerState {
	subscriptions: HashMap<SubscriptionKey, HashSet<u64>>,
	handlers: HashMap<u64, mpsc::UnboundedSender<DispatchPayload>>,
}

impl ManagerState {
	fn add_subscription(
		&mut self,
		key: SubscriptionKey,
		handler_id: u64,
		handler: mpsc::UnboundedSender<DispatchPayload>,
	) -> bool {
		self.handlers.insert(handler_id, handler);
		let is_new_key = !self.subscriptions.contains_key(&key);
		let handlers = self.subscriptions.entry(key).or_default();
		handlers.insert(handler_id);
		is_new_key
	}

	fn remove_subscription(&mut self, key: &SubscriptionKey, handler_id: u64) -> bool {
		self.handlers.remove(&handler_id);
		let Some(handlers) = self.subscriptions.get_mut(key) else {
			return false;
		};

		let removed = handlers.remove(&handler_id);
		if handlers.is_empty() {
			self.subscriptions.remove(key);
		}
		removed && !self.subscriptions.contains_key(key)
	}
}

async fn run_event_api(mut command_rx: mpsc::UnboundedReceiver<Command>) {
	let mut state = ManagerState::default();

	loop {
		info!(url = %SEVENTV_EVENT_API_URL, "connecting to 7tv event api");
		let (mut ws, _) = match tokio_tungstenite::connect_async(SEVENTV_EVENT_API_URL).await {
			Ok(result) => result,
			Err(err) => {
				warn!(error = %err, "7tv event api connect failed");
				tokio::time::sleep(SEVENTV_EVENT_API_RECONNECT_DELAY).await;
				continue;
			}
		};

		let mut connected = false;

		loop {
			tokio::select! {
				cmd = command_rx.recv() => {
					let Some(cmd) = cmd else {
						info!("7tv event api command channel closed");
						return;
					};
					handle_command(cmd, &mut state, connected, &mut ws).await;
				}
				msg = ws.next() => {
					let Some(msg) = msg else {
						warn!("7tv event api websocket closed");
						break;
					};
					match msg {
						Ok(Message::Text(text)) => {
							if let Ok(envelope) = serde_json::from_str::<EventApiEnvelope>(&text) {
								match envelope.op {
									0 => {
										if let Ok(dispatch) = serde_json::from_value::<DispatchPayload>(envelope.d) {
											dispatch_payload(dispatch, &state);
										}
									}
									1 => {
										connected = true;
										debug!("7tv event api hello received");
										resubscribe_all(&state, &mut ws).await;
									}
									2 => {
										debug!("7tv event api heartbeat");
									}
									4 => {
										warn!("7tv event api reconnect requested");
										break;
									}
									6 => {
										warn!("7tv event api error: {text}");
									}
									_ => {}
								}
							}
						}
						Ok(Message::Close(frame)) => {
							warn!(?frame, "7tv event api websocket closed");
							break;
						}
						Ok(_) => {}
						Err(err) => {
							warn!(error = %err, "7tv event api websocket error");
							break;
						}
					}
				}
			}
		}

		tokio::time::sleep(SEVENTV_EVENT_API_RECONNECT_DELAY).await;
	}
}

async fn handle_command(
	cmd: Command,
	state: &mut ManagerState,
	connected: bool,
	ws: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
) {
	match cmd {
		Command::Subscribe {
			key,
			handler_id,
			handler,
		} => {
			let is_new = state.add_subscription(key.clone(), handler_id, handler);
			if is_new && connected {
				send_subscribe(ws, &key).await;
			}
		}
		Command::Unsubscribe { key, handler_id } => {
			let should_unsubscribe = state.remove_subscription(&key, handler_id);
			if should_unsubscribe && connected {
				send_unsubscribe(ws, &key).await;
			}
		}
	}
}

async fn resubscribe_all(
	state: &ManagerState,
	ws: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
) {
	for key in state.subscriptions.keys() {
		send_subscribe(ws, key).await;
	}
}

async fn send_subscribe(
	ws: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
	key: &SubscriptionKey,
) {
	let payload = SubscribeMessage {
		op: 35,
		d: SubscribeData {
			dispatch_type: key.dispatch_type.clone(),
			condition: Condition {
				object_id: key.object_id.clone(),
			},
		},
	};

	if let Ok(text) = serde_json::to_string(&payload)
		&& let Err(err) = ws.send(Message::Text(text.into())).await
	{
		warn!(error = %err, "7tv event api subscribe send failed");
	}
}

async fn send_unsubscribe(
	ws: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
	key: &SubscriptionKey,
) {
	let payload = UnsubscribeMessage {
		op: 36,
		d: SubscribeData {
			dispatch_type: key.dispatch_type.clone(),
			condition: Condition {
				object_id: key.object_id.clone(),
			},
		},
	};

	if let Ok(text) = serde_json::to_string(&payload)
		&& let Err(err) = ws.send(Message::Text(text.into())).await
	{
		warn!(error = %err, "7tv event api unsubscribe send failed");
	}
}

fn dispatch_payload(payload: DispatchPayload, state: &ManagerState) {
	let key = SubscriptionKey {
		dispatch_type: payload.dispatch_type.clone(),
		object_id: payload.body.id.clone(),
	};
	let Some(handlers) = state.subscriptions.get(&key) else {
		return;
	};

	for handler_id in handlers {
		if let Some(sender) = state.handlers.get(handler_id) {
			let _ = sender.send(payload.clone());
		}
	}
}

#[derive(Debug, Deserialize)]
struct EventApiEnvelope {
	op: u32,
	d: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DispatchType {
	#[serde(rename = "system.announcement")]
	SystemAnnouncement,
	#[serde(rename = "emote.create")]
	EmoteCreate,
	#[serde(rename = "emote.update")]
	EmoteUpdate,
	#[serde(rename = "emote.delete")]
	EmoteDelete,
	#[serde(rename = "emote_set.create")]
	EmoteSetCreate,
	#[serde(rename = "emote_set.update")]
	EmoteSetUpdate,
	#[serde(rename = "emote_set.delete")]
	EmoteSetDelete,
	#[serde(rename = "user.create")]
	UserCreate,
	#[serde(rename = "user.update")]
	UserUpdate,
	#[serde(rename = "user.delete")]
	UserDelete,
	#[serde(rename = "user.add_connection")]
	UserAddConnection,
	#[serde(rename = "user.update_connection")]
	UserUpdateConnection,
	#[serde(rename = "user.delete_connection")]
	UserDeleteConnection,
	#[serde(rename = "cosmetic.create")]
	CosmeticCreate,
	#[serde(rename = "cosmetic.update")]
	CosmeticUpdate,
	#[serde(rename = "cosmetic.delete")]
	CosmeticDelete,
	#[serde(rename = "entitlement.create")]
	EntitlementCreate,
	#[serde(rename = "entitlement.update")]
	EntitlementUpdate,
	#[serde(rename = "entitlement.delete")]
	EntitlementDelete,
	#[serde(other)]
	Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchPayload {
	#[serde(rename = "type")]
	pub dispatch_type: DispatchType,
	pub body: DispatchBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchBody {
	pub id: String,
	pub kind: i32,
	pub added: Option<Vec<ChangeField>>,
	pub updated: Option<Vec<ChangeField>>,
	pub removed: Option<Vec<ChangeField>>,
	pub pushed: Option<Vec<ChangeField>>,
	pub pulled: Option<Vec<ChangeField>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeField {
	pub key: String,
	pub index: Option<i32>,
	pub old_value: Option<serde_json::Value>,
	pub value: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct SubscribeMessage {
	op: u32,
	d: SubscribeData,
}

#[derive(Debug, Serialize)]
struct UnsubscribeMessage {
	op: u32,
	d: SubscribeData,
}

#[derive(Debug, Serialize)]
struct SubscribeData {
	#[serde(rename = "type")]
	dispatch_type: DispatchType,
	condition: Condition,
}

#[derive(Debug, Serialize)]
struct Condition {
	#[serde(rename = "object_id")]
	object_id: String,
}
