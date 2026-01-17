#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use chatty_domain::{Platform, RoomId, RoomKey};
use chatty_platform::{
	AdapterControl, AdapterControlRx, AdapterEvent, AdapterEventTx, ChatMessage, IngestEvent, IngestPayload,
	PermissionsInfo, PlatformAdapter, UserRef, new_session_id, status, validate_ingest_event,
};
use tokio::time::Interval;
use tracing::{debug, info, warn};

/// Stub adapter used for end-to-end ingestion tests.
pub struct DemoAdapter {
	emit_interval: Duration,
}

impl DemoAdapter {
	pub fn new() -> Self {
		Self {
			emit_interval: Duration::from_millis(250),
		}
	}

	/// Customize emit interval (useful for tests).
	#[allow(dead_code)]
	pub fn with_emit_interval(mut self, interval: Duration) -> Self {
		self.emit_interval = interval;
		self
	}

	fn make_user() -> UserRef {
		UserRef {
			id: "demo-user-id".to_string(),
			login: "demo_user".to_string(),
			display: Some("DemoUser".to_string()),
		}
	}

	fn room_display(room: &RoomKey) -> String {
		format!("room:{}/{}", room.platform.as_str(), room.room_id.as_str())
	}

	fn make_event(platform: Platform, room_id: &RoomId, n: u64, session_id: &str) -> IngestEvent {
		let author = Self::make_user();
		let text = format!("demo ingest message #{n} in {}", room_id.as_str());

		let mut msg = ChatMessage::new(author, text);

		msg.ids.platform_id = None;

		let trace = chatty_platform::IngestTrace {
			session_id: Some(session_id.to_string()),
			local_seq: Some(n),
			..Default::default()
		};

		let mut ev = IngestEvent::new(platform, room_id.clone(), IngestPayload::ChatMessage(msg));
		ev.ingest_time = SystemTime::now();
		ev.platform_time = None;
		ev.trace = trace;
		ev
	}

	async fn emit_one_tick(events_tx: &AdapterEventTx, joined: &HashSet<RoomKey>, tick: &mut u64, session_id: &str) {
		for room in joined {
			*tick += 1;

			let ev = Self::make_event(room.platform, &room.room_id, *tick, session_id);

			if let Err(e) = validate_ingest_event(&ev) {
				warn!(
					platform = %room.platform,
					room = %room,
					error = %e,
					"dropping invalid demo ingest event"
				);
				continue;
			}

			if events_tx.try_send(AdapterEvent::Ingest(Box::new(ev))).is_err() {
				warn!("demo adapter events channel full; dropping ingest event");
				return;
			}
		}
	}
}

#[async_trait]
impl PlatformAdapter for DemoAdapter {
	fn platform(&self) -> Platform {
		Platform::Twitch
	}

	async fn run(self: Box<Self>, mut control_rx: AdapterControlRx, events_tx: AdapterEventTx) -> anyhow::Result<()> {
		let platform = self.platform();
		let session_id = new_session_id();

		let _ = events_tx.try_send(status(
			platform,
			true,
			format!("demo adapter online (session_id={session_id})"),
		));

		let mut joined: HashSet<RoomKey> = HashSet::new();
		let mut tick: u64 = 0;

		let mut interval: Interval = tokio::time::interval(self.emit_interval);
		interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

		info!(%platform, %session_id, "demo adapter started");
		debug!(%platform, interval_ms = self.emit_interval.as_millis(), "demo adapter tick interval configured");

		loop {
			tokio::select! {
				_ = interval.tick() => {
					if joined.is_empty() {
						continue;
					}
					Self::emit_one_tick(&events_tx, &joined, &mut tick, &session_id).await;
				}

				cmd = control_rx.recv() => {
					let Some(cmd) = cmd else {
						info!(%platform, "demo adapter control channel closed; shutting down");
						break;
					};

					match cmd {
						AdapterControl::Join { room } => {
							if room.platform != platform {
								debug!(%platform, room = %room, "ignoring Join for non-matching platform");
								continue;
							}

							let newly_inserted = joined.insert(room.clone());
							if newly_inserted {
								let detail = format!("joined {}", Self::room_display(&room));
								let _ = events_tx.try_send(status(platform, true, detail));
								info!(%platform, room=%room, "demo adapter joined room");
							}
						}

						AdapterControl::Leave { room } => {
							if room.platform != platform {
								debug!(%platform, room = %room, "ignoring Leave for non-matching platform");
								continue;
							}

							let removed = joined.remove(&room);
							if removed {
								let detail = format!("left {}", Self::room_display(&room));
								let _ = events_tx.try_send(status(platform, true, detail));
								info!(%platform, room=%room, "demo adapter left room");
							}
						}

						AdapterControl::UpdateAuth { .. } => {
							let _ = events_tx.try_send(status(platform, true, "ignored UpdateAuth (demo adapter)"));
						}

						AdapterControl::Command { resp, .. } => {
							let _ = resp.send(Err(chatty_platform::CommandError::NotSupported(Some(
								"demo adapter".to_string(),
							))));
						}

						AdapterControl::QueryPermissions { resp, .. } => {
							let _ = resp.send(PermissionsInfo::default());
						}

						AdapterControl::Shutdown => {
							info!(%platform, "demo adapter received Shutdown");
							break;
						}
					}
				}
			}
		}

		let _ = events_tx.try_send(status(platform, false, "demo adapter offline"));
		Ok(())
	}
}
