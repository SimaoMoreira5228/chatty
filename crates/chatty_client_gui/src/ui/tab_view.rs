use iced::Length;
use iced::widget::pane_grid::PaneGrid;

use crate::app::features::chat::ChatPane;
use crate::app::features::tabs::TabModel;
use crate::app::message::Message;
use crate::app::model::Chatty;
use crate::app::view_models::build_chat_pane_view_model;
use crate::theme;

pub fn view<'a>(app: &'a Chatty, tab: &'a TabModel, palette: theme::Palette) -> iced::Element<'a, Message> {
	let view_pane = |pane, state: &'a ChatPane, _| {
		let vm = build_chat_pane_view_model(app, tab, pane, state, palette);
		state.view(vm, &app.assets, palette)
	};

	let mut grid = PaneGrid::new(&tab.panes, view_pane)
		.width(Length::Fill)
		.height(Length::Fill)
		.spacing(8)
		.on_click(|pane| Message::Layout(crate::app::message::LayoutMessage::PaneClicked(pane)))
		.on_resize(10, |ev| Message::Layout(crate::app::message::LayoutMessage::PaneResized(ev)));

	if app.pane_drag_enabled() {
		grid = grid.on_drag(|ev| Message::Layout(crate::app::message::LayoutMessage::PaneDragged(ev)));
	}

	grid.into()
}
