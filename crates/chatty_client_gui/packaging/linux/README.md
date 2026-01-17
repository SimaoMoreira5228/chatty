# Linux packaging

Install the desktop entry and icon:

- Copy `chatty.desktop` to `~/.local/share/applications/` (or `/usr/share/applications/`).
- Copy `chatty.png` to `~/.local/share/icons/hicolor/256x256/apps/` (or a system icon path) and name it `chatty.png`.

The release artifacts include this desktop file. The icon file will be included when `assets/app-icons/chatty.png` is present.
