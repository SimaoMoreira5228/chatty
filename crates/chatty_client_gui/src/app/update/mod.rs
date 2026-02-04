use std::time::Duration;

use iced::Task;

use crate::app::message::Message;
use crate::app::model::Chatty;

mod chat;
mod input;
mod layout;
mod nav;
mod net;
mod pane;
mod settings;
mod tabs;

impl Chatty {
	pub fn toast(&mut self, msg: String) -> Task<Message> {
		let mut toaster = std::mem::replace(&mut self.state.ui.toaster, crate::app::features::toaster::Toaster::new());
		let task = toaster.update(self, crate::app::features::toaster::ToasterMessage::Show(msg));
		self.state.ui.toaster = toaster;
		task
	}

	pub fn report_error(&mut self, msg: impl Into<String>) -> Task<Message> {
		let msg = msg.into();
		self.state
			.push_notification(crate::app::features::toaster::UiNotificationKind::Error, msg.clone());
		self.toast(msg)
	}

	pub fn report_warning(&mut self, msg: impl Into<String>) -> Task<Message> {
		let msg = msg.into();
		self.state
			.push_notification(crate::app::features::toaster::UiNotificationKind::Warning, msg.clone());
		self.toast(msg)
	}

	pub fn report_info(&mut self, msg: impl Into<String>) -> Task<Message> {
		self.state
			.push_notification(crate::app::features::toaster::UiNotificationKind::Info, msg.into());
		Task::none()
	}

	pub fn update(&mut self, message: Message) -> Task<Message> {
		match message {
			Message::AnimationTick(instant) => {
				self.state.ui.animation_clock = instant;
				if cfg!(debug_assertions) {
					self.state.ui.fps_frame_count = self.state.ui.fps_frame_count.saturating_add(1);
					let elapsed = instant.duration_since(self.state.ui.fps_last_instant);
					if elapsed >= Duration::from_secs(1) {
						let fps = ((self.state.ui.fps_frame_count as f64) / elapsed.as_secs_f64()).round() as u32;
						self.state.ui.fps_value = fps;
						self.state.ui.fps_frame_count = 0;
						self.state.ui.fps_last_instant = instant;
					}
				}
				Task::none()
			}
			Message::CursorMoved(x, y) => self.update_cursor_moved(x, y),
			Message::UserScrolled => self.update_user_scrolled(),
			Message::Navigate(p) => self.update_navigate(p),
			Message::ToasterMessage(msg) => {
				let mut toaster =
					std::mem::replace(&mut self.state.ui.toaster, crate::app::features::toaster::Toaster::new());
				let task = toaster.update(self, msg);
				self.state.ui.toaster = toaster;
				task
			}
			Message::OverlayMessage(msg) => {
				if let Some(mut overlay) = self.state.ui.active_overlay.take() {
					let task = overlay.update(self, msg);
					if self.state.ui.overlay_dismissed {
						self.state.ui.overlay_dismissed = false;
					} else if self.state.ui.active_overlay.is_none() {
						self.state.ui.active_overlay = Some(overlay);
					}

					task
				} else {
					Task::none()
				}
			}
			Message::UsersViewMessage(msg) => {
				let mut view =
					std::mem::replace(&mut self.state.ui.users_view, crate::app::features::users::UsersView::new());
				let task = view.update(self, msg);
				self.state.ui.users_view = view;
				task
			}
			Message::Net(msg) => self.update_net_message(msg),
			Message::Window(msg) => self.update_window_message(msg),
			Message::Chat(msg) => self.update_chat_message(msg),
			Message::Layout(msg) => self.update_layout_message(msg),
			Message::PaneMessage(pane, msg) => self.update_pane_message(pane, msg),
			Message::Settings(msg) => self.update_settings_message(msg),

			Message::ModalDismissed => self.update_modal_dismissed(),
			Message::OpenJoinModal(target) => self.update_open_join_modal(target),
			Message::PaneSubscribed(pane, res) => self.update_pane_subscribed(pane, res),
			Message::TabUnsubscribed(room, res) => self.update_tab_unsubscribed(room, res),
			Message::ClipboardRead(target, txt) => self.update_clipboard_read(target, txt),
			Message::ModifiersChanged(modifiers) => {
				self.state.ui.modifiers = modifiers;
				Task::none()
			}
			Message::DismissToast => self.update_dismiss_toast(),
			Message::CharPressed(ch, modifiers) => self.update_char_pressed(ch, modifiers),
			Message::NamedKeyPressed(named) => self.update_named_key_pressed(named),
			Message::TabSelected(id) => self.update_tab_selected(id),
			Message::AddTabPressed => self.update_add_tab_pressed(),
			Message::CloseTabPressed(id) => self.update_close_tab_pressed(id),
			Message::PopTab(id) => self.update_pop_tab(id),
		}
	}
}
