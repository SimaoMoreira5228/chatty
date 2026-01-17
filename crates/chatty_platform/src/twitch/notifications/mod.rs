#![forbid(unsafe_code)]

mod handlers;
mod moderation;
mod room_state;

#[cfg(test)]
mod tests;

pub(crate) use handlers::handle_notification_json;
pub(crate) use moderation::decode_channel_moderate_to_ingest;
pub(crate) use room_state::{mk_room_state_ingest, mk_user_notice_ingest};

/// Apply moderation gating policy.
pub(crate) fn should_emit_payload(token_user_is_mod: bool, payload: &crate::IngestPayload) -> bool {
	use crate::{IngestPayload, ModerationAction};

	match payload {
		IngestPayload::ChatMessage(_) => true,
		IngestPayload::AssetBundle(_) => true,
		IngestPayload::UserNotice(_) => true,
		IngestPayload::RoomState(_) => true,
		IngestPayload::Moderation(m) => {
			if token_user_is_mod {
				return true;
			}

			match &m.action {
				Some(ModerationAction::DeleteMessage { .. }) => true,
				None if m.kind == "delete" => true,
				_ => false,
			}
		}
	}
}
