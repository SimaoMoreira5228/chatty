#![forbid(unsafe_code)]

use iced::widget::{button, column, container, row, svg, text};
use iced::{Alignment, Background, Border, Element, Length};
use rust_i18n::t;

use crate::app::{Chatty, Message};
use crate::assets::svg_handle;
use crate::theme;

pub fn view(app: &Chatty, palette: theme::Palette) -> Element<'_, Message> {
	let Some(selected_tab) = app.selected_tab() else {
		return container(text(t!("main.info_join_begin")).size(20))
			.width(Length::Fill)
			.height(Length::Fill)
			.center_x(Length::Fill)
			.center_y(Length::Fill)
			.into();
	};

	let panes = crate::ui::tab_view::view(app, selected_tab, palette);

	let tab_bar = row(app.state.tab_order.iter().map(|tid| {
		let tab = app.state.tabs.get(tid).unwrap();
		let is_selected = Some(*tid) == app.state.selected_tab_id;

		let tab_color = if is_selected { palette.text } else { palette.text_dim };
		let bg = if is_selected { palette.panel_bg_2 } else { palette.chat_bg };

		let mut c = row![text(&tab.title).color(tab_color)].align_y(Alignment::Center).spacing(8);

		if !tab.pinned {
			c = c
				.push(
					button(svg(svg_handle("close.svg")).width(12).height(12))
						.on_press(Message::CloseTabPressed(*tid))
						.style(move |_theme, _status| button::Style {
							background: None,
							text_color: tab_color,
							..Default::default()
						}),
				)
				.push(
					button(svg(svg_handle("open-in-new.svg")).width(12).height(12))
						.on_press(Message::PopTab(*tid))
						.style(move |_theme, _status| button::Style {
							background: None,
							text_color: tab_color,
							..Default::default()
						}),
				);
		}

		button(c)
			.on_press(Message::TabSelected(*tid))
			.padding([6, 12])
			.style(move |_theme, _status| button::Style {
				background: Some(Background::Color(bg)),
				text_color: tab_color,
				border: Border {
					color: if is_selected { palette.accent_blue } else { palette.border },
					width: 1.0,
					radius: iced::border::Radius {
						top_left: 4.0,
						top_right: 4.0,
						bottom_right: 0.0,
						bottom_left: 0.0,
					},
				},
				..Default::default()
			})
			.into()
	}))
	.spacing(4)
	.align_y(Alignment::End);

	let add_tab_btn = button(svg(svg_handle("plus.svg")).width(16).height(16))
		.on_press(Message::AddTabPressed)
		.padding(8);

	let top_bar = row![tab_bar, add_tab_btn].spacing(8).align_y(Alignment::Center);

	column![top_bar, panes]
		.width(Length::Fill)
		.height(Length::Fill)
		.padding(8)
		.into()
}
