#![forbid(unsafe_code)]

use std::time::SystemTime;

use chatty_domain::Platform;

use crate::{AdapterEvent, IngestEvent, IngestPayload, RoomChatSettings, RoomState, UserNotice};

/// Create a RoomState ingest event.
pub(crate) fn mk_room_state_ingest(
	room: chatty_domain::RoomKey,
	ingest_now: SystemTime,
	platform_time: Option<SystemTime>,
	actor: Option<crate::UserRef>,
	settings: RoomChatSettings,
	notes: Option<String>,
	adapter_session_id: &str,
	ws_message_id: Option<String>,
	subscription_id: Option<String>,
) -> AdapterEvent {
	let mut ingest = IngestEvent::new(
		Platform::Twitch,
		room.room_id.clone(),
		IngestPayload::RoomState(RoomState {
			flags: Default::default(),
			settings,
			actor,
			notes,
		}),
	);

	ingest.room = room;
	ingest.ingest_time = ingest_now;
	ingest.platform_time = platform_time;

	let mut trace = crate::IngestTrace {
		session_id: Some(adapter_session_id.to_string()),
		..crate::IngestTrace::default()
	};

	if let Some(ws_message_id) = ws_message_id {
		trace.fields.insert("twitch_ws_message_id".to_string(), ws_message_id);
	}
	if let Some(subscription_id) = subscription_id {
		trace.fields.insert("twitch_subscription_id".to_string(), subscription_id);
	}

	ingest.trace = trace;

	AdapterEvent::Ingest(Box::new(ingest))
}

/// Create a UserNotice ingest event.
pub(crate) fn mk_user_notice_ingest(
	room: chatty_domain::RoomKey,
	ingest_now: SystemTime,
	platform_time: Option<SystemTime>,
	kind: impl Into<String>,
	text: Option<String>,
	user: Option<crate::UserRef>,
	adapter_session_id: &str,
	ws_message_id: Option<String>,
	subscription_id: Option<String>,
) -> AdapterEvent {
	let mut ingest = IngestEvent::new(
		Platform::Twitch,
		room.room_id.clone(),
		IngestPayload::UserNotice(UserNotice {
			kind: kind.into(),
			text,
			user,
		}),
	);

	ingest.room = room;
	ingest.ingest_time = ingest_now;
	ingest.platform_time = platform_time;

	let mut trace = crate::IngestTrace {
		session_id: Some(adapter_session_id.to_string()),
		..crate::IngestTrace::default()
	};

	if let Some(ws_message_id) = ws_message_id {
		trace.fields.insert("twitch_ws_message_id".to_string(), ws_message_id);
	}
	if let Some(subscription_id) = subscription_id {
		trace.fields.insert("twitch_subscription_id".to_string(), subscription_id);
	}

	ingest.trace = trace;

	AdapterEvent::Ingest(Box::new(ingest))
}
