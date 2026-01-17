#![forbid(unsafe_code)]

use std::time::SystemTime;

use anyhow::Context;
use chatty_domain::Platform;

use crate::twitch::eventsub;
use crate::{AdapterEvent, IngestEvent, IngestPayload};

/// Handle a raw EventSub WS notification message.
pub(crate) fn handle_notification_json(
	raw_json: &str,
	adapter_session_id: &str,
	ingest_now: SystemTime,
) -> anyhow::Result<(Option<chatty_domain::RoomKey>, Vec<AdapterEvent>)> {
	let mut out = Vec::new();
	let ty = eventsub::peek_message_type(raw_json).unwrap_or_default();
	if ty != "notification" {
		return Ok((None, out));
	}

	let sub_ty = eventsub::peek_subscription_type(raw_json).unwrap_or(None);

	match sub_ty.as_deref() {
		Some("channel.chat.message") => {
			let Some(n) =
				eventsub::try_normalize_channel_chat_message(raw_json).context("normalize channel.chat.message")?
			else {
				return Ok((None, out));
			};

			let room_for_gating = n.room.clone();

			let mut ingest = IngestEvent::new(
				Platform::Twitch,
				n.room.room_id.clone(),
				IngestPayload::ChatMessage(crate::ChatMessage {
					ids: crate::IngestMessageIds {
						server_id: uuid::Uuid::new_v4(),
						platform_id: Some(n.platform_message_id.clone().into_string()),
					},
					author: crate::UserRef {
						id: n.chatter_user_id,
						login: n.chatter_user_login,
						display: Some(n.chatter_user_name),
					},
					text: n.text,
					badges: n.badge_ids,
				}),
			);

			ingest.room = n.room;
			ingest.ingest_time = ingest_now;
			ingest.platform_time = Some(n.platform_time);
			ingest.platform_message_id = Some(n.platform_message_id);

			let mut trace = crate::IngestTrace {
				session_id: Some(adapter_session_id.to_string()),
				..crate::IngestTrace::default()
			};
			trace.fields.insert("twitch_ws_message_id".to_string(), n.ws_message_id);
			trace.fields.insert("twitch_subscription_id".to_string(), n.subscription_id);
			ingest.trace = trace;

			out.push(AdapterEvent::Ingest(Box::new(ingest)));
			Ok((Some(room_for_gating), out))
		}

		Some("channel.chat.message_delete") => {
			let Some(d) = eventsub::try_normalize_channel_chat_message_delete(raw_json)
				.context("normalize channel.chat.message_delete")?
			else {
				return Ok((None, out));
			};

			let room_for_gating = d.room.clone();

			let target_message_platform_id: chatty_domain::PlatformMessageId = d.target_message_platform_id.clone();

			let actor = None;

			let target = Some(crate::UserRef {
				id: d.target_user_id.clone(),
				login: d.target_user_login,
				display: Some(d.target_user_name),
			});

			let mut ingest = IngestEvent::new(
				Platform::Twitch,
				d.room.room_id.clone(),
				IngestPayload::Moderation(Box::new(crate::ModerationEvent {
					kind: "delete".to_string(),
					actor,
					target,
					target_message_platform_id: Some(target_message_platform_id.clone().into_string()),
					notes: None,
					action: Some(crate::ModerationAction::DeleteMessage {
						message_id: target_message_platform_id.clone().into_string(),
					}),
				})),
			);

			ingest.room = d.room;
			ingest.ingest_time = ingest_now;
			ingest.platform_time = Some(d.platform_time);
			ingest.platform_message_id = Some(target_message_platform_id);

			let mut trace = crate::IngestTrace {
				session_id: Some(adapter_session_id.to_string()),
				..crate::IngestTrace::default()
			};
			trace.fields.insert("twitch_ws_message_id".to_string(), d.ws_message_id);
			trace.fields.insert("twitch_subscription_id".to_string(), d.subscription_id);
			ingest.trace = trace;

			out.push(AdapterEvent::Ingest(Box::new(ingest)));
			Ok((Some(room_for_gating), out))
		}

		Some("channel.ban") => {
			let Some(b) = eventsub::try_normalize_channel_ban(raw_json).context("normalize channel.ban")? else {
				return Ok((None, out));
			};

			let room_for_gating = b.room.clone();

			let actor = Some(crate::UserRef {
				id: b.moderator_user_id,
				login: b.moderator_user_login,
				display: Some(b.moderator_user_name),
			});

			let target = Some(crate::UserRef {
				id: b.target_user_id.clone(),
				login: b.target_user_login,
				display: Some(b.target_user_name),
			});

			let action = if b.is_permanent {
				Some(crate::ModerationAction::Ban {
					is_permanent: Some(true),
					reason: b.reason.clone(),
				})
			} else {
				let duration_seconds = b
					.ends_at
					.and_then(|ends_at| ends_at.duration_since(b.platform_time).ok())
					.map(|d| d.as_secs());
				Some(crate::ModerationAction::Timeout {
					duration_seconds,
					expires_at: b.ends_at,
					reason: b.reason.clone(),
				})
			};

			let kind = if b.is_permanent { "ban" } else { "timeout" }.to_string();

			let mut ingest = IngestEvent::new(
				Platform::Twitch,
				b.room.room_id.clone(),
				IngestPayload::Moderation(Box::new(crate::ModerationEvent {
					kind,
					actor,
					target,
					target_message_platform_id: None,
					notes: None,
					action,
				})),
			);

			ingest.room = b.room;
			ingest.ingest_time = ingest_now;
			ingest.platform_time = Some(b.platform_time);

			let mut trace = crate::IngestTrace {
				session_id: Some(adapter_session_id.to_string()),
				..crate::IngestTrace::default()
			};
			trace.fields.insert("twitch_ws_message_id".to_string(), b.ws_message_id);
			trace.fields.insert("twitch_subscription_id".to_string(), b.subscription_id);
			ingest.trace = trace;

			out.push(AdapterEvent::Ingest(Box::new(ingest)));
			Ok((Some(room_for_gating), out))
		}

		Some("channel.moderate") => {
			let Some(m) = eventsub::try_normalize_channel_moderate(raw_json).context("normalize channel.moderate")? else {
				return Ok((None, out));
			};

			let room_for_gating = m.room.clone();

			let (maybe_ingest, maybe_state_ingest) =
				super::decode_channel_moderate_to_ingest(&m, ingest_now, adapter_session_id);

			if let Some(ev) = maybe_ingest {
				out.push(ev);
			}
			if let Some(ev) = maybe_state_ingest {
				out.push(ev);
			}

			Ok((Some(room_for_gating), out))
		}

		Some("channel.raid") => {
			let Some(r) = eventsub::try_normalize_channel_raid(raw_json).context("normalize channel.raid")? else {
				return Ok((None, out));
			};

			let room_for_gating = r.room.clone();

			let text = Some(format!(
				"Raid: {} (login={}) -> viewers={}",
				r.from_broadcaster_user_name, r.from_broadcaster_user_login, r.viewers
			));

			let user = Some(crate::UserRef {
				id: r.from_broadcaster_user_id,
				login: r.from_broadcaster_user_login,
				display: Some(r.from_broadcaster_user_name),
			});

			out.push(super::mk_user_notice_ingest(
				r.room,
				ingest_now,
				Some(r.platform_time),
				"raid",
				text,
				user,
				adapter_session_id,
				Some(r.ws_message_id),
				Some(r.subscription_id),
			));

			Ok((Some(room_for_gating), out))
		}

		Some("channel.cheer") => {
			let Some(c) = eventsub::try_normalize_channel_cheer(raw_json).context("normalize channel.cheer")? else {
				return Ok((None, out));
			};

			let room_for_gating = c.room.clone();

			let text = match (&c.message, c.is_anonymous) {
				(Some(msg), true) => Some(format!("Anonymous cheer: {} bits: {}", c.bits, msg)),
				(Some(msg), false) => Some(format!("Cheer: {} bits: {}", c.bits, msg)),
				(None, true) => Some(format!("Anonymous cheer: {} bits", c.bits)),
				(None, false) => Some(format!("Cheer: {} bits", c.bits)),
			};

			let user = match (c.user_id, c.user_login, c.user_name, c.is_anonymous) {
				(_, _, _, true) => None,
				(Some(id), Some(login), Some(name), false) => Some(crate::UserRef {
					id,
					login,
					display: Some(name),
				}),
				_ => None,
			};

			out.push(super::mk_user_notice_ingest(
				c.room,
				ingest_now,
				Some(c.platform_time),
				"cheer",
				text,
				user,
				adapter_session_id,
				Some(c.ws_message_id),
				Some(c.subscription_id),
			));

			Ok((Some(room_for_gating), out))
		}

		Some("channel.subscribe") => {
			let Some(s) = eventsub::try_normalize_channel_subscribe(raw_json).context("normalize channel.subscribe")? else {
				return Ok((None, out));
			};

			let room_for_gating = s.room.clone();

			let text = Some(format!(
				"Subscribe: tier={} gift={}",
				s.tier,
				if s.is_gift { "true" } else { "false" }
			));

			let user = Some(crate::UserRef {
				id: s.user_id,
				login: s.user_login,
				display: Some(s.user_name),
			});

			out.push(super::mk_user_notice_ingest(
				s.room,
				ingest_now,
				Some(s.platform_time),
				"subscribe",
				text,
				user,
				adapter_session_id,
				Some(s.ws_message_id),
				Some(s.subscription_id),
			));

			Ok((Some(room_for_gating), out))
		}

		_ => Ok((None, out)),
	}
}
