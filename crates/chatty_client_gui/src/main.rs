#![forbid(unsafe_code)]

mod app;
mod assets;
mod theme;
mod ui;

#[macro_use]
extern crate rust_i18n;

i18n!("locales", fallback = "en-US", minify_key = true);

fn main() -> iced::Result {
	rust_i18n::set_locale("en-US");
	app::run()
}
