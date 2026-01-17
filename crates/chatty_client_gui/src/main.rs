#![forbid(unsafe_code)]

#[cfg(feature = "gpui")]
use std::borrow::Cow;

#[cfg(feature = "gpui")]
use gpui::AssetSource;

#[cfg(feature = "gpui")]
use tracing::info;

#[cfg(feature = "gpui")]
use gpui::{App, Application, SharedString, actions};

#[cfg(feature = "gpui")]
#[derive(rust_embed::RustEmbed)]
#[folder = "assets"]
#[include = "**/*.svg"]
struct Assets;

#[cfg(feature = "gpui")]
impl AssetSource for Assets {
	fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
		if path.is_empty() {
			return Ok(None);
		}

		Assets::get(path)
			.map(|f| Some(f.data))
			.ok_or_else(|| anyhow::anyhow!("could not find asset at path \"{path}\""))
	}

	fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
		Ok(Assets::iter().filter_map(|p| p.starts_with(path).then(|| p.into())).collect())
	}
}

#[cfg(feature = "gpui")]
mod ui;

#[cfg(feature = "gpui")]
actions!(chatty, [Quit]);

#[cfg(feature = "gpui")]
fn init_tracing() {
	let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info,chatty_client_gpui=debug".to_string());
	tracing_subscriber::fmt().with_env_filter(filter).with_target(false).init();
}

#[cfg(feature = "gpui")]
fn main() {
	init_tracing();
	info!("starting chatty_client_gpui");

	Application::new().with_assets(Assets).run(|cx: &mut App| {
		cx.on_action(|_: &Quit, cx: &mut App| cx.quit());

		gpui_component::init(cx);
		ui::theme::sync_component_theme(cx, ui::settings::theme_kind());
		ui::open_all_windows(cx).expect("open windows");

		cx.activate(true);
	});
}

#[cfg(not(feature = "gpui"))]
fn main() {
	eprintln!(
		"chatty_client_gpui: GPUI feature not enabled.\n\
Build with:\n\
    cargo run -p chatty_client_gpui --features gpui\n\
or (dev helpers):\n\
    cargo run -p chatty_client_gpui --features gpui-dev\n"
	);
}
