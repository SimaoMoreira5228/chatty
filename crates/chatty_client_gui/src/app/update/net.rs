use std::time::{Duration, SystemTime};

use chatty_domain::{RoomKey, RoomTopic};
use iced::Task;
use iced::widget::pane_grid;
use rust_i18n::t;
use tracing::info;

use crate::app::message::{Message, NetMessage};
use crate::app::message_format::{build_message_key, tokenize_message_text};
use crate::app::model::Chatty;
use crate::app::net::recv_next;
use crate::app::state::ConnectionStatus;
use crate::app::view_models::{AssetBundleUi, ChatMessageUi};
use crate::net::UiEvent;
use crate::settings;

impl Chatty {
	pub fn update_net_message(&mut self, message: NetMessage) -> Task<Message> {
		match message {
			NetMessage::ConnectPressed => self.update_connect_pressed(),
			NetMessage::DisconnectPressed => self.update_disconnect_pressed(),
			NetMessage::ConnectFinished(res) => self.update_connect_finished(res),
			NetMessage::NetPolled(ev) => self.update_net_polled(ev),
			NetMessage::AutoJoinCompleted(results) => self.update_auto_join_completed(results),
		}
	}

	pub fn update_chat_message_prepared(&mut self, msg: ChatMessageUi) -> Task<Message> {
		let key = msg.key.clone();
		self.message_text_editors.entry(key).or_default();
		let tabs = self.state.push_message(msg);

		if self.state.ui.follow_end {
			for tid in tabs {
				let mut panes_with_tab: Vec<pane_grid::Pane> = Vec::new();
				for tab in self.state.tabs.values() {
					for (pane, p) in tab.panes.iter() {
						if p.tab_id == Some(tid) {
							panes_with_tab.push(*pane);
						}
					}
				}

				if !panes_with_tab.is_empty() {
					let do_focus = match self.state.ui.last_focus {
						Some(ts) => ts.elapsed() >= Duration::from_millis(250),
						None => true,
					};

					let recv_cmd = Task::perform(recv_next(self.net_rx.clone()), |ev| {
						Message::Net(crate::app::message::NetMessage::NetPolled(ev))
					});

					if do_focus {
						self.state.ui.last_focus = Some(self.clock.now());
						let mut cmds = Vec::new();
						for pane in panes_with_tab {
							let id = format!("log-{:?}", pane);
							cmds.push(iced::widget::operation::snap_to_end(id));
						}
						cmds.push(recv_cmd);
						return Task::batch(cmds);
					} else {
						return recv_cmd;
					}
				}
			}
		}

		Task::none()
	}

	pub fn update_server_endpoint_changed(&mut self, v: String) -> Task<Message> {
		self.state.ui.server_endpoint_quic = v;
		Task::none()
	}

	pub fn update_server_auth_token_changed(&mut self, v: String) -> Task<Message> {
		self.state.ui.server_auth_token = v;
		Task::none()
	}

	pub fn update_connect_pressed(&mut self) -> Task<Message> {
		let mut gs = self.state.gui_settings().clone();
		if !chatty_client_core::ClientConfigV1::server_endpoint_locked() {
			gs.server_endpoint_quic = self.state.ui.server_endpoint_quic.clone();
		}
		gs.server_auth_token = self.state.ui.server_auth_token.clone();

		let cfg = match settings::build_client_config(&gs) {
			Ok(c) => c,
			Err(e) => {
				return self.report_error(e);
			}
		};

		self.state.set_connection_status(ConnectionStatus::Connecting);
		let net = self.net_effects.clone();
		Task::perform(net.connect(cfg), |res| {
			Message::Net(crate::app::message::NetMessage::ConnectFinished(res))
		})
	}

	pub fn update_disconnect_pressed(&mut self) -> Task<Message> {
		let net = self.net_effects.clone();
		Task::perform(net.disconnect("user".to_string()), |res| {
			Message::Net(crate::app::message::NetMessage::ConnectFinished(res))
		})
	}

	pub fn update_connect_finished(&mut self, res: Result<(), String>) -> Task<Message> {
		match res {
			Ok(()) => {
				self.state.ui.active_overlay = None;
			}
			Err(e) => {
				self.state.ui.active_overlay = Some(crate::app::features::overlays::ActiveOverlay::Layout(
					crate::app::features::overlays::LayoutModal::new_error(e.clone()),
				));
				return self.report_error(e);
			}
		}
		Task::none()
	}

	pub fn update_auto_join_completed(&mut self, results: Vec<(RoomKey, Result<(), String>)>) -> Task<Message> {
		let mut tasks = Vec::new();
		for (room, res) in results {
			if let Err(e) = res {
				let msg = format!("{} {}: {}", t!("failed_to_subscribe"), RoomTopic::format(&room), e);
				tasks.push(self.report_error(msg));
			}
		}
		Task::batch(tasks)
	}

