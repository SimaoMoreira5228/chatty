#![forbid(unsafe_code)]

use iced::widget::svg;
use rust_embed::RustEmbed;
use tracing::warn;

#[derive(RustEmbed)]
#[folder = "assets"]
#[include = "**/*"]
struct Assets;

pub fn svg_handle(path: &str) -> svg::Handle {
	if let Some(f) = Assets::get(path) {
		return svg::Handle::from_memory(f.data.into_owned());
	}

	let normalized = path.trim_start_matches('/');
	if normalized != path {
		if let Some(f) = Assets::get(normalized) {
			return svg::Handle::from_memory(f.data.into_owned());
		}
	}

	let prefixed = format!("assets/{normalized}");
	if let Some(f) = Assets::get(prefixed.as_str()) {
		return svg::Handle::from_memory(f.data.into_owned());
	}

	if let Ok(exe_path) = std::env::current_exe()
		&& let Some(exe_dir) = exe_path.parent()
	{
		let sibling_path = exe_dir.join("assets").join(normalized);
		if let Ok(bytes) = std::fs::read(&sibling_path) {
			return svg::Handle::from_memory(bytes);
		}
	}

	let fallback_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("assets")
		.join(normalized);
	match std::fs::read(&fallback_path) {
		Ok(bytes) => svg::Handle::from_memory(bytes),
		Err(err) => {
			warn!(path = %fallback_path.display(), error = %err, "missing embedded asset");
			svg::Handle::from_memory(Vec::new())
		}
	}
}
