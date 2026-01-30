use crate::app::InsertTarget;

#[derive(Debug, Clone, Default)]
pub struct VimState {
	pub insert_mode: bool,
	pub insert_target: Option<InsertTarget>,
}

impl VimState {
	pub fn enter_insert_mode(&mut self, target: InsertTarget) {
		self.insert_mode = true;
		self.insert_target = Some(target);
	}

	pub fn exit_insert_mode(&mut self) {
		self.insert_mode = false;
		self.insert_target = None;
	}
}