	pub(crate) fn update_net_polled(&mut self, ev: Option<UiEvent>) -> Task<Message> {
		let Some(ev) = ev else {
			return self.report_warning(t!("network_event_stream_closed").to_string());
		};

		let event_kind = match ev {
			UiEvent::Connecting => "connecting",
			UiEvent::Reconnecting { .. } => "reconnecting",
			UiEvent::Connected { .. } => "connected",
			UiEvent::Disconnected { .. } => "disconnected",
			UiEvent::ErrorWithServer { .. } => "error",
			UiEvent::ChatMessage { .. } => "chat_message",
			UiEvent::RoomPermissions { .. } => "room_permissions",
			UiEvent::RoomState { .. } => "room_state",
			UiEvent::AssetBundle { .. } => "asset_bundle",
			UiEvent::CommandResult { .. } => "command_result",
		};
		tracing::debug!(event_kind, "NetPolled event received in UI");

		let mut pre_task: Option<Task<Message>> = None;
		if let Some(room) = self.collect_orphaned_tab() {
			info!(%room, "NetPolled: found orphaned tab; unsubscribing (continuing to process event)");
			let net = self.net_effects.clone();
			pre_task = Some(Task::perform(
				async move { (room.clone(), net.unsubscribe_room_key(room.clone()).await) },
				|(room, res)| Message::TabUnsubscribed(room, res),
			));
		}

		let ev_task_opt = match ev {
			UiEvent::Connecting
			| UiEvent::Reconnecting { .. }
			| UiEvent::Connected { .. }
			| UiEvent::Disconnected { .. } => self.handle_connection_event(ev),
			UiEvent::ErrorWithServer { .. } => self.handle_error_event(ev),
			UiEvent::ChatMessage { .. } => self.handle_chat_event(ev),
			UiEvent::RoomPermissions { .. } | UiEvent::RoomState { .. } => self.handle_room_event(ev),
			UiEvent::AssetBundle { .. } => self.handle_asset_event(ev),
			UiEvent::CommandResult { .. } => self.handle_command_result_event(ev),
		};

		let ev_task = ev_task_opt.unwrap_or_else(Task::none);
		let recv_task = Task::perform(recv_next(self.net_rx.clone()), |ev| {
			Message::Net(crate::app::message::NetMessage::NetPolled(ev))
		});

		if let Some(pre) = pre_task {
			info!("scheduling recv_next and running pre-task");
			Task::batch(vec![pre, ev_task, recv_task])
		} else {
			info!("scheduling recv_next again");
			Task::batch(vec![ev_task, recv_task])
		}
	}

	fn handle_connection_event(&mut self, ev: UiEvent) -> Option<Task<Message>> {
		match ev {
			UiEvent::Connecting => {
				self.state.set_connection_status(ConnectionStatus::Connecting);
				None
			}
			UiEvent::Reconnecting {
				attempt,
				next_retry_in_ms,
			} => {
				self.state.set_connection_status(ConnectionStatus::Reconnecting {
					attempt,
					next_retry_in_ms,
				});
				None
			}
			UiEvent::Connected {
				server_name,
				server_instance_id,
			} => {
				let server = if server_instance_id.trim().is_empty() || server_instance_id == "unknown" {
					server_name
				} else {
					format!("{} ({})", server_name, server_instance_id)
				};
				self.state.set_connection_status(ConnectionStatus::Connected { server });

				let mut rooms = Vec::new();
				let mut seen = std::collections::HashSet::new();
				for tab in self.state.tabs.values() {
					for room in &tab.target.0 {
						if seen.insert(room.clone()) {
							rooms.push(room.clone());
						}
					}
				}

				if !rooms.is_empty() {
					let net = self.net_effects.clone();
					Some(Task::perform(
						async move {
							let mut results = Vec::new();
							for room in rooms {
								let res = net.subscribe_room_key(room.clone()).await;
								results.push((room, res));
							}
							results
						},
						|results| Message::Net(crate::app::message::NetMessage::AutoJoinCompleted(results)),
					))
				} else {
					None
				}
			}
			UiEvent::Disconnected { reason } => {
				let mut t = Task::none();
				if !reason.trim().is_empty() {
					t = self.toast(format!("{} {reason}", t!("disconnected_colon")));
				}
				self.state
					.set_connection_status(ConnectionStatus::Disconnected { reason: Some(reason) });
				Some(t)
			}
			_ => unreachable!("handle_connection_event called with non-connection event"),
		}
	}

