#![forbid(unsafe_code)]

use std::time::{Duration, SystemTime};

use chatty_domain::{Platform, RoomId, RoomKey};

use super::{decode_channel_moderate_to_ingest, should_emit_payload};
use crate::{IngestPayload, ModerationAction};

fn mk_room(login: &str) -> RoomKey {
	RoomKey::new(Platform::Twitch, RoomId::new(login.to_string()).expect("valid room id"))
}

fn mk_moderate(
	room_login: &str,
	action: &str,
	action_data: Option<serde_json::Value>,
) -> crate::twitch::eventsub::NormalizedChannelModerateNotification {
	let now = SystemTime::now();
	crate::twitch::eventsub::NormalizedChannelModerateNotification {
		platform: Platform::Twitch,
		room: mk_room(room_login),
		ws_message_id: "ws-msg-1".to_string(),
		subscription_id: "sub-1".to_string(),
		platform_time: now,

		moderator_user_id: "mod-id".to_string(),
		moderator_user_login: "modlogin".to_string(),
		moderator_user_name: "Moderator".to_string(),

		action: action.to_string(),
		action_data,
	}
}

#[test]
fn mod_gating_allows_non_moderation_payloads_even_when_not_mod() {
	let room_state = IngestPayload::RoomState(crate::RoomState {
		flags: Default::default(),
		settings: Default::default(),
		actor: None,
		notes: None,
	});

	let notice = IngestPayload::UserNotice(crate::UserNotice {
		kind: "raid".to_string(),
		text: Some("hello".to_string()),
		user: None,
	});

	let chat = IngestPayload::ChatMessage(crate::ChatMessage {
		ids: crate::IngestMessageIds {
			server_id: uuid::Uuid::new_v4(),
			platform_id: None,
		},
		author: crate::UserRef {
			id: "u".to_string(),
			login: "u".to_string(),
			display: Some("u".to_string()),
		},
		text: "hi".to_string(),
		badges: Vec::new(),
	});

	assert!(should_emit_payload(false, &room_state));
	assert!(should_emit_payload(false, &notice));
	assert!(should_emit_payload(false, &chat));
}

#[test]
fn mod_gating_blocks_non_delete_moderation_when_not_mod() {
	let ban = IngestPayload::Moderation(Box::new(crate::ModerationEvent {
		kind: "ban".to_string(),
		actor: None,
		target: Some(crate::UserRef {
			id: "target".to_string(),
			login: "target".to_string(),
			display: Some("Target".to_string()),
		}),
		target_message_platform_id: None,
		notes: None,
		action: Some(ModerationAction::Ban {
			is_permanent: Some(true),
			reason: None,
		}),
	}));

	assert!(!should_emit_payload(false, &ban));
}

#[test]
fn mod_gating_allows_delete_moderation_when_not_mod() {
	let delete = IngestPayload::Moderation(Box::new(crate::ModerationEvent {
		kind: "delete".to_string(),
		actor: None,
		target: Some(crate::UserRef {
			id: "target".to_string(),
			login: "target".to_string(),
			display: Some("Target".to_string()),
		}),
		target_message_platform_id: Some("msg-1".to_string()),
		notes: None,
		action: Some(ModerationAction::DeleteMessage {
			message_id: "msg-1".to_string(),
		}),
	}));

	assert!(should_emit_payload(false, &delete));
}

#[test]
fn mod_gating_allows_all_moderation_when_mod() {
	let timeout = IngestPayload::Moderation(Box::new(crate::ModerationEvent {
		kind: "timeout".to_string(),
		actor: None,
		target: Some(crate::UserRef {
			id: "target".to_string(),
			login: "target".to_string(),
			display: Some("Target".to_string()),
		}),
		target_message_platform_id: None,
		notes: None,
		action: Some(ModerationAction::Timeout {
			duration_seconds: Some(600),
			expires_at: None,
			reason: Some("reason".to_string()),
		}),
	}));

	assert!(should_emit_payload(true, &timeout));
}

