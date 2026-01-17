#![forbid(unsafe_code)]

use std::time::SystemTime;

use chatty_domain::RoomKey;

use crate::twitch::eventsub;
use crate::{AdapterEvent, IngestEvent, IngestPayload, ModerationAction, ModerationEvent, RoomChatSettings};

/// Decode a normalized `channel.moderate` event into ingest events.
pub(crate) fn decode_channel_moderate_to_ingest(
	m: &eventsub::NormalizedChannelModerateNotification,
	ingest_now: SystemTime,
	adapter_session_id: &str,
) -> (Option<AdapterEvent>, Option<AdapterEvent>) {
	let actor = Some(crate::UserRef {
		id: m.moderator_user_id.clone(),
		login: m.moderator_user_login.clone(),
		display: Some(m.moderator_user_name.clone()),
	});

	let ws_id = Some(m.ws_message_id.clone());
	let sub_id = Some(m.subscription_id.clone());

	#[allow(clippy::too_many_arguments)]
	fn mk_mod(
		room: RoomKey,
		platform_time: SystemTime,
		ingest_now: SystemTime,
		adapter_session_id: &str,
		ws_message_id: String,
		subscription_id: String,
		actor: Option<crate::UserRef>,
		kind: &str,
		target: Option<crate::UserRef>,
		target_message_platform_id: Option<String>,
		action: Option<ModerationAction>,
		notes: Option<String>,
	) -> AdapterEvent {
		let mut ingest = IngestEvent::new(
			chatty_domain::Platform::Twitch,
			room.room_id.clone(),
			IngestPayload::Moderation(Box::new(ModerationEvent {
				kind: kind.to_string(),
				actor,
				target,
				target_message_platform_id,
				notes,
				action,
			})),
		);

		ingest.room = room;
		ingest.ingest_time = ingest_now;
		ingest.platform_time = Some(platform_time);

		let mut trace = crate::IngestTrace {
			session_id: Some(adapter_session_id.to_string()),
			..crate::IngestTrace::default()
		};
		trace.fields.insert("twitch_ws_message_id".to_string(), ws_message_id);
		trace.fields.insert("twitch_subscription_id".to_string(), subscription_id);
		ingest.trace = trace;

		AdapterEvent::Ingest(Box::new(ingest))
	}

	let field_str =
		|v: &serde_json::Value, k: &str| -> Option<String> { v.get(k).and_then(|x| x.as_str()).map(|s| s.to_string()) };

	let ad = m.action_data.as_ref();

	match m.action.as_str() {
		"timeout" | "shared_chat_timeout" => {
			if let Some(ad) = ad {
				let user = Some(crate::UserRef {
					id: field_str(ad, "user_id").unwrap_or_default(),
					login: field_str(ad, "user_login").unwrap_or_default(),
					display: field_str(ad, "user_name"),
				});
				let reason = field_str(ad, "reason");
				let expires_at =
					field_str(ad, "expires_at").and_then(|ts| eventsub::parse_message_timestamp_system_time(&ts).ok());
				let duration_seconds = expires_at
					.and_then(|e| e.duration_since(m.platform_time).ok())
					.map(|d| d.as_secs());

				let ev = mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"timeout",
					user.clone(),
					None,
					Some(ModerationAction::Timeout {
						duration_seconds,
						expires_at,
						reason,
					}),
					None,
				);

				return (Some(ev), None);
			}
			let ev = mk_mod(
				m.room.clone(),
				m.platform_time,
				ingest_now,
				adapter_session_id,
				m.ws_message_id.clone(),
				m.subscription_id.clone(),
				actor.clone(),
				"timeout",
				None,
				None,
				None,
				Some("missing action_data".to_string()),
			);
			(Some(ev), None)
		}

		"untimeout" | "shared_chat_untimeout" => {
			if let Some(ad) = ad {
				let user = Some(crate::UserRef {
					id: field_str(ad, "user_id").unwrap_or_default(),
					login: field_str(ad, "user_login").unwrap_or_default(),
					display: field_str(ad, "user_name"),
				});
				let ev = mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"untimeout",
					user,
					None,
					Some(ModerationAction::Untimeout {}),
					None,
				);
				return (Some(ev), None);
			}
			(
				Some(mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"untimeout",
					None,
					None,
					Some(ModerationAction::Untimeout {}),
					Some("missing action_data".to_string()),
				)),
				None,
			)
		}

		"ban" | "shared_chat_ban" => {
			if let Some(ad) = ad {
				let user = Some(crate::UserRef {
					id: field_str(ad, "user_id").unwrap_or_default(),
					login: field_str(ad, "user_login").unwrap_or_default(),
					display: field_str(ad, "user_name"),
				});
				let reason = field_str(ad, "reason");
				let ev = mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"ban",
					user,
					None,
					Some(ModerationAction::Ban {
						is_permanent: Some(true),
						reason,
					}),
					None,
				);
				return (Some(ev), None);
			}
			(
				Some(mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"ban",
					None,
					None,
					Some(ModerationAction::Ban {
						is_permanent: Some(true),
						reason: None,
					}),
					Some("missing action_data".to_string()),
				)),
				None,
			)
		}

		"unban" | "shared_chat_unban" => {
			if let Some(ad) = ad {
				let user = Some(crate::UserRef {
					id: field_str(ad, "user_id").unwrap_or_default(),
					login: field_str(ad, "user_login").unwrap_or_default(),
					display: field_str(ad, "user_name"),
				});
				let ev = mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"unban",
					user,
					None,
					Some(ModerationAction::Unban {}),
					None,
				);
				return (Some(ev), None);
			}
			(
				Some(mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"unban",
					None,
					None,
					Some(ModerationAction::Unban {}),
					Some("missing action_data".to_string()),
				)),
				None,
			)
		}

		"delete" | "shared_chat_delete" => {
			if let Some(ad) = ad {
				let user = Some(crate::UserRef {
					id: field_str(ad, "user_id").unwrap_or_default(),
					login: field_str(ad, "user_login").unwrap_or_default(),
					display: field_str(ad, "user_name"),
				});
				let msg_id = field_str(ad, "message_id").unwrap_or_default();
				let message_body = field_str(ad, "message_body");

				let ev = mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"delete",
					user,
					Some(msg_id.clone()),
					Some(ModerationAction::DeleteMessage { message_id: msg_id }),
					message_body.map(|s| format!("message_body={s}")),
				);
				return (Some(ev), None);
			}
			(
				Some(mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"delete",
					None,
					None,
					None,
					Some("missing action_data".to_string()),
				)),
				None,
			)
		}

		"clear" => {
			let ev = mk_mod(
				m.room.clone(),
				m.platform_time,
				ingest_now,
				adapter_session_id,
				m.ws_message_id.clone(),
				m.subscription_id.clone(),
				actor.clone(),
				"clear_chat",
				None,
				None,
				Some(ModerationAction::ClearChat {}),
				None,
			);
			(Some(ev), None)
		}

		"slow" => {
			let mut settings = RoomChatSettings {
				slow_mode: Some(true),
				..Default::default()
			};
			if let Some(ad) = ad {
				settings.slow_mode_wait_time_seconds = ad.get("wait_time_seconds").and_then(|v| v.as_u64());
			}
			let ev = super::mk_room_state_ingest(
				m.room.clone(),
				ingest_now,
				Some(m.platform_time),
				actor,
				settings,
				Some("slow mode enabled".to_string()),
				adapter_session_id,
				ws_id,
				sub_id,
			);
			(None, Some(ev))
		}
		"slowoff" => {
			let settings = RoomChatSettings {
				slow_mode: Some(false),
				..Default::default()
			};
			let ev = super::mk_room_state_ingest(
				m.room.clone(),
				ingest_now,
				Some(m.platform_time),
				actor,
				settings,
				Some("slow mode disabled".to_string()),
				adapter_session_id,
				ws_id,
				sub_id,
			);
			(None, Some(ev))
		}

		"followers" => {
			let mut settings = RoomChatSettings {
				followers_only: Some(true),
				..Default::default()
			};
			if let Some(ad) = ad
				&& let Some(f) = ad.get("follow_duration_minutes").and_then(|v| v.as_u64())
			{
				settings.followers_only_duration_minutes = Some(f);
			}
			let ev = super::mk_room_state_ingest(
				m.room.clone(),
				ingest_now,
				Some(m.platform_time),
				actor,
				settings,
				Some("followers-only enabled".to_string()),
				adapter_session_id,
				ws_id,
				sub_id,
			);
			(None, Some(ev))
		}

		"followersoff" => {
			let settings = RoomChatSettings {
				followers_only: Some(false),
				..Default::default()
			};
			let ev = super::mk_room_state_ingest(
				m.room.clone(),
				ingest_now,
				Some(m.platform_time),
				actor,
				settings,
				Some("followers-only disabled".to_string()),
				adapter_session_id,
				ws_id,
				sub_id,
			);
			(None, Some(ev))
		}

		"emoteonly" => {
			let settings = RoomChatSettings {
				emote_only: Some(true),
				..Default::default()
			};
			let ev = super::mk_room_state_ingest(
				m.room.clone(),
				ingest_now,
				Some(m.platform_time),
				actor,
				settings,
				Some("emote-only enabled".to_string()),
				adapter_session_id,
				ws_id,
				sub_id,
			);
			(None, Some(ev))
		}

		"emoteonlyoff" => {
			let settings = RoomChatSettings {
				emote_only: Some(false),
				..Default::default()
			};
			let ev = super::mk_room_state_ingest(
				m.room.clone(),
				ingest_now,
				Some(m.platform_time),
				actor,
				settings,
				Some("emote-only disabled".to_string()),
				adapter_session_id,
				ws_id,
				sub_id,
			);
			(None, Some(ev))
		}

		"subscribers" => {
			let settings = RoomChatSettings {
				subscribers_only: Some(true),
				..Default::default()
			};
			let ev = super::mk_room_state_ingest(
				m.room.clone(),
				ingest_now,
				Some(m.platform_time),
				actor,
				settings,
				Some("subscribers-only enabled".to_string()),
				adapter_session_id,
				ws_id,
				sub_id,
			);
			(None, Some(ev))
		}

		"subscribersoff" => {
			let settings = RoomChatSettings {
				subscribers_only: Some(false),
				..Default::default()
			};
			let ev = super::mk_room_state_ingest(
				m.room.clone(),
				ingest_now,
				Some(m.platform_time),
				actor,
				settings,
				Some("subscribers-only disabled".to_string()),
				adapter_session_id,
				ws_id,
				sub_id,
			);
			(None, Some(ev))
		}

		"uniquechat" => {
			let settings = RoomChatSettings {
				unique_chat: Some(true),
				..Default::default()
			};
			let ev = super::mk_room_state_ingest(
				m.room.clone(),
				ingest_now,
				Some(m.platform_time),
				actor,
				settings,
				Some("unique chat enabled".to_string()),
				adapter_session_id,
				ws_id,
				sub_id,
			);
			(None, Some(ev))
		}

		"uniquechatoff" => {
			let settings = RoomChatSettings {
				unique_chat: Some(false),
				..Default::default()
			};
			let ev = super::mk_room_state_ingest(
				m.room.clone(),
				ingest_now,
				Some(m.platform_time),
				actor,
				settings,
				Some("unique chat disabled".to_string()),
				adapter_session_id,
				ws_id,
				sub_id,
			);
			(None, Some(ev))
		}

		"vip" => {
			if let Some(ad) = ad {
				let user = crate::UserRef {
					id: field_str(ad, "user_id").unwrap_or_default(),
					login: field_str(ad, "user_login").unwrap_or_default(),
					display: field_str(ad, "user_name"),
				};
				let ev = mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"vip_add",
					Some(user.clone()),
					None,
					Some(ModerationAction::VipAdd { user }),
					None,
				);
				return (Some(ev), None);
			}
			(
				Some(mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"vip_add",
					None,
					None,
					None,
					Some("missing action_data".to_string()),
				)),
				None,
			)
		}

		"unvip" => {
			if let Some(ad) = ad {
				let user = crate::UserRef {
					id: field_str(ad, "user_id").unwrap_or_default(),
					login: field_str(ad, "user_login").unwrap_or_default(),
					display: field_str(ad, "user_name"),
				};
				let ev = mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"vip_remove",
					Some(user.clone()),
					None,
					Some(ModerationAction::VipRemove { user }),
					None,
				);
				return (Some(ev), None);
			}
			(
				Some(mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"vip_remove",
					None,
					None,
					None,
					Some("missing action_data".to_string()),
				)),
				None,
			)
		}

		"mod" => {
			if let Some(ad) = ad {
				let user = crate::UserRef {
					id: field_str(ad, "user_id").unwrap_or_default(),
					login: field_str(ad, "user_login").unwrap_or_default(),
					display: field_str(ad, "user_name"),
				};
				let ev = mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"mod_add",
					Some(user.clone()),
					None,
					Some(ModerationAction::ModeratorAdd { user }),
					None,
				);
				return (Some(ev), None);
			}
			(
				Some(mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"mod_add",
					None,
					None,
					None,
					Some("missing action_data".to_string()),
				)),
				None,
			)
		}

		"unmod" => {
			if let Some(ad) = ad {
				let user = crate::UserRef {
					id: field_str(ad, "user_id").unwrap_or_default(),
					login: field_str(ad, "user_login").unwrap_or_default(),
					display: field_str(ad, "user_name"),
				};
				let ev = mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"mod_remove",
					Some(user.clone()),
					None,
					Some(ModerationAction::ModeratorRemove { user }),
					None,
				);
				return (Some(ev), None);
			}
			(
				Some(mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"mod_remove",
					None,
					None,
					None,
					Some("missing action_data".to_string()),
				)),
				None,
			)
		}

		"add_blocked_term" | "add_permitted_term" | "remove_blocked_term" | "remove_permitted_term" => {
			if let Some(ad) = ad {
				let terms_obj = ad.get("automod_terms").unwrap_or(ad);
				let action = field_str(terms_obj, "action").or_else(|| Some(m.action.clone()));
				let terms = terms_obj.get("terms").and_then(|t| t.as_array()).map(|arr| {
					arr.iter()
						.filter_map(|x| x.as_str().map(|s| s.to_string()))
						.collect::<Vec<_>>()
				});
				let ev = mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"automod_terms_update",
					None,
					None,
					Some(ModerationAction::AutoModTermsUpdate { action, terms }),
					None,
				);
				return (Some(ev), None);
			}
			(
				Some(mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"automod_terms_update",
					None,
					None,
					None,
					Some("missing action_data".to_string()),
				)),
				None,
			)
		}

		"approve_unban_request" | "deny_unban_request" => {
			if let Some(ad) = ad {
				let is_approved = ad.get("is_approved").and_then(|v| v.as_bool());
				let user = crate::UserRef {
					id: field_str(ad, "user_id").unwrap_or_default(),
					login: field_str(ad, "user_login").unwrap_or_default(),
					display: field_str(ad, "user_name"),
				};
				let moderator_message = field_str(ad, "moderator_message");
				let resolution = match (m.action.as_str(), is_approved) {
					("approve_unban_request", _) => Some("approved".to_string()),
					("deny_unban_request", _) => Some("denied".to_string()),
					(_, Some(true)) => Some("approved".to_string()),
					(_, Some(false)) => Some("denied".to_string()),
					_ => None,
				};

				let ev = mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"unban_request_resolve",
					Some(user.clone()),
					None,
					Some(ModerationAction::UnbanRequestResolve {
						request_id: None,
						user,
						resolution,
						resolved_by: actor.clone(),
						resolved_at: Some(m.platform_time),
					}),
					moderator_message.map(|s| format!("moderator_message={s}")),
				);
				return (Some(ev), None);
			}
			(
				Some(mk_mod(
					m.room.clone(),
					m.platform_time,
					ingest_now,
					adapter_session_id,
					m.ws_message_id.clone(),
					m.subscription_id.clone(),
					actor.clone(),
					"unban_request_resolve",
					None,
					None,
					None,
					Some("missing action_data".to_string()),
				)),
				None,
			)
		}

		"raid" | "unraid" => {
			let notes = ad.map(|ad| ad.to_string());
			let ev = mk_mod(
				m.room.clone(),
				m.platform_time,
				ingest_now,
				adapter_session_id,
				m.ws_message_id.clone(),
				m.subscription_id.clone(),
				actor.clone(),
				"raid_control",
				None,
				None,
				None,
				notes,
			);
			(Some(ev), None)
		}

		other => {
			let notes = m.action_data.as_ref().map(|v| v.to_string());
			let ev = mk_mod(
				m.room.clone(),
				m.platform_time,
				ingest_now,
				adapter_session_id,
				m.ws_message_id.clone(),
				m.subscription_id.clone(),
				actor.clone(),
				other,
				None,
				None,
				None,
				notes,
			);
			(Some(ev), None)
		}
	}
}
