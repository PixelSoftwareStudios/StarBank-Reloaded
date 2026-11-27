use std::{
	borrow::Cow,
	collections::{BTreeMap, BTreeSet},
};

use crate::metadata::{
	MAX_BANK_NAME_BYTES, MAX_BANK_NAMES, MetadataError, display_text, normalized_bank_name,
};

const TITLE_HEADING: &str = "Name:";
const AUTHOR_HEADING: &str = "Author:";
const BANK_LOAD: &str = "BankLoad";
const MAX_VARIABLES: usize = 4096;
const MAX_IDENTIFIER_BYTES: usize = 256;

pub(crate) fn headings(source: &str) -> (Option<&str>, Option<&str>) {
	let mut lexer = Lexer { source, cursor: 0 };
	let mut title = None;
	let mut author = None;
	while title.is_none() || author.is_none() {
		let next = lexer.next_token(|start, text| {
			let line_prefix = source[..start]
				.rsplit(['\r', '\n'])
				.next()
				.unwrap_or_default();
			if !line_prefix.trim().is_empty() {
				return;
			}
			let text = text.trim_start();
			title = title.or_else(|| text.strip_prefix(TITLE_HEADING).and_then(display_text));
			author = author.or_else(|| text.strip_prefix(AUTHOR_HEADING).and_then(display_text));
		});
		if next.is_none() {
			break;
		}
	}
	(title, author)
}

pub(crate) fn bank_names(source: &str) -> Result<Vec<String>, MetadataError> {
	let mut banks = BTreeSet::new();
	let mut variables = BTreeMap::new();
	for window in windows(source) {
		let [
			Some(Token::Identifier(BANK_LOAD)),
			Some(Token::Symbol(b'(')),
			Some(argument),
			Some(Token::Symbol(b',' | b')')),
		] = window
		else {
			continue;
		};
		match argument {
			Token::String(value) => insert_bank(&mut banks, value)?,
			Token::Identifier(name) if name.len() <= MAX_IDENTIFIER_BYTES => {
				variables.entry(name).or_insert(Assignment::Unassigned);
				if variables.len() > MAX_VARIABLES {
					return Err(MetadataError::TooManyBankReferences);
				}
			}
			_ => {}
		}
	}
	if !variables.is_empty() {
		for window in windows(source) {
			let [
				Some(Token::Identifier(name)),
				Some(Token::Assignment(op)),
				value,
				terminator,
			] = window
			else {
				continue;
			};
			let Some(assigned) = variables.get_mut(name) else {
				continue;
			};
			// Treat any nonliteral or conflicting assignment as ambiguous. This is
			// intentionally a textual hint across scopes, never control-flow analysis.
			let literal = match (op, value, terminator) {
				(
					WriteOperator::Plain,
					Some(Token::String(value)),
					Some(Token::Symbol(b';' | b',' | b')')),
				) => unescape_literal(value).and_then(|value| normalized_bank_name(&value)),
				_ => None,
			};
			*assigned = match (std::mem::take(assigned), literal) {
				(Assignment::Unassigned, Some(value)) => Assignment::Literal(value),
				(Assignment::Literal(previous), Some(value)) if previous == value => {
					Assignment::Literal(previous)
				}
				_ => Assignment::Ambiguous,
			};
		}
		for assignment in variables.into_values() {
			if let Assignment::Literal(bank) = assignment {
				banks.insert(bank);
			}
		}
	}
	if banks.len() > MAX_BANK_NAMES {
		return Err(MetadataError::TooManyBankReferences);
	}
	Ok(banks.into_iter().collect())
}

fn insert_bank(banks: &mut BTreeSet<String>, value: &str) -> Result<(), MetadataError> {
	if let Some(bank) = unescape_literal(value).and_then(|value| normalized_bank_name(&value)) {
		banks.insert(bank);
		if banks.len() > MAX_BANK_NAMES {
			return Err(MetadataError::TooManyBankReferences);
		}
	}
	Ok(())
}

