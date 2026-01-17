#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chatty_domain::{Platform, RoomKey, RoomTopic};
use chatty_platform::{
	AdapterAuth, AdapterControl, AdapterEvent, CommandError, CommandRequest, IngestEvent, PermissionsInfo, PlatformAdapter,
};
use tokio::sync::{RwLock, broadcast, mpsc, oneshot};
use tracing::{debug, info, warn};

use crate::server::state::GlobalState;

/// Maximum number of in-flight ingest events buffered in the broadcast channel.
pub const DEFAULT_INGEST_BROADCAST_CAPACITY: usize = 8_192;

/// Adapter manager configuration.
#[derive(Debug, Clone)]
pub struct AdapterManagerConfig {
	pub ingest_broadcast_capacity: usize,
	pub control_channel_capacity: usize,
	pub adapter_events_channel_capacity: usize,
}

impl Default for AdapterManagerConfig {
	fn default() -> Self {
		Self {
			ingest_broadcast_capacity: DEFAULT_INGEST_BROADCAST_CAPACITY,
			control_channel_capacity: 512,
			adapter_events_channel_capacity: 8_192,
		}
	}
}

/// Subscription to global ingest events.
pub type IngestBroadcastRx = broadcast::Receiver<IngestEvent>;

/// Global adapter manager handle.
#[derive(Debug)]
pub struct AdapterManager {
	state: Arc<RwLock<GlobalState>>,

	control_by_platform: HashMap<Platform, mpsc::Sender<AdapterControl>>,

	joined_rooms: Arc<RwLock<HashSet<RoomKey>>>,

	ingest_tx: broadcast::Sender<IngestEvent>,

	#[allow(dead_code)]
	shutdown_tx: Option<oneshot::Sender<()>>,
}

impl AdapterManager {
	/// Create and start the global adapter manager and its platform adapters.
	pub fn start(
		state: Arc<RwLock<GlobalState>>,
		platform_adapters: Vec<Box<dyn PlatformAdapter>>,
		cfg: AdapterManagerConfig,
	) -> Self {
		let (ingest_tx, _ingest_rx) = broadcast::channel(cfg.ingest_broadcast_capacity);

		let joined_rooms: Arc<RwLock<HashSet<RoomKey>>> = Arc::new(RwLock::new(HashSet::new()));

		let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
		let shutdown_rx = Arc::new(tokio::sync::Mutex::new(Some(shutdown_rx)));

		let mut control_by_platform: HashMap<Platform, mpsc::Sender<AdapterControl>> = HashMap::new();

		for adapter in platform_adapters {
			let platform = adapter.platform();

			let (control_tx, control_rx) = mpsc::channel::<AdapterControl>(cfg.control_channel_capacity);
			let (events_tx, events_rx) = mpsc::channel::<AdapterEvent>(cfg.adapter_events_channel_capacity);

			let adapter_box = adapter;
			tokio::spawn(async move {
				if let Err(e) = adapter_box.run(control_rx, events_tx).await {
					warn!(%platform, error=%e, "platform adapter task exited with error");
				}
			});

			Self::spawn_adapter_forwarder(platform, events_rx, ingest_tx.clone(), shutdown_rx.clone());

			control_by_platform.insert(platform, control_tx);
		}

		Self {
			state,
			control_by_platform,
			joined_rooms,
			ingest_tx,
			shutdown_tx: Some(shutdown_tx),
		}
	}

	fn spawn_adapter_forwarder(
		platform: Platform,
		mut events_rx: mpsc::Receiver<AdapterEvent>,
		ingest_tx: broadcast::Sender<IngestEvent>,
		shutdown_rx: Arc<tokio::sync::Mutex<Option<oneshot::Receiver<()>>>>,
	) {
		tokio::spawn(async move {
			let mut maybe_shutdown = shutdown_rx.lock().await.take();

			loop {
				tokio::select! {
					ev = events_rx.recv() => {
						let Some(ev) = ev else {
							debug!(%platform, "adapter events channel closed; forwarder exiting");
							break;
						};

						match ev {
							AdapterEvent::Ingest(ingest) => {
								let _ = ingest_tx.send(*ingest);
							}
							AdapterEvent::Status(st) => {
								metrics::counter!("chatty_server_adapter_status_total").increment(1);
								metrics::gauge!("chatty_server_adapter_connected").set(if st.connected { 1.0 } else { 0.0 });
								if st.connected {
									metrics::counter!("chatty_server_adapter_connected_total").increment(1);
								} else {
									metrics::counter!("chatty_server_adapter_disconnected_total").increment(1);
								}
								if st.last_error.is_some() {
									metrics::counter!("chatty_server_adapter_status_errors_total").increment(1);
								}
								debug!(
									%platform,
									connected = st.connected,
									detail = %st.detail,
									last_error = ?st.last_error,
									"adapter status"
								);
							}
						}
					}

					_ = async {
						if let Some(rx) = &mut maybe_shutdown {
							let _ = rx.await;
						}
					}, if maybe_shutdown.is_some() => {
						info!(%platform, "adapter forwarder observed shutdown");
						break;
					}
				}
			}
		});
	}

	/// Subscribe to global ingest events.
	pub fn subscribe_ingest(&self) -> IngestBroadcastRx {
		self.ingest_tx.subscribe()
	}

	/// Update authentication for a specific platform adapter (best-effort).
	pub async fn update_auth(&self, platform: Platform, auth: AdapterAuth) -> bool {
		let Some(ctrl) = self.control_by_platform.get(&platform) else {
			return false;
		};
		ctrl.send(AdapterControl::UpdateAuth { auth }).await.is_ok()
	}

