#![forbid(unsafe_code)]

use iced::widget::pane_grid;

use crate::app::Chatty;
use crate::ui::components::chat_pane::ChatPane;

impl Chatty {
	pub(crate) fn split_spiral(&mut self) {
		let dir = self.state.ui.spiral_dir % 4;
		self.state.ui.spiral_dir = (self.state.ui.spiral_dir + 1) % 4;

		let tab_id = self.selected_tab_id();
		let Some(tab) = self.selected_tab_mut() else {
			return;
		};
		let (axis, ratio, swap) = match dir {
			0 => (pane_grid::Axis::Vertical, 0.618, false),
			1 => (pane_grid::Axis::Horizontal, 0.618, false),
			2 => (pane_grid::Axis::Vertical, 0.618, true),
			3 => (pane_grid::Axis::Horizontal, 0.618, true),
			_ => unreachable!(),
		};

		if tab.panes.iter().next().is_none() {
			return;
		}

		let state = ChatPane::new(tab_id);

		let Some(old_pane) = tab.focused_pane.or_else(|| tab.panes.iter().next().map(|(id, _)| *id)) else {
			return;
		};

		if let Some((new_pane, split)) = tab.panes.split(axis, old_pane, state) {
			tab.panes.resize(split, ratio);
			if swap {
				tab.panes.swap(old_pane, new_pane);
			}

			tab.focused_pane = Some(old_pane);
		}
	}

	pub(crate) fn split_masonry(&mut self) {
		use iced::Rectangle;

		let Some((width, height)) = self.state.ui.window_size else {
			return;
		};
		let bounds = iced::Size::new(width, height);

		let (target_pane, rect, target_tab_id) = {
			let Some(tab) = self.selected_tab() else {
				return;
			};

			let regions = tab.panes.layout().pane_regions(8.0, 50.0, bounds);

			let mut best: Option<(pane_grid::Pane, Rectangle, f32)> = None;
			for (pane, rect) in regions {
				let area = rect.width * rect.height;
				best = match best {
					None => Some((pane, rect, area)),
					Some((bp, br, ba)) => {
						let is_focused = Some(pane) == tab.focused_pane;
						if area > ba || (area == ba && is_focused) {
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

			let target_tab_id = tab.panes.get(target_pane).and_then(|ps| ps.tab_id);
			(target_pane, rect, target_tab_id)
		};

		let inherited_join = target_tab_id
			.and_then(|tid| self.state.tabs.get(&tid))
			.map(|t| {
				t.target
					.0
					.iter()
					.map(|r| format!("{}:{}", r.platform.as_str(), r.room_id.as_str()))
					.collect::<Vec<_>>()
					.join(", ")
			})
			.unwrap_or_default();

		let flip = self.state.ui.masonry_flip;
		self.state.ui.masonry_flip = !self.state.ui.masonry_flip;

		let axis = if rect.width >= rect.height {
			pane_grid::Axis::Vertical
		} else {
			pane_grid::Axis::Horizontal
		};

		let ratio = if flip { 0.5 } else { 0.618 };
		let swap = flip;

		let mut new_state = ChatPane::new(target_tab_id);
		new_state.join_raw = inherited_join;

		if let Some(tab) = self.selected_tab_mut()
			&& let Some((new_pane, split)) = tab.panes.split(axis, target_pane, new_state)
		{
			tab.panes.resize(split, ratio);
			if swap {
				tab.panes.swap(target_pane, new_pane);
			}
			tab.focused_pane = Some(target_pane);
		}
	}
}
