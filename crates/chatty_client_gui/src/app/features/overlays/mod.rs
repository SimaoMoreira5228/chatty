#![forbid(unsafe_code)]

use iced::Task;

use crate::app::message::Message;
use crate::app::model::Chatty;

pub mod confirm;
pub mod join;
pub mod layout;
pub mod message_action;

pub use confirm::{ConfirmModal, ConfirmModalKind, ConfirmModalMessage};
pub use join::{JoinModal, JoinModalMessage};
pub use layout::{LayoutModal, LayoutModalKind, LayoutModalMessage};
pub use message_action::{MessageActionMenu, MessageActionMenuMessage};

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
}