	/// Execute a command against a specific platform adapter.
	pub async fn execute_command(&self, request: CommandRequest) -> Result<(), CommandError> {
		let platform = request.platform();
		let Some(ctrl) = self.control_by_platform.get(&platform) else {
			return Err(CommandError::NotSupported(Some(format!(
				"platform {platform} not configured"
			))));
		};
		let (tx, rx) = oneshot::channel();
		if ctrl.send(AdapterControl::Command { request, resp: tx }).await.is_err() {
			return Err(CommandError::Internal("adapter control channel closed".to_string()));
		}
		match tokio::time::timeout(std::time::Duration::from_secs(5), rx).await {
			Ok(Ok(result)) => result,
			Ok(Err(_)) => Err(CommandError::Internal("adapter response dropped".to_string())),
			Err(_) => Err(CommandError::Internal("adapter command timed out".to_string())),
		}
	}

	/// Query permission snapshot for a room.
	pub async fn query_permissions(&self, room: &RoomKey) -> Option<PermissionsInfo> {
		let Some(ctrl) = self.control_by_platform.get(&room.platform) else {
			return None;
		};
		let (tx, rx) = oneshot::channel();
		if ctrl
			.send(AdapterControl::QueryPermissions {
				room: room.clone(),
				resp: tx,
			})
			.await
			.is_err()
		{
			return None;
		}

		match tokio::time::timeout(std::time::Duration::from_secs(3), rx).await {
			Ok(Ok(result)) => Some(result),
			_ => None,
		}
	}

	/// Apply join/leave changes based on topics that crossed refcount thresholds.
	pub async fn apply_global_joins_leaves(&self, topics_to_join: &[String], topics_to_leave: &[String]) {
		let mut join_rooms: Vec<RoomKey> = Vec::new();
		for t in topics_to_join {
			if let Some(r) = topic_to_room_key(t) {
				join_rooms.push(r);
			}
		}

		let mut leave_rooms: Vec<RoomKey> = Vec::new();
		for t in topics_to_leave {
			if let Some(r) = topic_to_room_key(t) {
				leave_rooms.push(r);
			}
		}

		{
			let mut joined = self.joined_rooms.write().await;

			for room in join_rooms {
				if joined.contains(&room) {
					continue;
				}
				if let Some(ctrl) = self.control_by_platform.get(&room.platform) {
					if ctrl.send(AdapterControl::Join { room: room.clone() }).await.is_ok() {
						joined.insert(room.clone());
						debug!(room=%room, "issued global Join");
					}
				} else {
					debug!(room=%room, "no adapter registered for platform; ignoring Join");
				}
			}

			for room in leave_rooms {
				if !joined.contains(&room) {
					continue;
				}
				if let Some(ctrl) = self.control_by_platform.get(&room.platform) {
					if ctrl.send(AdapterControl::Leave { room: room.clone() }).await.is_ok() {
						joined.remove(&room);
						debug!(room=%room, "issued global Leave");
					}
				} else {
					debug!(room=%room, "no adapter registered for platform; ignoring Leave");
					joined.remove(&room);
				}
			}
		}
	}

	/// Recompute desired rooms from `GlobalState` and reconcile joins.
	#[allow(dead_code)]
	pub async fn reconcile_from_state_snapshot(&self) {
		let snapshot = {
			let st = self.state.read().await;
			st.topic_refcounts_snapshot()
		};

		let mut desired: HashSet<RoomKey> = HashSet::new();
		for (topic, rc) in snapshot {
			if rc == 0 {
				continue;
			}
			if let Some(room) = topic_to_room_key(&topic) {
				desired.insert(room);
			}
		}

		let mut to_join: Vec<RoomKey> = Vec::new();
		let mut to_leave: Vec<RoomKey> = Vec::new();

		{
			let joined = self.joined_rooms.read().await;

			for room in desired.iter() {
				if !joined.contains(room) {
					to_join.push(room.clone());
				}
			}
			for room in joined.iter() {
				if !desired.contains(room) {
					to_leave.push(room.clone());
				}
			}
		}

		for room in to_join {
			if let Some(ctrl) = self.control_by_platform.get(&room.platform) {
				let _ = ctrl.send(AdapterControl::Join { room: room.clone() }).await;
				self.joined_rooms.write().await.insert(room);
			}
		}
		for room in to_leave {
			if let Some(ctrl) = self.control_by_platform.get(&room.platform) {
				let _ = ctrl.send(AdapterControl::Leave { room: room.clone() }).await;
			}
			self.joined_rooms.write().await.remove(&room);
		}
	}

	/// Shutdown the adapter manager.
	#[allow(dead_code)]
	pub async fn shutdown(mut self) {
		if let Some(tx) = self.shutdown_tx.take() {
			let _ = tx.send(());
		}

		for (platform, ctrl) in self.control_by_platform.drain() {
			let _ = ctrl.send(AdapterControl::Shutdown).await;
			debug!(%platform, "sent adapter Shutdown");
		}
	}
}

/// Parse a topic into a `RoomKey` using the v1 topic format.
fn topic_to_room_key(topic: &str) -> Option<RoomKey> {
	RoomTopic::parse(topic).ok()
}

/// Start a global adapter manager for v1.
pub fn start_global_adapter_manager(
	state: Arc<RwLock<GlobalState>>,
	cfg: AdapterManagerConfig,
	platform_adapters: Vec<Box<dyn PlatformAdapter>>,
) -> AdapterManager {
	AdapterManager::start(state, platform_adapters, cfg)
}