#[test]
fn channel_moderate_timeout_decodes_to_moderation_action_timeout() {
	let now = SystemTime::now();
	let expires_at = now + Duration::from_secs(600);
	let expires_at_rfc3339: String = chrono::DateTime::<chrono::Utc>::from(expires_at).to_rfc3339();

	let m = mk_moderate(
		"somechannel",
		"timeout",
		Some(serde_json::json!({
			"user_id": "u1",
			"user_login": "user1",
			"user_name": "User One",
			"reason": "spamming",
			"expires_at": expires_at_rfc3339,
		})),
	);

	let (maybe_mod, maybe_state) = decode_channel_moderate_to_ingest(&m, now, "sess-1");
	assert!(maybe_state.is_none(), "timeout should not produce room state");

	let Some(crate::AdapterEvent::Ingest(ing)) = maybe_mod else {
		panic!("expected a moderation ingest event");
	};

	let IngestPayload::Moderation(mev) = &ing.payload else {
		panic!("expected moderation payload");
	};

	assert_eq!(mev.kind, "timeout");
	assert!(mev.action.is_some());

	match &mev.action {
		Some(ModerationAction::Timeout {
			duration_seconds,
			expires_at: decoded_expires_at,
			reason,
		}) => {
			assert!(duration_seconds.is_some());
			assert_eq!(reason.as_deref(), Some("spamming"));
			assert!(decoded_expires_at.is_some());
		}
		other => panic!("expected ModerationAction::Timeout, got {other:?}"),
	}
}

#[test]
fn channel_moderate_delete_decodes_to_delete_message_action_and_notes() {
	let now = SystemTime::now();

	let m = mk_moderate(
		"somechannel",
		"delete",
		Some(serde_json::json!({
			"user_id": "u1",
			"user_login": "user1",
			"user_name": "User One",
			"message_id": "mid-123",
			"message_body": "hello world",
		})),
	);

	let (maybe_mod, maybe_state) = decode_channel_moderate_to_ingest(&m, now, "sess-1");
	assert!(maybe_state.is_none(), "delete should not produce room state");

	let Some(crate::AdapterEvent::Ingest(ing)) = maybe_mod else {
		panic!("expected moderation ingest event");
	};

	let IngestPayload::Moderation(mev) = &ing.payload else {
		panic!("expected moderation payload");
	};

	assert_eq!(mev.kind, "delete");
	assert_eq!(mev.target_message_platform_id.as_deref(), Some("mid-123"));
	assert!(mev.notes.as_deref().unwrap_or("").contains("message_body="));

	match &mev.action {
		Some(ModerationAction::DeleteMessage { message_id }) => {
			assert_eq!(message_id, "mid-123");
		}
		other => panic!("expected ModerationAction::DeleteMessage, got {other:?}"),
	}
}

#[test]
fn channel_moderate_slow_decodes_to_room_state_toggle() {
	let now = SystemTime::now();

	let m = mk_moderate(
		"somechannel",
		"slow",
		Some(serde_json::json!({
			"wait_time_seconds": 10
		})),
	);

	let (maybe_mod, maybe_state) = decode_channel_moderate_to_ingest(&m, now, "sess-1");
	assert!(maybe_mod.is_none(), "slow should not produce a moderation payload");

	let Some(crate::AdapterEvent::Ingest(ing)) = maybe_state else {
		panic!("expected room-state ingest event");
	};

	let IngestPayload::RoomState(rs) = &ing.payload else {
		panic!("expected room-state payload");
	};

	assert_eq!(rs.settings.slow_mode, Some(true));
	assert_eq!(rs.settings.slow_mode_wait_time_seconds, Some(10));
}

#[test]
fn channel_moderate_unknown_action_falls_back_to_moderation_event_with_notes() {
	let now = SystemTime::now();

	let m = mk_moderate(
		"somechannel",
		"some_new_action",
		Some(serde_json::json!({
			"foo": "bar",
			"n": 1
		})),
	);

	let (maybe_mod, maybe_state) = decode_channel_moderate_to_ingest(&m, now, "sess-1");
	assert!(maybe_state.is_none(), "unknown action should not produce room state");

	let Some(crate::AdapterEvent::Ingest(ing)) = maybe_mod else {
		panic!("expected moderation ingest event");
	};

	let IngestPayload::Moderation(mev) = &ing.payload else {
		panic!("expected moderation payload");
	};

	assert_eq!(mev.kind, "some_new_action");
	assert!(mev.action.is_none());
	assert!(mev.notes.as_deref().unwrap_or("").contains("\"foo\""));
}
