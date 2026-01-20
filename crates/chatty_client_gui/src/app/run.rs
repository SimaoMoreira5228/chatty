#![forbid(unsafe_code)]

use iced::Task;

use crate::app::{Chatty, Message};

pub fn run() -> iced::Result {
	iced::application(Chatty::new, update, Chatty::view)
		.title("Chatty")
		.theme(iced::Theme::Dark)
		.subscription(Chatty::subscription)
		.run()
}

fn update(app: &mut Chatty, message: Message) -> Task<Message> {
	app.update(message)
}
