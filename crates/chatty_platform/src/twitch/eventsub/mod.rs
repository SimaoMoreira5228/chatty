#![forbid(unsafe_code)]

use std::time::SystemTime;

use anyhow::Context;
use chatty_domain::{Platform, PlatformMessageId, RoomId, RoomKey};
use serde::Deserialize;

/// EventSub metadata (present on all WebSocket messages).
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub(crate) struct EventSubMetadata {
	pub(crate) message_id: String,
	pub(crate) message_type: String,
	pub(crate) message_timestamp: String,

	#[serde(default)]
	pub(crate) subscription_type: Option<String>,
	#[serde(default)]
	pub(crate) subscription_version: Option<String>,
}

/// A lightweight peek struct to cheaply inspect message_type/subscription_type.
#[derive(Debug, Deserialize)]
pub(crate) struct EventSubMetadataPeek {
	pub(crate) metadata: EventSubMetadataPeekInner,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventSubMetadataPeekInner {
	pub(crate) message_type: String,
	#[serde(default)]
	pub(crate) subscription_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventSubWelcomeMessage {
	#[allow(dead_code)]
	pub(crate) metadata: EventSubMetadata,
	pub(crate) payload: EventSubWelcomePayload,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventSubWelcomePayload {
	pub(crate) session: EventSubWelcomeSession,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventSubWelcomeSession {
	pub(crate) id: String,

	#[allow(dead_code)]
	pub(crate) status: String,
	#[allow(dead_code)]
	pub(crate) connected_at: String,

	#[serde(default)]
	pub(crate) keepalive_timeout_seconds: Option<u64>,

	#[allow(dead_code)]
	#[serde(default)]
	pub(crate) reconnect_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventSubReconnectMessage {
	#[allow(dead_code)]
	pub(crate) metadata: EventSubMetadata,
	pub(crate) payload: EventSubReconnectPayload,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventSubReconnectPayload {
	pub(crate) session: EventSubReconnectSession,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventSubReconnectSession {
	#[allow(dead_code)]
	pub(crate) id: String,
	#[allow(dead_code)]
	pub(crate) status: String,

	#[serde(default)]
	#[allow(dead_code)]
	pub(crate) keepalive_timeout_seconds: Option<u64>,

	pub(crate) reconnect_url: String,

	#[allow(dead_code)]
	pub(crate) connected_at: String,

	#[serde(default)]
	#[allow(dead_code)]
	pub(crate) recovery_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventSubNotification<TEvent> {
	pub(crate) metadata: EventSubMetadata,
	pub(crate) payload: EventSubNotificationPayload<TEvent>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventSubNotificationPayload<TEvent> {
	pub(crate) subscription: EventSubSubscription,
	pub(crate) event: TEvent,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventSubSubscription {
	pub(crate) id: String,

	#[allow(dead_code)]
	pub(crate) status: String,
	#[serde(rename = "type")]
	#[allow(dead_code)]
	pub(crate) r#type: String,
	#[allow(dead_code)]
	pub(crate) version: String,
	#[allow(dead_code)]
	pub(crate) condition: serde_json::Value,
	#[allow(dead_code)]
	pub(crate) created_at: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChannelChatMessageEvent {
	#[allow(dead_code)]
	pub(crate) broadcaster_user_id: String,
	pub(crate) broadcaster_user_login: String,
	#[allow(dead_code)]
	pub(crate) broadcaster_user_name: String,

	pub(crate) chatter_user_id: String,
	pub(crate) chatter_user_login: String,
	pub(crate) chatter_user_name: String,

	pub(crate) message_id: String,
	pub(crate) message: ChannelChatMessageContent,
	#[serde(default)]
	pub(crate) badges: Vec<TwitchChatBadge>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TwitchChatBadge {
	#[serde(rename = "set_id")]
	pub(crate) set_id: String,
	pub(crate) id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChannelBanEvent {
	pub(crate) user_id: String,
	pub(crate) user_login: String,
	pub(crate) user_name: String,

	#[allow(dead_code)]
	pub(crate) broadcaster_user_id: String,
	pub(crate) broadcaster_user_login: String,
	#[allow(dead_code)]
	pub(crate) broadcaster_user_name: String,

	pub(crate) moderator_user_id: String,
	pub(crate) moderator_user_login: String,
	pub(crate) moderator_user_name: String,

	#[serde(default)]
	pub(crate) reason: Option<String>,

	#[allow(dead_code)]
	pub(crate) banned_at: String,

	/// RFC3339 timestamp if timeout; null if permanent ban.
	#[serde(default)]
	pub(crate) ends_at: Option<String>,

	pub(crate) is_permanent: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChannelModerateEvent {
	#[allow(dead_code)]
	pub(crate) broadcaster_user_id: String,
	pub(crate) broadcaster_user_login: String,
	#[allow(dead_code)]
	pub(crate) broadcaster_user_name: String,

	pub(crate) moderator_user_id: String,
	pub(crate) moderator_user_login: String,
	pub(crate) moderator_user_name: String,

	pub(crate) action: String,

	#[serde(default)]
	pub(crate) action_data: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChannelRaidEvent {
	pub(crate) from_broadcaster_user_id: String,
	pub(crate) from_broadcaster_user_login: String,
	pub(crate) from_broadcaster_user_name: String,

	#[allow(dead_code)]
	pub(crate) to_broadcaster_user_id: String,
	pub(crate) to_broadcaster_user_login: String,
	#[allow(dead_code)]
	pub(crate) to_broadcaster_user_name: String,

	pub(crate) viewers: u64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChannelCheerEvent {
	pub(crate) is_anonymous: bool,

	#[serde(default)]
	pub(crate) user_id: Option<String>,
	#[serde(default)]
	pub(crate) user_login: Option<String>,
	#[serde(default)]
	pub(crate) user_name: Option<String>,

	#[allow(dead_code)]
	pub(crate) broadcaster_user_id: String,
	pub(crate) broadcaster_user_login: String,
	#[allow(dead_code)]
	pub(crate) broadcaster_user_name: String,

	#[serde(default)]
	pub(crate) message: Option<String>,

	pub(crate) bits: u64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChannelSubscribeEvent {
	pub(crate) user_id: String,
	pub(crate) user_login: String,
	pub(crate) user_name: String,

	#[allow(dead_code)]
	pub(crate) broadcaster_user_id: String,
	pub(crate) broadcaster_user_login: String,
	#[allow(dead_code)]
	pub(crate) broadcaster_user_name: String,

	pub(crate) tier: String,
	pub(crate) is_gift: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChannelChatMessageContent {
	pub(crate) text: String,
	#[serde(default)]
	pub(crate) fragments: Vec<ChannelChatMessageFragment>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChannelChatMessageFragment {
	#[serde(rename = "type")]
	pub(crate) kind: String,
	pub(crate) text: String,
	#[serde(default)]
	pub(crate) emote: Option<ChannelChatMessageFragmentEmote>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChannelChatMessageFragmentEmote {
	pub(crate) id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChannelChatMessageDeleteEvent {
	#[allow(dead_code)]
	pub(crate) broadcaster_user_id: String,
	pub(crate) broadcaster_user_login: String,
	#[allow(dead_code)]
	pub(crate) broadcaster_user_name: String,
	#[allow(dead_code)]
	pub(crate) target_user_id: String,
	#[allow(dead_code)]
	pub(crate) target_user_login: String,
	#[allow(dead_code)]
	pub(crate) target_user_name: String,
	pub(crate) message_id: String,
}

/// Extract `metadata.message_type` from a raw EventSub WS JSON string.
pub(crate) fn peek_message_type(raw_json: &str) -> anyhow::Result<String> {
	let peek: EventSubMetadataPeek = serde_json::from_str(raw_json).context("parse EventSub metadata peek")?;
	Ok(peek.metadata.message_type)
}

/// Extract `metadata.subscription_type` from a raw EventSub WS JSON string (if present).
pub(crate) fn peek_subscription_type(raw_json: &str) -> anyhow::Result<Option<String>> {
	let peek: EventSubMetadataPeek = serde_json::from_str(raw_json).context("parse EventSub metadata peek")?;
	Ok(peek.metadata.subscription_type)
}

/// Parse a raw WS message as `session_welcome`.
pub(crate) fn parse_welcome(raw_json: &str) -> anyhow::Result<EventSubWelcomeMessage> {
	serde_json::from_str(raw_json).context("parse session_welcome")
}

/// Parse a raw WS message as `session_reconnect`.
pub(crate) fn parse_reconnect(raw_json: &str) -> anyhow::Result<EventSubReconnectMessage> {
	serde_json::from_str(raw_json).context("parse session_reconnect")
}

/// Parse a raw WS message as `notification` of `channel.chat.message`.
pub(crate) fn parse_channel_chat_message_notification(
	raw_json: &str,
) -> anyhow::Result<EventSubNotification<ChannelChatMessageEvent>> {
	serde_json::from_str(raw_json).context("parse channel.chat.message notification")
}

/// Parse a raw WS message as `notification` of `channel.chat.message_delete`.
pub(crate) fn parse_channel_chat_message_delete_notification(
	raw_json: &str,
) -> anyhow::Result<EventSubNotification<ChannelChatMessageDeleteEvent>> {
	serde_json::from_str(raw_json).context("parse channel.chat.message_delete notification")
}

/// Parse a raw WS message as `notification` of `channel.ban`.
pub(crate) fn parse_channel_ban_notification(raw_json: &str) -> anyhow::Result<EventSubNotification<ChannelBanEvent>> {
	serde_json::from_str(raw_json).context("parse channel.ban notification")
}

/// Parse a raw WS message as `notification` of `channel.moderate`.
pub(crate) fn parse_channel_moderate_notification(
	raw_json: &str,
) -> anyhow::Result<EventSubNotification<ChannelModerateEvent>> {
	serde_json::from_str(raw_json).context("parse channel.moderate notification")
}

/// Parse a raw WS message as `notification` of `channel.raid`.
pub(crate) fn parse_channel_raid_notification(raw_json: &str) -> anyhow::Result<EventSubNotification<ChannelRaidEvent>> {
	serde_json::from_str(raw_json).context("parse channel.raid notification")
}

/// Parse a raw WS message as `notification` of `channel.cheer`.
pub(crate) fn parse_channel_cheer_notification(raw_json: &str) -> anyhow::Result<EventSubNotification<ChannelCheerEvent>> {
	serde_json::from_str(raw_json).context("parse channel.cheer notification")
}

/// Parse a raw WS message as `notification` of `channel.subscribe`.
pub(crate) fn parse_channel_subscribe_notification(
	raw_json: &str,
) -> anyhow::Result<EventSubNotification<ChannelSubscribeEvent>> {
	serde_json::from_str(raw_json).context("parse channel.subscribe notification")
}

/// Convert a `metadata.message_timestamp` RFC3339 timestamp into `SystemTime`.
///
/// EventSub timestamps are RFC3339 with fractional seconds and Zulu (UTC).
pub(crate) fn parse_message_timestamp_system_time(ts: &str) -> anyhow::Result<SystemTime> {
	let dt = chrono::DateTime::parse_from_rfc3339(ts).context("parse EventSub RFC3339 timestamp")?;
	Ok(SystemTime::from(dt.with_timezone(&chrono::Utc)))
}

#[derive(Debug, Clone)]
pub(crate) struct NormalizedChatNotification {
	#[allow(dead_code)]
	pub(crate) platform: Platform,
	pub(crate) room: RoomKey,
	pub(crate) platform_message_id: PlatformMessageId,
	pub(crate) ws_message_id: String,
	pub(crate) subscription_id: String,
	pub(crate) platform_time: SystemTime,

	pub(crate) chatter_user_id: String,
	pub(crate) chatter_user_login: String,
	pub(crate) chatter_user_name: String,

	pub(crate) text: String,
	pub(crate) badge_ids: Vec<String>,
	pub(crate) emotes: Vec<crate::AssetRef>,
}

#[derive(Debug, Clone)]
pub(crate) struct NormalizedChannelBanNotification {
	#[allow(dead_code)]
	pub(crate) platform: Platform,
	pub(crate) room: RoomKey,
	pub(crate) ws_message_id: String,
	pub(crate) subscription_id: String,
	pub(crate) platform_time: SystemTime,

	pub(crate) target_user_id: String,
	pub(crate) target_user_login: String,
	pub(crate) target_user_name: String,

	pub(crate) moderator_user_id: String,
	pub(crate) moderator_user_login: String,
	pub(crate) moderator_user_name: String,

	pub(crate) is_permanent: bool,
	pub(crate) ends_at: Option<SystemTime>,
	pub(crate) reason: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct NormalizedChannelModerateNotification {
	#[allow(dead_code)]
	pub(crate) platform: Platform,
	pub(crate) room: RoomKey,
	pub(crate) ws_message_id: String,
	pub(crate) subscription_id: String,
	pub(crate) platform_time: SystemTime,

	pub(crate) moderator_user_id: String,
	pub(crate) moderator_user_login: String,
	pub(crate) moderator_user_name: String,

	pub(crate) action: String,
	pub(crate) action_data: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub(crate) struct NormalizedChannelRaidNotification {
	#[allow(dead_code)]
	pub(crate) platform: Platform,
	pub(crate) room: RoomKey,
	pub(crate) ws_message_id: String,
	pub(crate) subscription_id: String,
	pub(crate) platform_time: SystemTime,

	pub(crate) from_broadcaster_user_id: String,
	pub(crate) from_broadcaster_user_login: String,
	pub(crate) from_broadcaster_user_name: String,

	pub(crate) viewers: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct NormalizedChannelCheerNotification {
	#[allow(dead_code)]
	pub(crate) platform: Platform,
	pub(crate) room: RoomKey,
	pub(crate) ws_message_id: String,
	pub(crate) subscription_id: String,
	pub(crate) platform_time: SystemTime,

	pub(crate) bits: u64,
	pub(crate) message: Option<String>,

	pub(crate) user_id: Option<String>,
	pub(crate) user_login: Option<String>,
	pub(crate) user_name: Option<String>,

	pub(crate) is_anonymous: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct NormalizedChannelSubscribeNotification {
	#[allow(dead_code)]
	pub(crate) platform: Platform,
	pub(crate) room: RoomKey,
	pub(crate) ws_message_id: String,
	pub(crate) subscription_id: String,
	pub(crate) platform_time: SystemTime,

	pub(crate) user_id: String,
	pub(crate) user_login: String,
	pub(crate) user_name: String,

	pub(crate) tier: String,
	pub(crate) is_gift: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct NormalizedChatMessageDeleteNotification {
	#[allow(dead_code)]
	pub(crate) platform: Platform,
	pub(crate) room: RoomKey,
	pub(crate) target_message_platform_id: PlatformMessageId,
	pub(crate) ws_message_id: String,
	pub(crate) subscription_id: String,
	pub(crate) platform_time: SystemTime,

	pub(crate) target_user_id: String,
	pub(crate) target_user_login: String,
	pub(crate) target_user_name: String,
}

pub(crate) fn try_normalize_channel_chat_message(raw_json: &str) -> anyhow::Result<Option<NormalizedChatNotification>> {
	let peek: EventSubMetadataPeek = serde_json::from_str(raw_json).context("parse EventSub metadata peek")?;

	if peek.metadata.message_type != "notification" {
		return Ok(None);
	}
	if peek.metadata.subscription_type.as_deref() != Some("channel.chat.message") {
		return Ok(None);
	}

	let msg: EventSubNotification<ChannelChatMessageEvent> = parse_channel_chat_message_notification(raw_json)?;

	let platform_time = parse_message_timestamp_system_time(&msg.metadata.message_timestamp)?;
	let platform_message_id = PlatformMessageId::new(msg.payload.event.message_id.clone())
		.context("construct PlatformMessageId from Twitch message_id")?;

	let room_id = RoomId::new(msg.payload.event.broadcaster_user_login.clone())
		.context("construct RoomId from broadcaster_user_login")?;
	let room = RoomKey::new(Platform::Twitch, room_id);

	Ok(Some(NormalizedChatNotification {
		platform: Platform::Twitch,
		room,
		platform_message_id,
		ws_message_id: msg.metadata.message_id,
		subscription_id: msg.payload.subscription.id,
		platform_time,

		chatter_user_id: msg.payload.event.chatter_user_id,
		chatter_user_login: msg.payload.event.chatter_user_login,
		chatter_user_name: msg.payload.event.chatter_user_name,

		text: msg.payload.event.message.text,
		badge_ids: msg
			.payload
			.event
			.badges
			.into_iter()
			.map(|badge| format!("twitch:{}:{}", badge.set_id, badge.id))
			.collect(),
		emotes: twitch_emotes_from_fragments(&msg.payload.event.message.fragments),
	}))
}

fn twitch_emotes_from_fragments(fragments: &[ChannelChatMessageFragment]) -> Vec<crate::AssetRef> {
	let mut seen = std::collections::HashSet::new();
	let mut emotes = Vec::new();
	for fragment in fragments {
		if fragment.kind != "emote" {
			continue;
		}

		let Some(emote) = fragment.emote.as_ref() else {
			continue;
		};

		if !seen.insert(emote.id.clone()) {
			continue;
		}

		let base = format!("https://static-cdn.jtvnw.net/emoticons/v2/{}/default/dark", emote.id);
		emotes.push(crate::AssetRef {
			id: emote.id.clone(),
			name: fragment.text.clone(),
			images: vec![
				crate::AssetImage {
					scale: crate::AssetScale::One,
					url: format!("{base}/1.0"),
					format: "png".to_string(),
					width: 28,
					height: 28,
				},
				crate::AssetImage {
					scale: crate::AssetScale::Two,
					url: format!("{base}/2.0"),
					format: "png".to_string(),
					width: 56,
					height: 56,
				},
				crate::AssetImage {
					scale: crate::AssetScale::Three,
					url: format!("{base}/3.0"),
					format: "png".to_string(),
					width: 84,
					height: 84,
				},
			],
		});
	}

	emotes
}

pub(crate) fn try_normalize_channel_chat_message_delete(
	raw_json: &str,
) -> anyhow::Result<Option<NormalizedChatMessageDeleteNotification>> {
	let peek: EventSubMetadataPeek = serde_json::from_str(raw_json).context("parse EventSub metadata peek")?;

	if peek.metadata.message_type != "notification" {
		return Ok(None);
	}
	if peek.metadata.subscription_type.as_deref() != Some("channel.chat.message_delete") {
		return Ok(None);
	}

	let msg: EventSubNotification<ChannelChatMessageDeleteEvent> = parse_channel_chat_message_delete_notification(raw_json)?;

	let platform_time = parse_message_timestamp_system_time(&msg.metadata.message_timestamp)?;
	let target_message_platform_id = PlatformMessageId::new(msg.payload.event.message_id.clone())
		.context("construct PlatformMessageId from Twitch deleted message_id")?;

	let room_id = RoomId::new(msg.payload.event.broadcaster_user_login.clone())
		.context("construct RoomId from broadcaster_user_login")?;
	let room = RoomKey::new(Platform::Twitch, room_id);

	Ok(Some(NormalizedChatMessageDeleteNotification {
		platform: Platform::Twitch,
		room,
		target_message_platform_id,
		ws_message_id: msg.metadata.message_id,
		subscription_id: msg.payload.subscription.id,
		platform_time,

		target_user_id: msg.payload.event.target_user_id,
		target_user_login: msg.payload.event.target_user_login,
		target_user_name: msg.payload.event.target_user_name,
	}))
}

pub(crate) fn try_normalize_channel_ban(raw_json: &str) -> anyhow::Result<Option<NormalizedChannelBanNotification>> {
	let peek: EventSubMetadataPeek = serde_json::from_str(raw_json).context("parse EventSub metadata peek")?;

	if peek.metadata.message_type != "notification" {
		return Ok(None);
	}
	if peek.metadata.subscription_type.as_deref() != Some("channel.ban") {
		return Ok(None);
	}

	let msg: EventSubNotification<ChannelBanEvent> = parse_channel_ban_notification(raw_json)?;

	let platform_time = parse_message_timestamp_system_time(&msg.metadata.message_timestamp)?;

	let room_id = RoomId::new(msg.payload.event.broadcaster_user_login.clone())
		.context("construct RoomId from broadcaster_user_login")?;
	let room = RoomKey::new(Platform::Twitch, room_id);

	let ends_at = match msg.payload.event.ends_at.as_deref() {
		None => None,
		Some(ts) => Some(parse_message_timestamp_system_time(ts).context("parse channel.ban ends_at")?),
	};

	Ok(Some(NormalizedChannelBanNotification {
		platform: Platform::Twitch,
		room,
		ws_message_id: msg.metadata.message_id,
		subscription_id: msg.payload.subscription.id,
		platform_time,

		target_user_id: msg.payload.event.user_id,
		target_user_login: msg.payload.event.user_login,
		target_user_name: msg.payload.event.user_name,

		moderator_user_id: msg.payload.event.moderator_user_id,
		moderator_user_login: msg.payload.event.moderator_user_login,
		moderator_user_name: msg.payload.event.moderator_user_name,

		is_permanent: msg.payload.event.is_permanent,
		ends_at,
		reason: msg.payload.event.reason,
	}))
}

pub(crate) fn try_normalize_channel_moderate(
	raw_json: &str,
) -> anyhow::Result<Option<NormalizedChannelModerateNotification>> {
	let peek: EventSubMetadataPeek = serde_json::from_str(raw_json).context("parse EventSub metadata peek")?;

	if peek.metadata.message_type != "notification" {
		return Ok(None);
	}
	if peek.metadata.subscription_type.as_deref() != Some("channel.moderate") {
		return Ok(None);
	}

	let msg: EventSubNotification<ChannelModerateEvent> = parse_channel_moderate_notification(raw_json)?;

	let platform_time = parse_message_timestamp_system_time(&msg.metadata.message_timestamp)?;

	let room_id = RoomId::new(msg.payload.event.broadcaster_user_login.clone())
		.context("construct RoomId from broadcaster_user_login")?;
	let room = RoomKey::new(Platform::Twitch, room_id);

	Ok(Some(NormalizedChannelModerateNotification {
		platform: Platform::Twitch,
		room,
		ws_message_id: msg.metadata.message_id,
		subscription_id: msg.payload.subscription.id,
		platform_time,

		moderator_user_id: msg.payload.event.moderator_user_id,
		moderator_user_login: msg.payload.event.moderator_user_login,
		moderator_user_name: msg.payload.event.moderator_user_name,

		action: msg.payload.event.action,
		action_data: msg.payload.event.action_data,
	}))
}

pub(crate) fn try_normalize_channel_raid(raw_json: &str) -> anyhow::Result<Option<NormalizedChannelRaidNotification>> {
	let peek: EventSubMetadataPeek = serde_json::from_str(raw_json).context("parse EventSub metadata peek")?;

	if peek.metadata.message_type != "notification" {
		return Ok(None);
	}
	if peek.metadata.subscription_type.as_deref() != Some("channel.raid") {
		return Ok(None);
	}

	let msg: EventSubNotification<ChannelRaidEvent> = parse_channel_raid_notification(raw_json)?;

	let platform_time = parse_message_timestamp_system_time(&msg.metadata.message_timestamp)?;

	let room_id = RoomId::new(msg.payload.event.to_broadcaster_user_login.clone())
		.context("construct RoomId from to_broadcaster_user_login")?;
	let room = RoomKey::new(Platform::Twitch, room_id);

	Ok(Some(NormalizedChannelRaidNotification {
		platform: Platform::Twitch,
		room,
		ws_message_id: msg.metadata.message_id,
		subscription_id: msg.payload.subscription.id,
		platform_time,

		from_broadcaster_user_id: msg.payload.event.from_broadcaster_user_id,
		from_broadcaster_user_login: msg.payload.event.from_broadcaster_user_login,
		from_broadcaster_user_name: msg.payload.event.from_broadcaster_user_name,

		viewers: msg.payload.event.viewers,
	}))
}

pub(crate) fn try_normalize_channel_cheer(raw_json: &str) -> anyhow::Result<Option<NormalizedChannelCheerNotification>> {
	let peek: EventSubMetadataPeek = serde_json::from_str(raw_json).context("parse EventSub metadata peek")?;

	if peek.metadata.message_type != "notification" {
		return Ok(None);
	}
	if peek.metadata.subscription_type.as_deref() != Some("channel.cheer") {
		return Ok(None);
	}

	let msg: EventSubNotification<ChannelCheerEvent> = parse_channel_cheer_notification(raw_json)?;

	let platform_time = parse_message_timestamp_system_time(&msg.metadata.message_timestamp)?;

	let room_id = RoomId::new(msg.payload.event.broadcaster_user_login.clone())
		.context("construct RoomId from broadcaster_user_login")?;
	let room = RoomKey::new(Platform::Twitch, room_id);

	Ok(Some(NormalizedChannelCheerNotification {
		platform: Platform::Twitch,
		room,
		ws_message_id: msg.metadata.message_id,
		subscription_id: msg.payload.subscription.id,
		platform_time,

		bits: msg.payload.event.bits,
		message: msg.payload.event.message,

		user_id: msg.payload.event.user_id,
		user_login: msg.payload.event.user_login,
		user_name: msg.payload.event.user_name,

		is_anonymous: msg.payload.event.is_anonymous,
	}))
}

pub(crate) fn try_normalize_channel_subscribe(
	raw_json: &str,
) -> anyhow::Result<Option<NormalizedChannelSubscribeNotification>> {
	let peek: EventSubMetadataPeek = serde_json::from_str(raw_json).context("parse EventSub metadata peek")?;

	if peek.metadata.message_type != "notification" {
		return Ok(None);
	}
	if peek.metadata.subscription_type.as_deref() != Some("channel.subscribe") {
		return Ok(None);
	}

	let msg: EventSubNotification<ChannelSubscribeEvent> = parse_channel_subscribe_notification(raw_json)?;

	let platform_time = parse_message_timestamp_system_time(&msg.metadata.message_timestamp)?;

	let room_id = RoomId::new(msg.payload.event.broadcaster_user_login.clone())
		.context("construct RoomId from broadcaster_user_login")?;
	let room = RoomKey::new(Platform::Twitch, room_id);

	Ok(Some(NormalizedChannelSubscribeNotification {
		platform: Platform::Twitch,
		room,
		ws_message_id: msg.metadata.message_id,
		subscription_id: msg.payload.subscription.id,
		platform_time,

		user_id: msg.payload.event.user_id,
		user_login: msg.payload.event.user_login,
		user_name: msg.payload.event.user_name,

		tier: msg.payload.event.tier,
		is_gift: msg.payload.event.is_gift,
	}))
}
