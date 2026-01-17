#[cfg(target_os = "windows")]
use std::path::Path;

fn main() {
	#[cfg(target_os = "windows")]
	{
		let icon_path = "assets/app-icons/chatty.ico";
		if Path::new(icon_path).exists() {
			let mut res = winres::WindowsResource::new();
			res.set_icon(icon_path);
			res.compile().expect("Failed to compile Windows resources");
		}
	}
}
