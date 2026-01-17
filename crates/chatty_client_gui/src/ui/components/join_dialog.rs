#![forbid(unsafe_code)]

use std::sync::Arc;

use gpui::prelude::*;
use gpui::{Entity, Window, div};
use gpui_component::WindowExt;
use gpui_component::dialog::DialogButtonProps;
use gpui_component::input::{Input, InputState};
use gpui_component::select::{SelectEvent, SelectState};

use crate::ui::theme;

pub enum JoinSubmission {
	Single(String),
	Group(String),
}

pub fn open_join_dialog<T, F>(
	view: Entity<T>,
	window: &mut Window,
	cx: &mut Context<T>,
	groups: Vec<(u64, String)>,
	initial_value: Option<String>,
	on_submit: F,
) where
	T: 'static,
	F: Fn(&mut T, &mut Context<T>, JoinSubmission) -> bool + 'static,
{
	let on_submit = Arc::new(on_submit);

	let input = cx.new(|cx| {
		let mut state = InputState::new(window, cx)
			.placeholder("twitch:channel or group:<id>")
			.validate(|s, _| !s.trim().is_empty());

		if let Some(val) = initial_value {
			state = state.default_value(val);
		}

		state
	});

	let type_select = cx.new(|cx| {
		let mut state: SelectState<Vec<gpui::SharedString>> =
			SelectState::new(vec!["Single Channel".into(), "Group".into()], None, window, cx);
		state.set_selected_value(&"Single Channel".into(), window, cx);
		state
	});

	let t = theme::theme();

	{
		let input = input.clone();
		let type_select = type_select.clone();
		cx.subscribe_in(&type_select, window, move |_, _type_select, event, window, cx| {
			if let SelectEvent::Confirm(Some(value)) = event {
				input.update(cx, |input, cx| {
					if value == "Group" {
						input.set_placeholder("twitch:c1, kick:c2, ...", window, cx);
					} else {
						input.set_placeholder("twitch:channel or group:<id>", window, cx);
					}
				});
			}
		})
		.detach();
	}

	window.open_dialog(cx, move |dialog, _window, _cx| {
		dialog
			.title("Join")
			.confirm()
			.button_props(DialogButtonProps::default().ok_text("Join").cancel_text("Cancel"))
			.child({
				let groups = groups.clone();
				let type_select = type_select.clone();
				div()
					.flex()
					.flex_col()
					.gap_2()
					.child("Create a new tab")
					.child(
						div()
							.flex()
							.flex_col()
							.gap_1()
							.child(div().text_sm().text_color(t.text_dim).child("Tab Type"))
							.child(gpui_component::select::Select::new(&type_select)),
					)
					.child(Input::new(&input))
					.child(
						div().text_sm().text_color(t.text_dim).child(
							"Format: twitch:channel or room:twitch/channel. Leave platform blank to use the default.",
						),
					)
					.child(
						div()
							.text_sm()
							.text_color(t.text_dim)
							.child("You can also enter group:<id> to open a group tab. Existing groups:"),
					)
					.child({
						let mut container = div().flex().flex_col().gap_1();
						for (_id, name) in groups.into_iter() {
							container = container.child(div().text_sm().text_color(t.text_dim).child(name));
						}
						container
					})
			})
			.on_ok({
				let input = input.clone();
				let view = view.clone();
				let on_submit = on_submit.clone();
				let type_select = type_select.clone();
				move |_, _window, cx| {
					let raw = input.read(cx).value().to_string();
					let is_group = matches!(type_select.read(cx).selected_value(), Some(s) if s.as_ref() == "Group");
					let submission = if is_group {
						JoinSubmission::Group(raw)
					} else {
						JoinSubmission::Single(raw)
					};
					view.update(cx, |this, cx| (on_submit)(this, cx, submission))
				}
			})
	})
}
