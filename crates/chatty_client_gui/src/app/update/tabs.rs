use iced::Task;

use crate::app::{Chatty, Message};

impl Chatty {
	pub fn update_tab_selected(&mut self, id: crate::app::state::TabId) -> Task<Message> {
		if self.state.tabs.contains_key(&id) {
			self.state.selected_tab_id = Some(id);
		}
		Task::none()
	}

	pub fn update_add_tab_pressed(&mut self) -> Task<Message> {
		self.update_open_join_modal(crate::app::types::JoinTarget::NewTab)
	}

	pub fn update_close_tab_pressed(&mut self, id: crate::app::state::TabId) -> Task<Message> {
		self.state.tabs.remove(&id);
		self.state.tab_order.retain(|&tid| tid != id);

		if self.state.selected_tab_id == Some(id) {
			self.state.selected_tab_id = self.state.tab_order.first().copied();
		}

		Task::none()
	}

	pub fn update_pop_tab(&mut self, id: crate::app::state::TabId) -> Task<Message> {
		if self.state.pop_tab(id).is_some() {
			self.state.pending_popped_tabs.push_back(id);
			let (id, task) = iced::window::open(iced::window::Settings {
				exit_on_close_request: false,
				..Default::default()
			});

			if let Some(tab_id) = self.state.pending_popped_tabs.pop_back() {
				let title = self
					.state
					.tabs
					.get(&tab_id)
					.map(|t| t.title.clone())
					.unwrap_or_else(|| "Chatty Popout".to_string());

				let win_model = crate::app::state::WindowModel {
					id: crate::app::state::WindowId(0),
					title,
					tabs: vec![tab_id],
					active_tab: Some(tab_id),
					width: 800,
					height: 600,
					x: -1,
					y: -1,
				};
				self.state.popped_windows.insert(id, win_model);
			}
			return task.map(Message::WindowOpened);
		}
		Task::none()
	}

	pub fn update_window_closed(&mut self, id: iced::window::Id) -> Task<Message> {
		if let Some(win_model) = self.state.popped_windows.remove(&id) {
			for tab_id in win_model.tabs {
				if !self.state.tab_order.contains(&tab_id) {
					self.state.tab_order.push(tab_id);
				}

				if self.state.selected_tab_id.is_none() {
					self.state.selected_tab_id = Some(tab_id);
				}
			}

			iced::window::close(id)
		} else if self.state.ui.main_window_id == Some(id) {
			if let Some(shutdown) = self.shutdown.take() {
				shutdown.shutdown();
			}

			iced::exit()
		} else {
			Task::none()
		}
	}

	pub fn update_window_resized(&mut self, id: iced::window::Id, width: u32, height: u32) -> Task<Message> {
		if let Some(win_model) = self.state.popped_windows.get_mut(&id) {
			win_model.width = width;
			win_model.height = height;
		} else {
			self.state.main_window_geometry.width = width;
			self.state.main_window_geometry.height = height;
			self.state.ui.window_size = Some((width as f32, height as f32));
		}
		Task::none()
	}

	pub fn update_window_moved(&mut self, id: iced::window::Id, x: i32, y: i32) -> Task<Message> {
		if let Some(win_model) = self.state.popped_windows.get_mut(&id) {
			win_model.x = x;
			win_model.y = y;
		} else {
			self.state.main_window_geometry.x = x;
			self.state.main_window_geometry.y = y;
		}
		Task::none()
	}
}
