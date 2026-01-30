#![forbid(unsafe_code)]

use iced::Task;

use crate::app::{Chatty, Message};

pub fn run() -> iced::Result {
	iced::daemon(Chatty::new, update, Chatty::view)
		.title(Chatty::title)
		.theme(Chatty::theme)
		.subscription(Chatty::subscription)
		.run()
}

fn update(app: &mut Chatty, message: Message) -> Task<Message> {
	app.update(message)
}
