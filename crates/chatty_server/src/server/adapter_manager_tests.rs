#![forbid(unsafe_code)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use chatty_domain::{Platform, RoomId, RoomKey};
use tokio::sync::RwLock;
use tokio::time::timeout;

use crate::adapters::DemoAdapter;
use crate::server::adapter_manager::{AdapterManager, AdapterManagerConfig};
use crate::server::state::GlobalState;

fn room(platform: Platform, id: &str) -> RoomKey {
	RoomKey::new(platform, RoomId::new(id.to_string()).expect("valid RoomId"))
}

#[tokio::test]
async fn demo_adapter_emits_after_join() {
	let state = Arc::new(RwLock::new(GlobalState::default()));
	let demo = DemoAdapter::new().with_emit_interval(Duration::from_millis(10));

	let manager = AdapterManager::start(
		Arc::clone(&state),
		vec![Box::new(demo)],
		AdapterManagerConfig {
			ingest_broadcast_capacity: 32,
			control_channel_capacity: 8,
			adapter_events_channel_capacity: 32,
		},
	);

	let mut rx = manager.subscribe_ingest();

	let topic = "room:twitch/demo".to_string();
	manager.apply_global_joins_leaves(&[topic], &[]).await;

	let deadline = Instant::now() + Duration::from_millis(750);
	let mut received = None;
	while Instant::now() < deadline {
		let remaining = deadline.saturating_duration_since(Instant::now());
		match timeout(remaining, rx.recv()).await {
			Ok(Ok(ev)) => {
				received = Some(ev);
				break;
			}
			Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
			Ok(Err(e)) => panic!("unexpected broadcast recv error: {e:?}"),
			Err(_) => break,
		}
	}

	let ev = received.expect("expected ingest event from demo adapter");
	let expected_room = room(Platform::Twitch, "demo");
	assert_eq!(ev.room, expected_room, "ingest event should target joined room");

	manager.shutdown().await;
}
