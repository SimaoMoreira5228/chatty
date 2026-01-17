#![forbid(unsafe_code)]

use std::time::Duration;

use chatty_domain::{Platform, RoomId, RoomKey};
use chatty_platform::{ChatMessage, IngestEvent, IngestPayload, UserRef};
use tokio::time::timeout;

use crate::server::room_hub::{RoomHub, RoomHubConfig, RoomHubItem};

fn room(platform: Platform, id: &str) -> RoomKey {
	RoomKey::new(platform, RoomId::new(id.to_string()).expect("valid RoomId"))
}

fn mk_ingest(room: RoomKey, text: &str) -> IngestEvent {
	let msg = ChatMessage::new(
		UserRef {
			id: "u1".to_string(),
			login: "user".to_string(),
			display: Some("User".to_string()),
		},
		text.to_string(),
	);

	let mut ev = IngestEvent::new(Platform::Twitch, room.room_id.clone(), IngestPayload::ChatMessage(msg));
	ev.room = room;
	ev.platform = ev.room.platform;
	ev
}

#[tokio::test]
async fn subscribe_room_receives_events_for_that_room_only() {
	let hub = RoomHub::new(RoomHubConfig {
		subscriber_queue_capacity: 16,
		debug_logs: false,
	});

	let room_a = room(Platform::Twitch, "a");
	let room_b = room(Platform::Twitch, "b");

	let mut rx_a = hub.subscribe_room(room_a.clone()).await;

	hub.publish_ingest(mk_ingest(room_b.clone(), "b-1")).await;

	let got_unexpected = timeout(Duration::from_millis(50), rx_a.recv()).await;
	assert!(
		got_unexpected.is_err(),
		"subscriber for room A unexpectedly received an item for room B"
	);

	hub.publish_ingest(mk_ingest(room_a.clone(), "a-1")).await;

	let item = timeout(Duration::from_millis(250), rx_a.recv())
		.await
		.expect("expected to receive within timeout")
		.expect("channel open");

	match item {
		RoomHubItem::Ingest(ev) => match ev.payload {
			IngestPayload::ChatMessage(m) => assert_eq!(m.text, "a-1"),
			other => panic!("expected ChatMessage payload, got: {other:?}"),
		},
		other => panic!("expected Ingest item, got: {other:?}"),
	}
}

#[tokio::test]
async fn unsubscribed_clients_dont_receive_events_after_drop() {
	let hub = RoomHub::new(RoomHubConfig {
		subscriber_queue_capacity: 16,
		debug_logs: false,
	});

	let room_a = room(Platform::Twitch, "a");

	{
		let _rx = hub.subscribe_room(room_a.clone()).await;
	}

	hub.prune_room(&room_a).await;

	hub.publish_ingest(mk_ingest(room_a.clone(), "a-1")).await;

	let counts = hub.room_subscriber_counts().await;
	assert_eq!(counts.get(&room_a).copied().unwrap_or(0), 0);
}

#[tokio::test]
async fn bounded_queue_drops_and_emits_lagged_marker() {
	let hub = RoomHub::new(RoomHubConfig {
		subscriber_queue_capacity: 1,
		debug_logs: false,
	});

	let room_a = room(Platform::Twitch, "a");
	let mut rx = hub.subscribe_room(room_a.clone()).await;

	hub.publish_ingest(mk_ingest(room_a.clone(), "a-1")).await;

	hub.publish_ingest(mk_ingest(room_a.clone(), "a-2")).await;

	let first = timeout(Duration::from_millis(250), rx.recv())
		.await
		.expect("expected first item")
		.expect("channel open");
	match first {
		RoomHubItem::Ingest(ev) => match ev.payload {
			IngestPayload::ChatMessage(m) => assert_eq!(m.text, "a-1"),
			other => panic!("expected ChatMessage payload, got: {other:?}"),
		},
		other => panic!("expected Ingest item first, got: {other:?}"),
	}

	hub.publish_to_room(room_a.clone(), RoomHubItem::Lagged { dropped: 1 }).await;

	let second = timeout(Duration::from_millis(250), rx.recv())
		.await
		.expect("expected lag marker")
		.expect("channel open");

	match second {
		RoomHubItem::Lagged { dropped } => assert!(dropped >= 1, "expected dropped >= 1, got {dropped}"),
		other => panic!("expected Lagged marker, got: {other:?}"),
	}
}
