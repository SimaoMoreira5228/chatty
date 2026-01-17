#![forbid(unsafe_code)]

use std::sync::Arc;

use chatty_platform::IngestEvent;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use crate::server::adapter_manager::IngestBroadcastRx;
use crate::server::room_hub::{RoomHub, RoomHubItem};

/// Settings for the ingest router.
#[derive(Debug, Clone)]
pub struct RouterConfig {
	pub debug_log_events: bool,

	pub log_upstream_lag: bool,
}

impl Default for RouterConfig {
	fn default() -> Self {
		Self {
			debug_log_events: false,
			log_upstream_lag: true,
		}
	}
}

/// Router that consumes the global ingest broadcast and republishes into the per-room hub.
#[derive(Debug)]
pub struct IngestRouter {
	cfg: RouterConfig,
	room_hub: RoomHub,
	ingest_rx: IngestBroadcastRx,
}

impl IngestRouter {
	/// Create a router from an existing ingest receiver and a `RoomHub`.
	pub fn new(ingest_rx: IngestBroadcastRx, room_hub: RoomHub, cfg: RouterConfig) -> Self {
		Self {
			cfg,
			room_hub,
			ingest_rx,
		}
	}

	/// Create a router by subscribing to the adapter manager broadcast.
	pub fn from_adapter_manager(
		adapter_manager: &crate::server::adapter_manager::AdapterManager,
		room_hub: RoomHub,
		cfg: RouterConfig,
	) -> Self {
		Self::new(adapter_manager.subscribe_ingest(), room_hub, cfg)
	}

	/// Run the routing loop until the upstream broadcast is closed.
	pub async fn run(mut self) {
		info!("ingest router started");

		loop {
			let ingest = match self.ingest_rx.recv().await {
				Ok(ev) => ev,
				Err(broadcast::error::RecvError::Lagged(n)) => {
					if self.cfg.log_upstream_lag {
						warn!(
							lagged = n,
							"ingest router lagged on global broadcast; some ingest events may be dropped before routing"
						);
					}
					continue;
				}
				Err(broadcast::error::RecvError::Closed) => {
					info!("ingest router exiting (upstream ingest broadcast closed)");
					break;
				}
			};

			if self.cfg.debug_log_events {
				debug!(
					room = %ingest.room,
					platform = %ingest.platform,
					"routing ingest event to room hub"
				);
			}

			self.room_hub
				.publish_to_room(ingest.room.clone(), RoomHubItem::Ingest(ingest))
				.await;
		}
	}

	/// Access the hub.
	#[allow(dead_code)]
	pub fn room_hub(&self) -> &RoomHub {
		&self.room_hub
	}
}

/// Spawn a background task that routes ingest events into the room hub.
pub fn spawn_ingest_router(
	adapter_manager: Arc<crate::server::adapter_manager::AdapterManager>,
	room_hub: RoomHub,
	cfg: RouterConfig,
) -> RoomHub {
	let router = IngestRouter::from_adapter_manager(&adapter_manager, room_hub.clone(), cfg);

	tokio::spawn(async move {
		router.run().await;
	});

	room_hub
}

/// Route a single event (useful in tests).
#[allow(dead_code)]
pub async fn route_one(room_hub: &RoomHub, ingest: IngestEvent) {
	room_hub
		.publish_to_room(ingest.room.clone(), RoomHubItem::Ingest(ingest))
		.await;
}
