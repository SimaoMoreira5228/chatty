#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::Arc;

use chatty_domain::RoomKey;
use chatty_platform::{AdapterStatus, IngestEvent};
use tokio::sync::{Mutex, mpsc};
use tracing::debug;

/// Per-room hub that fans out ingest events and adapter status updates.
#[derive(Debug, Clone)]
pub struct RoomHub {
	inner: Arc<Mutex<Inner>>,
	cfg: RoomHubConfig,
}

/// Configuration for `RoomHub`.
#[derive(Debug, Clone)]
pub struct RoomHubConfig {
	/// Maximum number of queued messages per subscriber.
	pub subscriber_queue_capacity: usize,

	pub debug_logs: bool,
}

impl Default for RoomHubConfig {
	fn default() -> Self {
		Self {
			subscriber_queue_capacity: 1024,
			debug_logs: false,
		}
	}
}

/// Items emitted on a subscriber stream.
#[derive(Debug, Clone)]
pub enum RoomHubItem {
	Ingest(Box<IngestEvent>),

	#[allow(dead_code)]
	Status(AdapterStatus),

	/// Indicates the subscriber is lagging and items were dropped.
	Lagged {
		dropped: u64,
	},
}

/// Handle used to publish to a room.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RoomPublisher {
	hub: RoomHub,
	room: RoomKey,
}

impl RoomHub {
	pub fn new(cfg: RoomHubConfig) -> Self {
		Self {
			inner: Arc::new(Mutex::new(Inner::default())),
			cfg,
		}
	}

	/// Subscribe to a room.
	pub async fn subscribe_room(&self, room: RoomKey) -> mpsc::Receiver<RoomHubItem> {
		let (tx, rx) = mpsc::channel(self.cfg.subscriber_queue_capacity);

		let mut inner = self.inner.lock().await;
		let entry = inner.rooms.entry(room.clone()).or_default();

		prune_closed_subscribers(entry);

		entry.subscribers.push(tx);
		entry.pending_lag_by_subscriber.push(0);

		if self.cfg.debug_logs {
			debug!(room = %room, subs = entry.subscribers.len(), "room hub: subscribed");
		}

		rx
	}

	/// Unsubscribe bookkeeping for a given room.
	#[allow(dead_code)]
	pub async fn prune_room(&self, room: &RoomKey) {
		let mut inner = self.inner.lock().await;
		if let Some(entry) = inner.rooms.get_mut(room) {
			prune_closed_subscribers(entry);

			if entry.subscribers.is_empty() {
				inner.rooms.remove(room);
			}
		}
	}

	#[allow(dead_code)]
	pub fn publisher(&self, room: RoomKey) -> RoomPublisher {
		RoomPublisher { hub: self.clone(), room }
	}

	/// Publish an ingest event to subscribers of `event.room`.
	#[allow(dead_code)]
	pub async fn publish_ingest(&self, event: IngestEvent) {
		let room = event.room.clone();
		self.publish_to_room(room, RoomHubItem::Ingest(Box::new(event))).await;
	}

	/// Publish an adapter status event to subscribers of a room.
	pub async fn publish_status(&self, room: RoomKey, status: AdapterStatus) {
		self.publish_to_room(room, RoomHubItem::Status(status)).await;
	}

	/// Internal publish helper used by the server routing layer.
	pub(crate) async fn publish_to_room(&self, room: RoomKey, item: RoomHubItem) {
		let mut inner = self.inner.lock().await;
		let Some(entry) = inner.rooms.get_mut(&room) else {
			return;
		};

		prune_closed_subscribers(entry);

		if entry.subscribers.is_empty() {
			inner.rooms.remove(&room);
			return;
		}

		let mut dropped_total: u64 = 0;

		for (idx, sub) in entry.subscribers.iter_mut().enumerate() {
			match sub.try_send(item.clone()) {
				Ok(()) => {
					if let Some(pending) = entry.pending_lag_by_subscriber.get_mut(idx)
						&& *pending > 0 && sub.try_send(RoomHubItem::Lagged { dropped: *pending }).is_ok()
					{
						*pending = 0;
					}
				}
				Err(mpsc::error::TrySendError::Full(_)) => {
					dropped_total += 1;

					if let Some(pending) = entry.pending_lag_by_subscriber.get_mut(idx) {
						*pending = pending.saturating_add(1);
					}
				}
				Err(mpsc::error::TrySendError::Closed(_)) => {}
			}
		}

		prune_closed_subscribers(entry);

		if entry.subscribers.is_empty() {
			inner.rooms.remove(&room);
		}

		if self.cfg.debug_logs && dropped_total > 0 {
			debug!(
				room = %room,
				dropped = dropped_total,
				"room hub: dropped due to full subscriber queues"
			);
		}
	}

	/// Get a snapshot of subscriber counts per room.
	#[allow(dead_code)]
	pub async fn room_subscriber_counts(&self) -> HashMap<RoomKey, usize> {
		let inner = self.inner.lock().await;
		inner
			.rooms
			.iter()
			.map(|(k, v)| (k.clone(), v.subscribers.iter().filter(|s| !s.is_closed()).count()))
			.collect()
	}
}

impl RoomPublisher {
	/// Publish an ingest event into this publisher's room.
	#[allow(dead_code)]
	pub async fn publish_ingest(&self, event: IngestEvent) {
		self.hub.publish_ingest(event).await;
	}

	#[allow(dead_code)]
	pub async fn publish_status(&self, status: AdapterStatus) {
		self.hub.publish_status(self.room.clone(), status).await;
	}

	#[allow(dead_code)]
	pub async fn publish_item(&self, item: RoomHubItem) {
		self.hub.publish_to_room(self.room.clone(), item).await;
	}

	#[allow(dead_code)]
	pub fn room(&self) -> &RoomKey {
		&self.room
	}
}

#[derive(Debug, Default)]
struct Inner {
	rooms: HashMap<RoomKey, RoomEntry>,
}

#[derive(Debug, Default)]
struct RoomEntry {
	subscribers: Vec<mpsc::Sender<RoomHubItem>>,

	/// Pending lag markers per subscriber.
	pending_lag_by_subscriber: Vec<u64>,
}

fn prune_closed_subscribers(entry: &mut RoomEntry) {
	if entry.subscribers.len() != entry.pending_lag_by_subscriber.len() {
		entry.pending_lag_by_subscriber.resize(entry.subscribers.len(), 0);
	}

	let mut new_subs = Vec::with_capacity(entry.subscribers.len());
	let mut new_lag = Vec::with_capacity(entry.subscribers.len());

	for (idx, s) in entry.subscribers.drain(..).enumerate() {
		if !s.is_closed() {
			new_subs.push(s);
			new_lag.push(*entry.pending_lag_by_subscriber.get(idx).unwrap_or(&0));
		}
	}

	entry.subscribers = new_subs;
	entry.pending_lag_by_subscriber = new_lag;
}
