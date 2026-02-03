use iced::Element;

use crate::app::features::tabs::TabId;
use crate::app::message::Message;
use crate::app::model::Chatty;
use crate::theme;

pub fn view(app: &Chatty, tab_id: TabId) -> Element<'_, Message> {
	let palette = theme::palette(&app.state.gui_settings().theme, &app.state.custom_themes);

	if let Some(tab) = app.state.tabs.get(&tab_id) {
		crate::ui::tab_view::view(app, tab, palette)
	} else {
		iced::widget::text("Tab not found").into()
	}
}
