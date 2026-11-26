use std::{collections::VecDeque, sync::Arc};

use crate::{EditorError, identity::Identities};

const MAX_STEPS: usize = 128;
const MAX_HISTORY_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug)]
struct ByteChange {
	start: usize,
	before: Box<[u8]>,
	after: Box<[u8]>,
}

impl ByteChange {
	fn between(before: &[u8], after: &[u8]) -> Self {
		let start = before.iter().zip(after).take_while(|(a, b)| a == b).count();
		let old_tail = &before[start..];
		let new_tail = &after[start..];
		let suffix = old_tail
			.iter()
			.rev()
			.zip(new_tail.iter().rev())
			.take_while(|(a, b)| a == b)
			.count();
		Self {
			start,
			before: old_tail[..old_tail.len() - suffix].into(),
			after: new_tail[..new_tail.len() - suffix].into(),
		}
	}

	fn apply(&self, current: &[u8], direction: Direction) -> Result<Vec<u8>, EditorError> {
		let (old, new) = match direction {
			Direction::Undo => (&self.after, &self.before),
			Direction::Redo => (&self.before, &self.after),
		};
		let end = self
			.start
			.checked_add(old.len())
			.ok_or(EditorError::InconsistentHistory)?;
		if current.get(self.start..end) != Some(old) {
			return Err(EditorError::InconsistentHistory);
		}
		let mut updated = Vec::with_capacity(current.len() - old.len() + new.len());
		updated.extend_from_slice(&current[..self.start]);
		updated.extend_from_slice(new);
		updated.extend_from_slice(&current[end..]);
		Ok(updated)
	}
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Direction {
	Undo,
	Redo,
}

#[derive(Debug)]
struct Step {
	delta: ByteChange,
	before: Arc<Identities>,
	after: Arc<Identities>,
}

impl Step {
	fn byte_cost(&self) -> usize {
		let identity_cost = if Arc::ptr_eq(&self.before, &self.after) {
			0
		} else {
			self.before.allocation_bytes() + self.after.allocation_bytes()
		};
		self.delta.before.len() + self.delta.after.len() + identity_cost
	}
}

#[derive(Debug, Default)]
pub(crate) struct History {
	undo: VecDeque<Step>,
	redo: Vec<Step>,
	bytes: usize,
}

impl History {
	pub(crate) fn record(
		&mut self,
		before: &[u8],
		after: &[u8],
		old_ids: &Arc<Identities>,
		new_ids: &Arc<Identities>,
	) {
		self.bytes -= self
			.redo
			.drain(..)
			.map(|entry| entry.byte_cost())
			.sum::<usize>();
		let step = Step {
			delta: ByteChange::between(before, after),
			before: old_ids.clone(),
			after: new_ids.clone(),
		};
		let cost = step.byte_cost();
		while self.undo.len() >= MAX_STEPS || self.bytes.saturating_add(cost) > MAX_HISTORY_BYTES {
			let Some(expired) = self.undo.pop_front() else {
				return;
			};
			self.bytes -= expired.byte_cost();
		}
		self.undo.push_back(step);
		self.bytes += cost;
	}

	pub(crate) fn can_undo(&self) -> bool {
		!self.undo.is_empty()
	}
	pub(crate) fn can_redo(&self) -> bool {
		!self.redo.is_empty()
	}

	pub(crate) fn restore(
		&self,
		current: &[u8],
		direction: Direction,
	) -> Result<(Vec<u8>, Arc<Identities>), EditorError> {
		let step = match direction {
			Direction::Undo => self.undo.back(),
			Direction::Redo => self.redo.last(),
		}
		.ok_or(EditorError::NoHistory)?;
		let identities = match direction {
			Direction::Undo => step.before.clone(),
			Direction::Redo => step.after.clone(),
		};
		Ok((step.delta.apply(current, direction)?, identities))
	}

	pub(crate) fn advance(&mut self, direction: Direction) {
		match direction {
			Direction::Undo => {
				if let Some(step) = self.undo.pop_back() {
					self.redo.push(step)
				}
			}
			Direction::Redo => {
				if let Some(step) = self.redo.pop() {
					self.undo.push_back(step)
				}
			}
		}
	}
}