	fn handle_error_event(&mut self, ev: UiEvent) -> Option<Task<Message>> {
		if let UiEvent::ErrorWithServer { message, server } = ev {
			let msg = if let Some(s) = server.as_ref() {
				format!("{} (server {})", message, s)
			} else {
				message
			};
			if !msg.trim().is_empty() {
				return Some(self.report_error(msg));
			}
			Some(self.report_error(msg))
		} else {
			None
		}
	}

	fn handle_chat_event(&mut self, ev: UiEvent) -> Option<Task<Message>> {
		match ev {
			UiEvent::ChatMessage {
				topic,
				author_login,
				author_display,
				author_id,
				text,
				server_message_id,
				platform_message_id,
				badge_ids,
				emotes,
			} => {
				if let Ok(room) = RoomTopic::parse(&topic) {
					let _tid = self.ensure_tab_for_rooms(vec![room.clone()]);
					let tokens = tokenize_message_text(&text);
					let time = SystemTime::now();
					let display_name = author_display.clone().unwrap_or_else(|| author_login.clone());
					let key = build_message_key(&room, server_message_id.as_deref(), platform_message_id.as_deref(), time);
					let msg = ChatMessageUi {
						time,
						platform: room.platform,
						room: room.clone(),
						key,
						server_message_id,
						author_id,
						user_login: author_login,
						user_display: author_display,
						display_name,
						text,
						tokens,
						badge_ids,
						emotes,
						platform_message_id,
					};
					Some(self.update_chat_message_prepared(msg))
				} else {
					Some(self.report_warning(format!("{}: {topic}", t!("unparseable_topic"))))
				}
			}
			_ => unreachable!("handle_chat_event called with non-chat event"),
		}
	}

	fn handle_room_event(&mut self, ev: UiEvent) -> Option<Task<Message>> {
		match ev {
			UiEvent::RoomPermissions { .. } | UiEvent::RoomState { .. } => {
				if let UiEvent::RoomPermissions {
					topic,
					can_send,
					can_reply,
					can_delete,
					can_timeout,
					can_ban,
					is_moderator,
					is_broadcaster,
				} = ev
				{
					if let Ok(room) = RoomTopic::parse(&topic) {
						self.state.room_permissions.insert(
							room,
							crate::app::room::RoomPermissions {
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
				} else if let UiEvent::RoomState {
					topic,
					emote_only,
					subscribers_only,
					unique_chat,
					slow_mode,
					slow_mode_wait_time_seconds,
					followers_only,
					followers_only_duration_minutes,
				} = ev && let Ok(room) = RoomTopic::parse(&topic)
				{
					self.state.room_states.insert(
						room,
						crate::app::room::RoomStateUi {
							emote_only,
							subscribers_only,
							unique_chat,
							slow_mode,
							slow_mode_wait_time_seconds,
							followers_only,
							followers_only_duration_minutes,
						},
					);
				}
			}
			_ => unreachable!("handle_room_event called with non-room event"),
		}
		None
	}

	fn handle_asset_event(&mut self, ev: UiEvent) -> Option<Task<Message>> {
		if let UiEvent::AssetBundle {
			topic,
			cache_key,
			etag,
			provider,
			scope,
			emotes,
			badges,
		} = ev
		{
			info!(topic = %topic, cache_key = %cache_key, emote_count = emotes.len(), badge_count = badges.len(), "received AssetBundle UiEvent");
			let ck = cache_key.clone();
			let bundle = AssetBundleUi {
				cache_key: ck.clone(),
				etag,
				provider,
				scope,
				emotes,
				badges,
			};

			let room = chatty_domain::RoomTopic::parse(&topic).ok();
			let is_new = self.state.asset_catalog.register_bundle(bundle.clone(), scope, room);

			if is_new {
				if scope == chatty_protocol::pb::AssetScope::Global as i32 {
					info!(cache_key = %ck, "registering global AssetBundle cache_key");
				} else if let Ok(room) = chatty_domain::RoomTopic::parse(&topic) {
					info!(cache_key = %ck, room = %room, "registering room AssetBundle cache_key");
				}
			}

			self.assets.prefetch_bundle(&bundle, 64);
		}
		None
	}

	fn handle_command_result_event(&mut self, ev: UiEvent) -> Option<Task<Message>> {
		if let UiEvent::CommandResult { status, detail } = ev {
			let _ = self.report_info(format!("command status={status}: {detail}"));

			if status == chatty_protocol::pb::command_result::Status::Ok as i32 {
				let _ = self.pending_commands.pop();
				self.pending_commands.retain(|pc| {
					!matches!(
						pc,
						crate::app::types::PendingCommand::Timeout { .. } | crate::app::types::PendingCommand::Ban { .. }
					)
				});

				self.rebuild_pending_delete_keys();
			}
		}
		None
	}
}
