#![forbid(unsafe_code)]

use chatty_domain::RoomKey;
use iced::Task;
use iced::widget::pane_grid;

use crate::app::features::tabs::TabId;
use crate::app::message::Message;
use crate::app::model::Chatty;

#[derive(Debug, Clone)]
pub enum ChatPaneMessage {
	ComposerChanged(String),
	SendPressed,
}

#[derive(Debug, Clone)]
pub struct ChatPane {
	pub tab_id: Option<TabId>,
	pub composer: String,
	pub join_raw: String,
	pub reply_to_server_message_id: String,
	pub reply_to_platform_message_id: String,
	pub reply_to_room: Option<RoomKey>,
}

impl ChatPane {
	pub fn new(tab_id: Option<TabId>) -> Self {
		Self {
			tab_id,
			composer: String::new(),
			join_raw: String::new(),
			reply_to_server_message_id: String::new(),
			reply_to_platform_message_id: String::new(),
			reply_to_room: None,
		}
	}

	pub fn update(&mut self, pane: pane_grid::Pane, message: ChatPaneMessage, app: &mut Chatty) -> Task<Message> {
		match message {
			ChatPaneMessage::ComposerChanged(v) => {
				self.composer = v;
				app.save_ui_layout();
				Task::none()
			}
			ChatPaneMessage::SendPressed => {
				let task = app.update_pane_send_pressed(pane);
				self.composer.clear();
				app.save_ui_layout();
				task
			}
		}
	}
}
