#![forbid(unsafe_code)]

use iced::widget::{button, column, container, row, svg, text};
use iced::{Alignment, Background, Border, Element, Length};
use rust_i18n::t;

use crate::app::features::tabs::TabId;
use crate::app::message::Message;
use crate::app::model::Chatty;
use crate::app::types::JoinTarget;
use crate::assets::svg_handle;
use crate::theme;

#[derive(Debug, Clone)]
struct TabBarItemViewModel {
	id: TabId,
	title: String,
	is_selected: bool,
	pinned: bool,
}

#[derive(Debug, Clone)]
struct TabBarViewModel {
	items: Vec<TabBarItemViewModel>,
}

#[derive(Debug, Clone)]
struct MainViewModel {
	selected_tab_id: Option<TabId>,
	tab_bar: TabBarViewModel,
}

pub fn view(app: &Chatty, palette: theme::Palette) -> Element<'_, Message> {
	let vm = build_main_view_model(app);
	let Some(selected_tab) = vm.selected_tab_id.and_then(|tid| app.state.tabs.get(&tid)) else {
		let content = column![
			text(t!("main.info_join_begin")).size(20).color(palette.text),
			button(
				row![
					svg(svg_handle("plus.svg"))
						.width(16)
						.height(16)
						.style(move |_, _| svg::Style {
							color: Some(palette.text),
						}),
					text(t!("main.join_button"))
				]
				.spacing(8)
			)
			.on_press(Message::OpenJoinModal(JoinTarget::NewTab))
		]
		.spacing(16)
		.align_x(Alignment::Center);

		return container(content)
			.width(Length::Fill)
			.height(Length::Fill)
			.center_x(Length::Fill)
			.center_y(Length::Fill)
			.into();
	};

	let panes = crate::ui::tab_view::view(app, selected_tab, palette);

	let tab_items = vm.tab_bar.items;
	let tab_bar = row(tab_items.into_iter().map(|item| {
		let tab_color = if item.is_selected { palette.text } else { palette.text_dim };
		let bg = if item.is_selected {
			palette.panel_bg_2
		} else {
			palette.chat_bg
		};

		let title = item.title;
		let mut c = row![text(title).color(tab_color)].align_y(Alignment::Center).spacing(8);

		if !item.pinned {
			let icon_style = move |_theme: &iced::Theme, _status| svg::Style {
				color: Some(palette.text),
			};

			c = c
				.push(
					button(svg(svg_handle("close.svg")).width(12).height(12).style(icon_style))
						.on_press(Message::CloseTabPressed(item.id)),
				)
				.push(
					button(svg(svg_handle("open-in-new.svg")).width(12).height(12).style(icon_style))
						.on_press(Message::PopTab(item.id)),
				);
		}

		button(c)
			.on_press(Message::TabSelected(item.id))
			.padding([6, 12])
			.style(move |_theme, _status| button::Style {
				background: Some(Background::Color(bg)),
				text_color: tab_color,
				border: Border {
					color: if item.is_selected {
						palette.accent_blue
					} else {
						palette.border
					},
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

	let add_tab_btn = button(
		svg(svg_handle("plus.svg"))
			.width(16)
			.height(16)
			.style(move |_, _| svg::Style {
				color: Some(palette.text),
			}),
	)
	.on_press(Message::AddTabPressed)
	.padding(8);

	let top_bar = row![tab_bar, add_tab_btn].spacing(8).align_y(Alignment::Center);

	column![top_bar, panes]
		.width(Length::Fill)
		.height(Length::Fill)
		.padding(8)
		.into()
}

fn build_main_view_model(app: &Chatty) -> MainViewModel {
	let items = app
		.state
		.tab_order
		.iter()
		.filter_map(|tid| app.state.tabs.get(tid).map(|tab| (tid, tab)))
		.map(|(tid, tab)| TabBarItemViewModel {
			id: *tid,
			title: tab.title.clone(),
			is_selected: Some(*tid) == app.state.selected_tab_id,
			pinned: tab.pinned,
		})
		.collect();

	MainViewModel {
		selected_tab_id: app.state.selected_tab_id,
		tab_bar: TabBarViewModel { items },
	}
}
