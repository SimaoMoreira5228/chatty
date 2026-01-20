#![forbid(unsafe_code)]

use chatty_client_ui::app_state::TabTarget;
use iced::widget::pane_grid;

use crate::app::Chatty;

impl Chatty {
	pub(crate) fn split_spiral(&mut self) {
		let dir = self.spiral_dir % 4;
		self.spiral_dir = (self.spiral_dir + 1) % 4;
		let (axis, ratio, swap) = match dir {
			0 => (pane_grid::Axis::Vertical, 0.618, false),
			1 => (pane_grid::Axis::Horizontal, 0.618, false),
			2 => (pane_grid::Axis::Vertical, 0.382, true),
			_ => (pane_grid::Axis::Horizontal, 0.382, true),
		};

		let inherited_join = self
			.focused_tab_id()
			.and_then(|tid| self.state.tabs.get(&tid))
			.and_then(|tab| match &tab.target {
				TabTarget::Room(room) => Some(format!("{}:{}", room.platform.as_str(), room.room_id.as_str())),
				_ => None,
			})
			.unwrap_or_default();

		let new_state = crate::app::PaneState {
			tab_id: self.focused_tab_id(),
			composer: String::new(),
			join_raw: inherited_join,
			reply_to_server_message_id: String::new(),
			reply_to_platform_message_id: String::new(),
		};

		let old_pane = self.focused_pane;
		if let Some((new_pane, split)) = self.panes.split(axis, old_pane, new_state) {
			self.panes.resize(split, ratio);
			if swap {
				self.panes.swap(old_pane, new_pane);
			}

			self.focused_pane = old_pane;
		}
	}

	pub(crate) fn split_masonry(&mut self) {
		use iced::{Rectangle, Size};

		let bounds = Size::new(4096.0, 4096.0);
		let regions = self.panes.layout().pane_regions(8.0, 50.0, bounds);

		let mut best: Option<(pane_grid::Pane, Rectangle, f32)> = None;
		for (pane, rect) in regions {
			let area = rect.width * rect.height;
			best = match best {
				None => Some((pane, rect, area)),
				Some((bp, br, ba)) => {
					if area > ba || (area == ba && pane == self.focused_pane) {
						Some((pane, rect, area))
					} else {
						Some((bp, br, ba))
					}
				}
			};
		}

		let Some((target_pane, rect, _)) = best else {
			return;
		};

		let axis = if rect.width >= rect.height {
			pane_grid::Axis::Vertical
		} else {
			pane_grid::Axis::Horizontal
		};

		let ratio = if self.masonry_flip { 0.5 } else { 0.618 };
		let swap = self.masonry_flip;
		self.masonry_flip = !self.masonry_flip;

		let inherited_join = self
			.panes
			.get(target_pane)
			.and_then(|ps| ps.tab_id)
			.and_then(|tid| self.state.tabs.get(&tid))
			.and_then(|tab| match &tab.target {
				TabTarget::Room(room) => Some(format!("{}:{}", room.platform.as_str(), room.room_id.as_str())),
				_ => None,
			})
			.unwrap_or_default();

		let new_state = crate::app::PaneState {
			tab_id: self.panes.get(target_pane).and_then(|ps| ps.tab_id),
			composer: String::new(),
			join_raw: inherited_join,
			reply_to_server_message_id: String::new(),
			reply_to_platform_message_id: String::new(),
		};

		if let Some((new_pane, split)) = self.panes.split(axis, target_pane, new_state) {
			self.panes.resize(split, ratio);
			if swap {
				self.panes.swap(target_pane, new_pane);
			}
			self.focused_pane = target_pane;
		}
	}

	pub(crate) fn split_linear(&mut self) {
		let axis = pane_grid::Axis::Vertical;
		let ratio = 0.5;

		let inherited_join = self
			.focused_tab_id()
			.and_then(|tid| self.state.tabs.get(&tid))
			.and_then(|tab| match &tab.target {
				TabTarget::Room(room) => Some(format!("{}:{}", room.platform.as_str(), room.room_id.as_str())),
				_ => None,
			})
			.unwrap_or_default();

		let new_state = crate::app::PaneState {
			tab_id: self.focused_tab_id(),
			composer: String::new(),
			join_raw: inherited_join,
			reply_to_server_message_id: String::new(),
			reply_to_platform_message_id: String::new(),
		};

		let target = self.focused_pane;
		if let Some((new_pane, split)) = self.panes.split(axis, target, new_state) {
			self.panes.resize(split, ratio);
			self.focused_pane = new_pane;
		}
	}
}
