#![forbid(unsafe_code)]

use iced::widget::svg;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "assets"]
#[include = "**/*.svg"]
struct Assets;

pub fn svg_handle(path: &str) -> svg::Handle {
	Assets::get(path)
		.map(|f| svg::Handle::from_memory(f.data.into_owned()))
		.unwrap_or_else(|| {
			debug_assert!(false, "missing embedded asset: {}", path);
			svg::Handle::from_memory(Vec::new())
		})
}
