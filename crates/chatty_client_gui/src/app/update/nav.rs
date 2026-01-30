use iced::Task;

use crate::app::{Chatty, Message, Page};

impl Chatty {
	pub fn update_navigate(&mut self, page: Page) -> Task<Message> {
		self.state.ui.page = page;
		self.state.ui.active_overlay = None;
		Task::none()
	}

	pub fn update_cursor_moved(&mut self, x: f32, y: f32) -> Task<Message> {
		self.state.ui.last_cursor_pos = Some((x, y));
		self.set_focused_by_cursor(x, y);
		Task::none()
	}

	pub fn update_user_scrolled(&mut self) -> Task<Message> {
		self.state.ui.follow_end = false;
		Task::none()
	}
}
