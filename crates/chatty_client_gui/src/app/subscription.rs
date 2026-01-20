#![forbid(unsafe_code)]

use chatty_client_ui::settings::ShortcutKey;
use iced::Subscription;
use iced::keyboard;

use crate::app::{Chatty, Message};

impl Chatty {
	pub fn subscription(&self) -> Subscription<Message> {
		iced::event::listen_with(|event, _status, _id| match event {
			iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
				Some(Message::ModifiersChanged(modifiers))
			}
			iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => match key {
				keyboard::Key::Named(named) => Some(Message::NamedKeyPressed(named)),
				keyboard::Key::Character(c) => {
					let s = c.to_ascii_lowercase();
					let ch = s.chars().next().unwrap_or('\0');
					Some(Message::CharPressed(ch, modifiers))
				}
				_ => None,
			},
			iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
				Some(Message::CursorMoved(position.x, position.y))
			}
			iced::Event::Window(iced::window::Event::Resized(size)) => Some(Message::WindowResized(size.width, size.height)),
			_ => None,
		})
	}
}

pub fn shortcut_match(modifiers: keyboard::Modifiers, shortcut: ShortcutKey) -> bool {
	match shortcut {
		ShortcutKey::Alt => modifiers.alt(),
		ShortcutKey::Control => modifiers.control(),
		ShortcutKey::Shift => modifiers.shift(),
		ShortcutKey::Logo => modifiers.logo(),
		ShortcutKey::Always => true,
		ShortcutKey::None => false,
	}
}
