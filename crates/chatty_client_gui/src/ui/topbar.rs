#![forbid(unsafe_code)]

use chatty_client_ui::app_state::ConnectionStatus;
use iced::widget::{button, container, row, rule, space, svg, text};
use iced::{Alignment, Background, Border, Element, Length, Shadow};
use rust_i18n::t;

use crate::app::{Chatty, Message, Page};
use crate::assets::svg_handle;
use crate::theme;

pub fn view(app: &Chatty, palette: theme::Palette) -> Element<'_, Message> {
	let status_text = match &app.state.connection {
		ConnectionStatus::Disconnected { .. } => t!("status.disconnected").to_string(),
		ConnectionStatus::Connecting => t!("status.connecting").to_string(),
		ConnectionStatus::Reconnecting {
			attempt,
			next_retry_in_ms,
		} => format!("{} (attempt {attempt}, {next_retry_in_ms}ms)", t!("status.reconnecting")),
		ConnectionStatus::Connected { server } => format!("{}: {server}", t!("status.connected")),
	};

	let icon_button = |icon: &'static str, msg: Message| button(svg(svg_handle(icon)).width(16).height(16)).on_press(msg);

	let conn_button = match &app.state.connection {
		ConnectionStatus::Disconnected { .. } => button(text(t!("settings.connect"))).on_press(Message::ConnectPressed),
		ConnectionStatus::Connecting => button(text(t!("cancel_label"))).on_press(Message::DisconnectPressed),
		ConnectionStatus::Reconnecting { .. } => button(text(t!("settings.reconnect"))).on_press(Message::ConnectPressed),
		ConnectionStatus::Connected { .. } => button(text(t!("settings.disconnect"))).on_press(Message::DisconnectPressed),
	};

	let status_chip = container(text(status_text).color(palette.text_dim))
		.padding([2, 8])
		.style(move |_theme| container::Style {
			text_color: Some(palette.text_dim),
			background: Some(Background::Color(palette.app_bg)),
			border: Border {
				color: palette.border,
				width: 1.0,
				radius: 999.0.into(),
			},
			shadow: Shadow::default(),
			snap: false,
		});

	let insert_chip = if app.insert_mode {
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
	if app.page != Page::Main {
		right = right.push(icon_button("chevron-left.svg", Message::Navigate(Page::Main)));
	}
	right = right
		.push(icon_button("split.svg", Message::SplitPressed))
		.push(icon_button("close.svg", Message::CloseFocused))
		.push(icon_button("users.svg", Message::Navigate(Page::Users)))
		.push(icon_button("settings.svg", Message::Navigate(Page::Settings)))
		.push(rule::vertical(1))
		.push(conn_button);

	let left = row![
		svg(svg_handle("logo.svg")).width(16).height(16),
		text(t!("app.name")).color(palette.text_dim),
		rule::vertical(1),
		status_chip,
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

pub fn toast_bar(app: &Chatty, palette: theme::Palette) -> Element<'_, Message> {
	if let Some(msg) = app.toast.as_ref() {
		return container(
			row![
				text(msg.clone()).color(palette.system_text).width(Length::Fill),
				button(svg(svg_handle("close.svg")).width(14).height(14)).on_press(Message::DismissToast),
			]
			.spacing(10)
			.align_y(Alignment::Center),
		)
		.width(Length::Fill)
		.padding([6, 12])
		.style(move |_theme| container::Style {
			text_color: Some(palette.system_text),
			background: Some(Background::Color(palette.panel_bg_2)),
			border: Border {
				color: palette.border,
				width: 1.0,
				radius: 0.0.into(),
			},
			shadow: Shadow::default(),
			snap: false,
		})
		.into();
	}

	container(row![]).height(0).padding(0).into()
}
