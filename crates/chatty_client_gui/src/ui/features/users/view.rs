#![forbid(unsafe_code)]

use iced::widget::{column, container, row, rule, scrollable, svg, text, text_input};
use iced::{Alignment, Background, Border, Element, Length, Shadow};
use rust_i18n::t;

use crate::app::features::users::{UsersView, UsersViewMessage};
use crate::app::message::Message;
use crate::app::model::Chatty;
use crate::assets::svg_handle;
use crate::theme;

impl UsersView {
	pub fn view<'a>(&'a self, app: &'a Chatty, palette: theme::Palette) -> Element<'a, Message> {
		let mut title = t!("users.title").to_string();
		let mut rows = column![].spacing(6);

		let filter = self.filter.trim().to_ascii_lowercase();
		if let Some(tab_id) = app.focused_tab_id()
			&& let Some(tab) = app.state.tabs.get(&tab_id)
		{
			title = format!("Users • {}", tab.title);
			let mut users: Vec<(String, usize)> = tab.user_counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
			users.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

			let mut any = false;
			for (user, count) in users {
				if !filter.is_empty() && !user.to_ascii_lowercase().contains(&filter) {
					continue;
				}
				any = true;
				let count_label = if count == 1 {
					t!("users.messages_one").to_string()
				} else {
					t!("users.messages_other").to_string()
				};

				rows = rows.push(
					container(
						row![
							text(user).color(palette.text).width(Length::Fill),
							text(format!("{count} {count_label}")).color(palette.text_dim),
						]
						.align_y(Alignment::Center)
						.spacing(12),
					)
					.padding(8)
					.style(move |_theme| container::Style {
						text_color: Some(palette.text),
						background: Some(Background::Color(palette.panel_bg_2)),
						border: Border {
							color: palette.border,
							width: 1.0,
							radius: 8.0.into(),
						},
						shadow: Shadow::default(),
						snap: false,
					}),
				);
			}

			if !any {
				rows = rows.push(text(t!("users.no_users_yet")).color(palette.text_dim));
			}
		} else {
			rows = rows.push(text(t!("users.no_active_tab")).color(palette.text_dim));
		}

		let header = row![
			svg(svg_handle("users.svg")).width(18).height(18),
			text(title).color(palette.text),
		]
		.spacing(10)
		.align_y(Alignment::Center);

		let filter_placeholder = t!("users.filter_placeholder").to_string();
		let filter_row = row![
			text_input(&filter_placeholder, &self.filter)
				.on_input(|v| Message::UsersViewMessage(UsersViewMessage::FilterChanged(v)))
				.width(Length::Fill),
		];

		let body = column![header, rule::horizontal(1), filter_row, scrollable(rows).height(Length::Fill)]
			.spacing(10)
			.padding(12);

		container(body)
			.width(Length::Fill)
			.height(Length::Fill)
			.style(move |_theme| container::Style {
				text_color: Some(palette.text),
				background: Some(Background::Color(palette.panel_bg)),
				border: Border {
					color: palette.border,
					width: 1.0,
					radius: 10.0.into(),
				},
				shadow: Shadow::default(),
				snap: false,
			})
			.padding(12)
			.into()
	}
}
