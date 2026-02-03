use iced::Element;
use iced::widget::{column, row, rule, scrollable, text};
use rust_i18n::t;

use crate::app::message::Message;
use crate::app::model::Chatty;
use crate::theme;

pub fn view(app: &Chatty, palette: theme::Palette) -> Element<'_, Message> {
	let status_text = match &app.state.connection {
		crate::app::state::ConnectionStatus::Disconnected { .. } => t!("status.disconnected").to_string(),
		crate::app::state::ConnectionStatus::Connecting => t!("status.connecting").to_string(),
		crate::app::state::ConnectionStatus::Reconnecting {
			attempt,
			next_retry_in_ms,
		} => format!("{} (attempt {attempt}, {next_retry_in_ms}ms)", t!("status.reconnecting")),
		crate::app::state::ConnectionStatus::Connected { server } => {
			format!("{}: {server}", t!("status.connected"))
		}
	};

	let last_error = app
		.state
		.ui
		.active_overlay
		.as_ref()
		.and_then(|o| {
			if let crate::app::features::overlays::ActiveOverlay::Layout(m) = o {
				Some(m)
			} else {
				None
			}
		})
		.and_then(|m| {
			if let crate::app::features::overlays::LayoutModalKind::Error { message } = &m.kind {
				Some(message.clone())
			} else {
				None
			}
		})
		.or_else(|| match &app.state.connection {
			crate::app::state::ConnectionStatus::Disconnected { reason } => reason.clone(),
			_ => None,
		});

	let mut notifs = column![].spacing(4);
	for n in app.state.notifications.iter().rev().take(20).rev() {
		notifs = notifs.push(text(format!("{:?}: {}", n.kind, n.message)).color(palette.text_dim));
	}

	scrollable(
		column![
			text(t!("settings.diagnostics")).color(palette.text),
			rule::horizontal(1),
			row![
				text(format!("{} ", t!("settings.endpoint"))).color(palette.text_dim),
				text(app.state.ui.server_endpoint_quic.clone()).color(palette.text)
			],
			row![
				text(format!("{} ", t!("settings.connection_status"))).color(palette.text_dim),
				text(status_text).color(palette.text)
			],
			row![
				text(format!("{} ", t!("settings.last_error"))).color(palette.text_dim),
				text(last_error.unwrap_or_else(|| "(none)".to_string())).color(palette.text_dim)
			],
			rule::horizontal(1),
			text(t!("settings.recent_notifications")).color(palette.text_dim),
			notifs,
		]
		.spacing(12)
		.padding(12),
	)
	.into()
}
