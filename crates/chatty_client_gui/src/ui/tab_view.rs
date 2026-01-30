use iced::Length;
use iced::widget::pane_grid::PaneGrid;

use crate::app::state::TabModel;
use crate::app::{Chatty, Message};
use crate::theme;
use crate::ui::components::chat_pane::ChatPane;

pub fn view<'a>(app: &'a Chatty, tab: &'a TabModel, palette: theme::Palette) -> iced::Element<'a, Message> {
	let view_pane = |pane, state: &'a ChatPane, _| state.view(app, tab, pane, palette);

	let mut grid = PaneGrid::new(&tab.panes, view_pane)
		.width(Length::Fill)
		.height(Length::Fill)
		.spacing(8)
		.on_click(Message::PaneClicked)
		.on_resize(10, Message::PaneResized);

	if app.pane_drag_enabled() {
		grid = grid.on_drag(Message::PaneDragged);
	}

	grid.into()
}
