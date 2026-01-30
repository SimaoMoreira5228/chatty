use iced::{Element, Task};

use crate::app::{Chatty, Message};
use crate::theme;
use crate::ui::modals::confirm::{ConfirmModal, ConfirmModalMessage};
use crate::ui::modals::join::{JoinModal, JoinModalMessage};
use crate::ui::modals::layout::{LayoutModal, LayoutModalMessage};
use crate::ui::modals::message_action::{MessageActionMenu, MessageActionMenuMessage};

#[derive(Debug, Clone)]
pub enum OverlayMessage {
	Join(JoinModalMessage),
	Layout(LayoutModalMessage),
	MessageAction(MessageActionMenuMessage),
	Confirm(ConfirmModalMessage),
}

#[derive(Debug, Clone)]
pub enum ActiveOverlay {
	Join(JoinModal),
	Layout(LayoutModal),
	MessageAction(MessageActionMenu),
	Confirm(ConfirmModal),
}

impl ActiveOverlay {
	pub fn update(&mut self, app: &mut Chatty, message: OverlayMessage) -> Task<Message> {
		match (self, message) {
			(ActiveOverlay::Join(modal), OverlayMessage::Join(msg)) => modal.update(app, msg),
			(ActiveOverlay::Layout(modal), OverlayMessage::Layout(msg)) => modal.update(app, msg),
			(ActiveOverlay::MessageAction(modal), OverlayMessage::MessageAction(msg)) => modal.update(app, msg),
			(ActiveOverlay::Confirm(modal), OverlayMessage::Confirm(msg)) => modal.update(app, msg),
			_ => Task::none(),
		}
	}

	pub fn view<'a>(&'a self, app: &'a Chatty, palette: theme::Palette) -> Element<'a, Message> {
		match self {
			ActiveOverlay::Join(modal) => modal.view(app, palette),
			ActiveOverlay::Layout(modal) => modal.view(app, palette),
			ActiveOverlay::MessageAction(modal) => modal.view(app, palette),
			ActiveOverlay::Confirm(modal) => modal.view(app, palette),
		}
	}
}
