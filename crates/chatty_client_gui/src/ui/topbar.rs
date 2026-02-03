#![forbid(unsafe_code)]

use iced::widget::{button, container, row, rule, space, svg, text};
use iced::{Alignment, Background, Border, Element, Length, Shadow};
use rust_i18n::t;

use crate::app::message::Message;
use crate::app::model::Chatty;
use crate::app::state::ConnectionStatus;
use crate::app::types::Page;
use crate::assets::svg_handle;
use crate::theme;

pub fn view(app: &Chatty, palette: theme::Palette) -> Element<'_, Message> {
	let icon_button = |icon: &'static str, msg: Message| {
		let text_color = palette.text.clone();
		button(svg(svg_handle(icon)).width(16).height(16).style(move |_, _| svg::Style {
			color: Some(text_color.clone()),
		}))
		.on_press(msg)
	};

	let conn_button = match &app.state.connection {
		ConnectionStatus::Disconnected { .. } => button(
			row![
				svg(svg_handle("connect.svg"))
					.width(16)
					.height(16)
					.style(move |_, _| svg::Style {
						color: Some(palette.text),
					}),
				text(t!("settings.connect"))
			]
			.spacing(4),
		)
		.on_press(Message::Net(crate::app::message::NetMessage::ConnectPressed)),
		ConnectionStatus::Connecting => button(
			row![
				svg(svg_handle("spinner.svg"))
					.width(16)
					.height(16)
					.style(move |_, _| svg::Style {
						color: Some(palette.text),
					}),
				text(t!("cancel_label"))
			]
			.spacing(4),
		)
		.on_press(Message::Net(crate::app::message::NetMessage::DisconnectPressed)),
		ConnectionStatus::Reconnecting { .. } => button(
			row![
				svg(svg_handle("refresh.svg"))
					.width(16)
					.height(16)
					.style(move |_, _| svg::Style {
						color: Some(palette.text),
					}),
				text(t!("settings.reconnect"))
			]
			.spacing(4),
		)
		.on_press(Message::Net(crate::app::message::NetMessage::ConnectPressed)),
		ConnectionStatus::Connected { .. } => button(
			row![
				svg(svg_handle("disconnect.svg"))
					.width(16)
					.height(16)
					.style(move |_, _| svg::Style {
						color: Some(palette.text),
					}),
				text(t!("settings.disconnect"))
			]
			.spacing(4),
		)
		.on_press(Message::Net(crate::app::message::NetMessage::DisconnectPressed)),
	};

	let insert_chip = if app.state.ui.vim.insert_mode {
		container(text(t!("topbar.insert")).color(palette.text))
			.padding([2, 8])
			.style(move |_theme| container::Style {
				text_color: Some(palette.text),
				background: Some(Background::Color(palette.accent_blue)),
				border: Border {
					color: palette.accent_blue,
					width: 1.0,
					radius: 999.0.into(),
				},
				shadow: Shadow::default(),
				snap: false,
			})
	} else {
		container(row![]).height(0)
	};

	let mut right = row![].spacing(10).align_y(Alignment::Center);
	if app.state.ui.page != Page::Main {
		right = right.push(icon_button("chevron-left.svg", Message::Navigate(Page::Main)));
	}

	right = right
		.push(icon_button(
			"split.svg",
			Message::Layout(crate::app::message::LayoutMessage::SplitPressed),
		))
		.push(icon_button(
			"close.svg",
			Message::Layout(crate::app::message::LayoutMessage::CloseFocused),
		))
		.push(icon_button("users.svg", Message::Navigate(Page::Users)))
		.push(icon_button("settings.svg", Message::Navigate(Page::Settings)))
		.push(rule::vertical(1))
		.push(conn_button);

	let left = row![
		svg(svg_handle("logo.svg")).width(16).height(16),
		text(t!("app.name")).color(palette.text_dim),
		rule::vertical(1),
		insert_chip,
	]
	.spacing(10)
	.align_y(Alignment::Center);

	container(row![left, space::horizontal(), right].spacing(10).align_y(Alignment::Center))
		.width(Length::Fill)
		.height(32)
		.padding([0, 12])
		.style(move |_theme| container::Style {
			text_color: Some(palette.text),
			background: Some(Background::Color(palette.panel_bg_2)),
			border: Border {
				color: palette.border,
				width: 1.0,
				radius: 0.0.into(),
			},
			shadow: Shadow::default(),
			snap: false,
		})
		.into()
}
