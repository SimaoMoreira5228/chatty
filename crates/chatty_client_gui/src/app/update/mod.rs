use iced::Task;

use crate::app::{Chatty, Message};

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
		let mut toaster = std::mem::replace(&mut self.state.ui.toaster, crate::ui::components::toaster::Toaster::new());
		let task = toaster.update(self, crate::ui::components::toaster::ToasterMessage::Show(msg));
		self.state.ui.toaster = toaster;
		task
	}

	pub fn update(&mut self, message: Message) -> Task<Message> {
		match message {
			Message::AnimationTick(instant) => {
				self.state.ui.animation_clock = instant;
				Task::none()
			}
			Message::CursorMoved(x, y) => self.update_cursor_moved(x, y),
			Message::UserScrolled => self.update_user_scrolled(),
			Message::ChatLogScrolled(pane, viewport) => self.update_chat_log_scrolled(pane, viewport),
			Message::Navigate(p) => self.update_navigate(p),
			Message::ToasterMessage(msg) => {
				let mut toaster =
					std::mem::replace(&mut self.state.ui.toaster, crate::ui::components::toaster::Toaster::new());
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
				let mut view = std::mem::replace(&mut self.state.ui.users_view, crate::ui::users_view::UsersView::new());
				let task = view.update(self, msg);
				self.state.ui.users_view = view;
				task
			}
			Message::ConnectPressed => self.update_connect_pressed(),
			Message::DisconnectPressed => self.update_disconnect_pressed(),
			Message::ConnectFinished(res) => self.update_connect_finished(res),
			Message::PaneMessage(pane, msg) => self.update_pane_message(pane, msg),
			Message::SettingsMessage(msg) => {
				let mut sv = self.state.ui.settings_view.clone();
				let task = sv.update(self, msg);
				self.state.ui.settings_view = sv;
				task
			}
			Message::MessageActionButtonPressed(room, s_id, p_id, a_id) => {
				self.update_message_action_button_pressed(room, s_id, p_id, a_id)
			}
			Message::ModalDismissed => self.update_modal_dismissed(),
			Message::OpenJoinModal(target) => self.update_open_join_modal(target),
			Message::ReplyToMessage(room, s_id, p_id) => self.update_reply_to_message(room, s_id, p_id),
			Message::DeleteMessage(room, s_id, p_id) => self.update_delete_message(room, s_id, p_id),
			Message::TimeoutUser(room, user_id) => self.update_timeout_user(room, user_id),
			Message::BanUser(room, user_id) => self.update_ban_user(room, user_id),
			Message::PaneSubscribed(pane, res) => self.update_pane_subscribed(pane, res),
			Message::TabUnsubscribed(room, res) => self.update_tab_unsubscribed(room, res),
			Message::Sent(res) => self.update_sent(res),
			Message::ClipboardRead(target, txt) => self.update_clipboard_read(target, txt),
			Message::NetPolled(ev) => self.update_net_polled(ev),
			Message::MessageTextEdit(key, action) => self.update_message_text_edit(key, action),
			Message::AutoJoinCompleted(results) => self.update_auto_join_completed(results),
			Message::PaneClicked(pane) => self.update_pane_focus_changed(pane),
			Message::PaneResized(ev) => self.update_pane_resized(ev),
			Message::PaneDragged(ev) => self.update_pane_dragged(ev),
			Message::SplitSpiral => self.update_split_spiral(),
			Message::SplitMasonry => self.update_split_masonry(),
			Message::SplitPressed => self.update_split_pressed(),
			Message::ModifiersChanged(modifiers) => {
				self.state.ui.modifiers = modifiers;
				Task::none()
			}
			Message::CloseFocused => self.update_close_focused(),
			Message::DismissToast => self.update_dismiss_toast(),
			Message::CharPressed(ch, modifiers) => self.update_char_pressed(ch, modifiers),
			Message::NamedKeyPressed(named) => self.update_named_key_pressed(named),
			Message::NavigatePaneLeft => self.update_navigate_pane_left(),
			Message::NavigatePaneDown => self.update_navigate_pane_down(),
			Message::NavigatePaneUp => self.update_navigate_pane_up(),
			Message::NavigatePaneRight => self.update_navigate_pane_right(),
			Message::TabSelected(id) => self.update_tab_selected(id),
			Message::AddTabPressed => self.update_add_tab_pressed(),
			Message::CloseTabPressed(id) => self.update_close_tab_pressed(id),
			Message::PopTab(id) => self.update_pop_tab(id),
			Message::WindowClosed(id) => self.update_window_closed(id),
			Message::WindowOpened(_) => Task::none(),
			Message::WindowResized(id, w, h) => self.update_window_resized(id, w, h),
			Message::WindowMoved(id, x, y) => self.update_window_moved(id, x, y),
		}
	}
}
