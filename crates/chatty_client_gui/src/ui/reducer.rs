#![forbid(unsafe_code)]

use std::time::SystemTime;

use crate::ui::app_state::{
	AppState, AssetBundleUi, ChatItem, ChatMessageUi, RoomPermissions, SystemNoticeUi, UiNotificationKind, WindowId,
};
use crate::ui::net::UiEvent;
use chatty_domain::RoomTopic;
use chatty_protocol::pb;

#[derive(Debug, Clone)]
pub enum UiAction {
	NetEvent(UiEvent),
}

#[derive(Debug, Clone)]
pub enum UiCommand {
	PushNotification {
		kind: UiNotificationKind,
		message: String,
	},
	AppendSystemLine {
		text: String,
	},
}

pub fn reduce(state: &mut AppState, _window_id: WindowId, action: UiAction) -> Vec<UiCommand> {
	let mut commands = Vec::new();
	match action {
		UiAction::NetEvent(event) => match event {
			UiEvent::Connecting => {
				state.set_connection_status(crate::ui::app_state::ConnectionStatus::Connecting);
				commands.push(UiCommand::PushNotification {
					kind: UiNotificationKind::Info,
					message: "Connecting...".to_string(),
				});
			}
			UiEvent::Reconnecting {
				attempt,
				next_retry_in_ms,
			} => {
				state.set_connection_status(crate::ui::app_state::ConnectionStatus::Reconnecting {
					attempt,
					next_retry_in_ms,
				});
				commands.push(UiCommand::PushNotification {
					kind: UiNotificationKind::Warning,
					message: format!("Reconnecting in {}s (attempt {})", next_retry_in_ms / 1000, attempt),
				});
			}
			UiEvent::ChatMessage {
				topic,
				author_login,
				author_display,
				author_id,
				text,
				server_message_id,
				platform_message_id,
				badge_ids,
				..
			} => {
				if let Ok(room) = RoomTopic::parse(&topic) {
					let msg = ChatMessageUi {
						time: SystemTime::now(),
						platform: room.platform,
						room: room.clone(),
						server_message_id,
						author_id,
						user_login: author_login.clone(),
						user_display: author_display.clone(),
						text: text.clone(),
						badge_ids,
						platform_message_id,
					};
					state.push_message(msg);
				} else {
					commands.push(UiCommand::AppendSystemLine {
						text: format!("Unmapped topic: {}", topic),
					});
				}
			}
			UiEvent::TopicLagged {
				topic, dropped, detail, ..
			} => {
				if let Ok(room) = RoomTopic::parse(&topic) {
					state.push_lagged(&room, dropped, Some(detail.clone()));
				} else {
					commands.push(UiCommand::AppendSystemLine {
						text: format!("Messages skipped ({}): {}", dropped, detail),
					});
				}
			}
			UiEvent::Connected { server_name, .. } => {
				state.set_connection_status(crate::ui::app_state::ConnectionStatus::Connected {
					server: server_name.clone(),
				});
				commands.push(UiCommand::PushNotification {
					kind: UiNotificationKind::Success,
					message: format!("Connected to {}", server_name),
				});
				commands.push(UiCommand::AppendSystemLine {
					text: format!("Connected to {}", server_name),
				});
			}
			UiEvent::Disconnected { reason } => {
				state.set_connection_status(crate::ui::app_state::ConnectionStatus::Disconnected {
					reason: Some(reason.clone()),
				});
				commands.push(UiCommand::PushNotification {
					kind: UiNotificationKind::Warning,
					message: format!("Disconnected: {}", reason),
				});
				commands.push(UiCommand::AppendSystemLine {
					text: format!("Disconnected: {}", reason),
				});
			}
			UiEvent::Error { message } => {
				commands.push(UiCommand::PushNotification {
					kind: UiNotificationKind::Error,
					message: format!("Error: {}", message),
				});
				commands.push(UiCommand::AppendSystemLine {
					text: format!("Error: {}", message),
				});
			}
			UiEvent::RoomPermissions {
				topic,
				can_send,
				can_reply,
				can_delete,
				can_timeout,
				can_ban,
				is_moderator,
				is_broadcaster,
			} => {
				if let Ok(room) = RoomTopic::parse(&topic) {
					state.set_room_permissions(
						room,
						RoomPermissions {
							can_send,
							can_reply,
							can_delete,
							can_timeout,
							can_ban,
							is_moderator,
							is_broadcaster,
						},
					);
				}
			}
			UiEvent::AssetBundle {
				topic,
				cache_key,
				etag,
				provider,
				scope,
				emotes,
				badges,
			} => {
				let room = RoomTopic::parse(&topic).ok();
				state.upsert_asset_bundle(
					room,
					AssetBundleUi {
						cache_key,
						etag,
						provider,
						scope,
						emotes,
						badges,
					},
				);
			}
			UiEvent::CommandResult { status, detail } => {
				if status != pb::command_result::Status::Ok as i32 {
					let msg = if detail.is_empty() {
						"Command failed".to_string()
					} else {
						detail
					};
					commands.push(UiCommand::PushNotification {
						kind: UiNotificationKind::Error,
						message: msg.clone(),
					});
					commands.push(UiCommand::AppendSystemLine { text: msg });
				}
			}
		},
	}
	commands
}

pub fn apply_commands(state: &mut AppState, window_id: WindowId, commands: Vec<UiCommand>) {
	for command in commands {
		match command {
			UiCommand::PushNotification { kind, message } => {
				state.push_notification(kind, message);
			}
			UiCommand::AppendSystemLine { text } => {
				if let Some(tab_id) = state.windows.get(&window_id).and_then(|w| w.active_tab)
					&& let Some(tab) = state.tabs.get_mut(&tab_id)
				{
					tab.log.push(ChatItem::SystemNotice(SystemNoticeUi {
						time: SystemTime::now(),
						text,
					}));
				}
			}
		}
	}
}
