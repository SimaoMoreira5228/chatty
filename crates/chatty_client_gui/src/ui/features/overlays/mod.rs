use iced::Element;

use crate::app::features::overlays::ActiveOverlay;
use crate::app::message::Message;
use crate::app::model::Chatty;
use crate::theme;

pub mod confirm;
pub mod join;
pub mod layout;
pub mod message_action;
pub mod overlay;

impl ActiveOverlay {
	pub fn view<'a>(&'a self, app: &'a Chatty, palette: theme::Palette) -> Element<'a, Message> {
		match self {
			ActiveOverlay::Join(modal) => modal.view(palette),
			ActiveOverlay::Layout(modal) => modal.view(palette),
			ActiveOverlay::MessageAction(modal) => modal.view(modal.view_model(app), palette),
			ActiveOverlay::Confirm(modal) => modal.view(palette),
		}
	}
}
