#![forbid(unsafe_code)]

use std::time::Duration;

use iced::{Subscription, keyboard};

use crate::app::{Chatty, Message};
use crate::settings::ShortcutKey;

impl Chatty {
	pub fn subscription(&self) -> Subscription<Message> {
		let input = iced::event::listen_with(|event, _status, id| match event {
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
			iced::Event::Mouse(iced::mouse::Event::WheelScrolled { .. }) => Some(Message::UserScrolled),
			iced::Event::Window(event) => match event {
				iced::window::Event::CloseRequested => Some(Message::WindowClosed(id)),
				iced::window::Event::Resized(size) => {
					Some(Message::WindowResized(id, size.width as u32, size.height as u32))
				}
				iced::window::Event::Moved(point) => Some(Message::WindowMoved(id, point.x as i32, point.y as i32)),
				_ => None,
			},
			_ => None,
		});

		let anim = iced::time::every(Duration::from_millis(50)).map(Message::AnimationTick);
		Subscription::batch(vec![input, anim])
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
