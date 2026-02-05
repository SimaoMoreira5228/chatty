#![forbid(unsafe_code)]

use std::collections::HashSet;

use async_trait::async_trait;
use chatty_domain::{Platform, RoomKey};
use chatty_platform::{
	AdapterControl, AdapterControlRx, AdapterEventTx, CommandError, PermissionsInfo, PlatformAdapter, new_session_id, status,
};
use tracing::{debug, info};

/// Null adapter for unsupported platforms.
pub struct NullAdapter {
	platform: Platform,
}

impl NullAdapter {
	pub fn new(platform: Platform) -> Self {
		Self { platform }
	}
}

#[async_trait]
impl PlatformAdapter for NullAdapter {
	fn platform(&self) -> Platform {
		self.platform
	}

	async fn run(self: Box<Self>, mut control_rx: AdapterControlRx, events_tx: AdapterEventTx) -> anyhow::Result<()> {
		let platform = self.platform();
		let session_id = new_session_id();
		let _ = events_tx.try_send(status(
			platform,
			true,
			format!("null adapter online (session_id={session_id})"),
		));

		let mut joined: HashSet<RoomKey> = HashSet::new();
		info!(%platform, %session_id, "null adapter started");

		while let Some(cmd) = control_rx.recv().await {
			match cmd {
				AdapterControl::Join { room } => {
					if room.platform != platform {
						debug!(%platform, room=%room, "ignoring Join for non-matching platform");
						continue;
					}
					if joined.insert(room.clone()) {
						let _ = events_tx.try_send(status(platform, true, format!("joined {room}")));
					}
				}
				AdapterControl::Leave { room } => {
					if room.platform != platform {
						debug!(%platform, room=%room, "ignoring Leave for non-matching platform");
						continue;
					}
					if joined.remove(&room) {
						let _ = events_tx.try_send(status(platform, true, format!("left {room}")));
					}
				}
				AdapterControl::UpdateAuth { .. } => {
					let _ = events_tx.try_send(status(platform, true, "ignored UpdateAuth (null adapter)"));
				}
				AdapterControl::Command { resp, .. } => {
					let _ = resp.send(Err(CommandError::NotSupported(Some("null adapter".to_string()))));
				}
				AdapterControl::QueryPermissions { resp, .. } => {
					let _ = resp.send(PermissionsInfo::default());
				}
				AdapterControl::QueryAuth { resp } => {
					let _ = resp.send(None);
				}
				AdapterControl::Shutdown => {
					info!(%platform, "null adapter received Shutdown");
					break;
				}
			}
		}

		let _ = events_tx.try_send(status(platform, false, "null adapter offline"));
		Ok(())
	}
}