fn unescape_literal(value: &str) -> Option<Cow<'_, str>> {
	if value.len() > MAX_BANK_NAME_BYTES * 2 {
		return None;
	}
	if !value.contains('\\') {
		return Some(Cow::Borrowed(value));
	}
	let mut chars = value.chars();
	let mut decoded = String::new();
	while let Some(character) = chars.next() {
		if character == '\\' {
			let escaped = chars.next()?;
			if !matches!(escaped, '"' | '\\') {
				return None;
			}
			decoded.push(escaped);
		} else {
			decoded.push(character);
		}
	}
	Some(Cow::Owned(decoded))
}

#[derive(Default)]
enum Assignment {
	#[default]
	Unassigned,
	Literal(String),
	Ambiguous,
}

#[derive(Clone, Copy)]
enum WriteOperator {
	Plain,
	Compound,
}

#[derive(Clone, Copy)]
enum Token<'a> {
	Identifier(&'a str),
	String(&'a str),
	Symbol(u8),
	Assignment(WriteOperator),
	Other,
}

fn windows(source: &str) -> impl Iterator<Item = [Option<Token<'_>>; 4]> {
	let mut lexer = Lexer { source, cursor: 0 };
	let mut window = [lexer.next(), lexer.next(), lexer.next(), lexer.next()];
	std::iter::from_fn(move || {
		window[0]?;
		let current = window;
		window.rotate_left(1);
		window[3] = lexer.next();
		Some(current)
	})
}

struct Lexer<'a> {
	source: &'a str,
	cursor: usize,
}

impl<'a> Lexer<'a> {
	fn remaining(&self) -> &'a [u8] {
		&self.source.as_bytes()[self.cursor..]
	}

	fn skip_trivia(&mut self, mut on_line_comment: impl FnMut(usize, &'a str)) {
		loop {
			let input = self.remaining();
			if input.starts_with(b"//") {
				let end = self.cursor
					+ input
						.iter()
						.position(|byte| matches!(byte, b'\r' | b'\n'))
						.unwrap_or(input.len());
				on_line_comment(self.cursor, &self.source[self.cursor + 2..end]);
				self.cursor = end;
			} else if let Some(comment) = input.strip_prefix(b"/*") {
				let end = comment
					.windows(2)
					.position(|pair| pair == b"*/")
					.map_or(input.len(), |offset| offset + 4);
				self.cursor += end;
			} else if input.first().is_some_and(u8::is_ascii_whitespace) {
				self.cursor += 1;
			} else {
				break;
			}
		}
	}

	fn string(&mut self) -> Token<'a> {
		let start = self.cursor;
		let input = self.remaining();
		let mut cursor = 0;
		while let Some(&byte) = input.get(cursor) {
			match byte {
				b'"' => {
					self.cursor = start + cursor + 1;
					return Token::String(&self.source[start..start + cursor]);
				}
				b'\\' => cursor += 2,
				_ => cursor += 1,
			}
		}
		self.cursor = self.source.len();
		Token::Other
	}

	fn next_token(&mut self, on_line_comment: impl FnMut(usize, &'a str)) -> Option<Token<'a>> {
		self.skip_trivia(on_line_comment);
		let start = self.cursor;
		let byte = *self.remaining().first()?;
		self.cursor += 1;
		Some(match byte {
			b'"' => self.string(),
			b'_' | b'a'..=b'z' | b'A'..=b'Z' => {
				while self
					.remaining()
					.first()
					.is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
				{
					self.cursor += 1;
				}
				Token::Identifier(&self.source[start..self.cursor])
			}
			b'=' if self.remaining().starts_with(b"=") => {
				self.cursor += 1;
				Token::Other
			}
			b'=' => Token::Assignment(WriteOperator::Plain),
			b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'^'
				if self.remaining().starts_with(b"=") =>
			{
				self.cursor += 1;
				Token::Assignment(WriteOperator::Compound)
			}
			byte => Token::Symbol(byte),
		})
	}
}

impl<'a> Iterator for Lexer<'a> {
	type Item = Token<'a>;

	fn next(&mut self) -> Option<Self::Item> {
		self.next_token(|_, _| {})
	}
}
