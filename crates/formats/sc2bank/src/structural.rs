use std::{error::Error, fmt, ops::Range};

use crate::{
	BankDocument, Key, KeyId, LimitKind, Limits, OpaqueReason, ParseError, Section, SectionId,
	ValueId, ValueKind, ValueNode, parse,
};

mod ids;
mod source;

pub use ids::{StructuralIdMap, StructuralNodeId};

use ids::InsertedAt;
use source::Patch;

const BANK: &str = "Bank";
const SECTION: &str = "Section";
const KEY: &str = "Key";

/// A structural edit targeting a document-local occurrence.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum StructuralEdit<'input> {
	/// Add a section after the existing sections.
	AddSection {
		/// The decoded name for the new section.
		name: &'input str,
	},
	/// Remove the complete section and its contents.
	RemoveSection {
		/// The section occurrence to remove.
		id: SectionId,
	},
	/// Rename a section or supply its missing name.
	RenameSection {
		/// The section occurrence to rename.
		id: SectionId,
		/// The decoded replacement name.
		name: &'input str,
	},
	/// Append a key to a section.
	AddKey {
		/// The parent section occurrence.
		section: SectionId,
		/// The decoded key name.
		name: &'input str,
	},
	/// Remove the complete key and its contents.
	RemoveKey {
		/// The key occurrence to remove.
		id: KeyId,
	},
	/// Rename a key or supply its missing name.
	RenameKey {
		/// The key occurrence to rename.
		id: KeyId,
		/// The decoded replacement name.
		name: &'input str,
	},
	/// Append a named value to a key.
	AddValue {
		/// The parent key occurrence.
		key: KeyId,
		/// An unqualified XML element name.
		name: &'input str,
		/// The new value's known type.
		kind: &'input ValueKind,
		/// The decoded attribute content.
		value: &'input str,
	},
	/// Remove an entire value element.
	RemoveValue {
		/// The value occurrence to remove.
		id: ValueId,
	},
	/// Rename a value element.
	RenameValue {
		/// The value occurrence to rename.
		id: ValueId,
		/// An unqualified replacement XML element name.
		name: &'input str,
	},
	/// Update a value's type and its decoded lexical content together.
	SetValueType {
		/// The value occurrence whose type will change.
		id: ValueId,
		/// The replacement known type.
		kind: &'input ValueKind,
		/// The replacement lexical content, validated by the editing domain.
		value: &'input str,
	},
}

/// Fully reparsed bank with the correspondence of projected IDs.
#[derive(Clone, Debug)]
pub struct StructuralEditResult {
	/// The candidate bank after applying and validating the complete edit.
	pub document: BankDocument,
	/// Maps surviving occurrence IDs from the input bank to the candidate bank.
	pub remap: StructuralIdMap,
	/// The newly added occurrence, where the operation was an insertion.
	pub added: Option<StructuralNodeId>,
}

/// A failed structural edit. The source bank is unchanged.
#[derive(Debug)]
#[non_exhaustive]
pub enum StructuralEditError {
	/// The target ID is absent from the source bank.
	UnknownTarget(StructuralNodeId),
	/// The target's structure or namespace makes this operation ambiguous.
	UnsupportedTarget(StructuralNodeId),
	/// The operation requires a recognized SC2 value type.
	UnsupportedKind,
	/// The element name is invalid or namespace-qualified.
	InvalidElementName,
	/// Attribute content contains an invalid XML 1.0 scalar.
	InvalidCharacter(char),
	/// The edited XML exceeds a resource limit.
	LimitExceeded(LimitKind),
	/// A source span cannot be changed safely.
	InvalidSourceSpan,
	/// The edited XML failed validation.
	InvalidOutput(ParseError),
	/// The parsed result did not match the planned set of occurrence changes.
	UnexpectedProjection,
}

impl fmt::Display for StructuralEditError {
	fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::UnknownTarget(id) => write!(output, "unknown structural edit target {id:?}"),
			Self::UnsupportedTarget(id) => {
				write!(output, "unsupported structural edit target {id:?}")
			}
			Self::UnsupportedKind => write!(output, "expected a recognized SC2 value type"),
			Self::InvalidElementName => write!(output, "expected an unqualified XML element name"),
			Self::InvalidCharacter(character) => write!(
				output,
				"attribute contains invalid XML character U+{:04X}",
				u32::from(*character),
			),
			Self::LimitExceeded(limit) => write!(output, "XML edit limit exceeded ({limit:?})"),
			Self::InvalidSourceSpan => write!(output, "unsafe XML source span for structural edit"),
			Self::InvalidOutput(error) => {
				write!(output, "could not validate edited bank ({error})")
			}
			Self::UnexpectedProjection => {
				write!(output, "reparsed bank did not match the structural edit")
			}
		}
	}
}

impl Error for StructuralEditError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		match self {
			Self::InvalidOutput(error) => Some(error),
			_ => None,
		}
	}
}

/// Applies one structural edit, reparses the result, and maps surviving occurrence IDs.
///
/// Only the selected attribute, tag, or subtree is replaced. Explicit removals include the
/// entire selected subtree. XML outside those spans stays byte-identical. This format
/// operation leaves signature and numeric/domain validation to the respective callers.
///
/// # Errors
///
/// Returns a typed error for invalid targets, unsupported types or names, exceeded resource
/// limits, unsafe source spans, and candidates that fail parsing or occurrence reconciliation.
pub fn apply_structural_edit(
	document: &BankDocument,
	edit: StructuralEdit<'_>,
	limits: Limits,
) -> Result<StructuralEditResult, StructuralEditError> {
	let plan = plan_edit(document, edit, limits)?;
	let candidate = if plan.patches.is_empty() && document.limits == limits {
		document.clone()
	} else {
		let source = source::materialize(&document.source, &plan.patches, limits)?;
		parse(source.as_bytes(), limits).map_err(|error| match error.kind() {
			crate::ParseErrorKind::LimitExceeded(limit) => {
				StructuralEditError::LimitExceeded(limit)
			}
			_ => StructuralEditError::InvalidOutput(error),
		})?
	};
	let (remap, added) =
		StructuralIdMap::reconcile(document, &candidate, &plan.patches, plan.inserted)?;
	Ok(StructuralEditResult {
		document: candidate,
		remap,
		added,
	})
}

#[derive(Default)]
struct Plan {
	patches: Vec<Patch>,
	inserted: Option<InsertedAt>,
}

impl Plan {
	fn patch(patch: Patch) -> Self {
		Self {
			patches: vec![patch],
			inserted: None,
		}
	}

	fn insertion(patch: Patch, inserted: InsertedAt) -> Self {
		Self {
			patches: vec![patch],
			inserted: Some(inserted),
		}
	}

	fn removal(range: Range<usize>) -> Self {
		Self::patch(Patch::new(range, String::new()))
	}
}

fn plan_edit(
	document: &BankDocument,
	edit: StructuralEdit<'_>,
	limits: Limits,
) -> Result<Plan, StructuralEditError> {
	match edit {
		StructuralEdit::AddSection { name } => plan_add_section(document, name, limits),
		StructuralEdit::RemoveSection { id } => {
			Ok(Plan::removal(find_section(document, id)?.source_range()))
		}
		StructuralEdit::RenameSection { id, name } => {
			let section = find_section(document, id)?;
			let patches = source::rename_attribute(
				&document.source,
				section.source_range(),
				SECTION,
				section.name(),
				section.name_source_range.clone(),
				name,
				limits,
			)?;
			Ok(Plan {
				patches,
				inserted: None,
			})
		}
		StructuralEdit::AddKey { section, name } => {
			let section = find_section(document, section)?;
			let child = source::new_named_child(KEY, name, limits)?;
			let (patch, start) =
				source::append_child(&document.source, section.source_range(), SECTION, child)?;
			Ok(Plan::insertion(patch, InsertedAt::Key(start)))
		}
		StructuralEdit::RemoveKey { id } => {
			Ok(Plan::removal(find_key(document, id)?.source_range()))
		}
		StructuralEdit::RenameKey { id, name } => {
			let key = find_key(document, id)?;
			let patches = source::rename_attribute(
				&document.source,
				key.source_range(),
				KEY,
				key.name(),
				key.name_source_range.clone(),
				name,
				limits,
			)?;
			Ok(Plan {
				patches,
				inserted: None,
			})
		}
		StructuralEdit::AddValue {
			key,
			name,
			kind,
			value,
		} => {
			let key = find_key(document, key)?;
			let child = source::new_value_child(name, known_kind(kind)?, value, limits)?;
			let (patch, start) =
				source::append_child(&document.source, key.source_range(), KEY, child)?;
			Ok(Plan::insertion(patch, InsertedAt::Value(start)))
		}
		StructuralEdit::RemoveValue { id } => {
			Ok(Plan::removal(find_value(document, id)?.source_range()))
		}
		StructuralEdit::RenameValue { id, name } => plan_rename_value(document, id, name, limits),
		StructuralEdit::SetValueType { id, kind, value } => {
			plan_set_type(document, id, kind, value, limits)
		}
	}
}

fn plan_add_section(
	document: &BankDocument,
	name: &str,
	limits: Limits,
) -> Result<Plan, StructuralEditError> {
	let child = source::new_named_child(SECTION, name, limits)?;
	let trailing_signature = document.signature().filter(|signature| {
		document
			.sections()
			.last()
			.is_none_or(|section| section.source_range().end <= signature.source_range().start)
	});
	let (patch, start) = if let Some(signature) = trailing_signature {
		let start = signature.source_range().start;
		(Patch::new(start..start, child), start)
	} else {
		source::append_child(&document.source, document.root_range.clone(), BANK, child)?
	};
	Ok(Plan::insertion(patch, InsertedAt::Section(start)))
}

fn plan_rename_value(
	document: &BankDocument,
	id: ValueId,
	name: &str,
	limits: Limits,
) -> Result<Plan, StructuralEditError> {
	let node = find_value(document, id)?;
	if node.namespace().is_some()
		|| node
			.opaque_reason()
			.is_some_and(|reason| reason != OpaqueReason::MissingType)
	{
		return Err(StructuralEditError::UnsupportedTarget(
			StructuralNodeId::Value(id),
		));
	}
	if node.name() == name {
		return Ok(Plan::default());
	}
	source::validate_element_name(name, limits)?;
	let element = node.source_range();
	let opening = source::opening_name(&document.source, element.clone(), node.name())?;
	let mut patches = vec![Patch::new(opening, name.to_owned())];
	if !source::is_empty_tag(&document.source, element.clone())? {
		let closing = source::closing_name(&document.source, element, node.name())?;
		patches.push(Patch::new(closing, name.to_owned()));
	}
	Ok(Plan {
		patches,
		inserted: None,
	})
}

fn plan_set_type(
	document: &BankDocument,
	id: ValueId,
	kind: &ValueKind,
	value: &str,
	limits: Limits,
) -> Result<Plan, StructuralEditError> {
	let new_kind = known_kind(kind)?;
	let node = find_value(document, id)?;
	let attribute = node
		.attribute()
		.filter(|attribute| !matches!(attribute.kind(), ValueKind::Unknown(_)))
		.ok_or(StructuralEditError::UnsupportedTarget(
			StructuralNodeId::Value(id),
		))?;
	let mut patches = Vec::new();
	if attribute.kind() != kind {
		let name = &attribute.name_source_range;
		if document.source.get(name.clone()) != Some(attribute.kind().as_str()) {
			return Err(StructuralEditError::InvalidSourceSpan);
		}
		patches.push(Patch::new(name.clone(), new_kind.to_owned()));
	}
	if attribute.value() != value {
		patches.push(source::replace_attribute_content(
			&document.source,
			attribute.source_range(),
			value,
			limits,
		)?);
	}
	Ok(Plan {
		patches,
		inserted: None,
	})
}

fn known_kind(kind: &ValueKind) -> Result<&str, StructuralEditError> {
	if matches!(kind, ValueKind::Unknown(_)) {
		return Err(StructuralEditError::UnsupportedKind);
	}
	Ok(kind.as_str())
}

fn find_section(document: &BankDocument, id: SectionId) -> Result<&Section, StructuralEditError> {
	document
		.sections()
		.iter()
		.find(|section| section.id() == id)
		.ok_or(StructuralEditError::UnknownTarget(
			StructuralNodeId::Section(id),
		))
}

fn find_key(document: &BankDocument, id: KeyId) -> Result<&Key, StructuralEditError> {
	document
		.sections()
		.iter()
		.flat_map(|section| section.keys())
		.find(|key| key.id() == id)
		.ok_or(StructuralEditError::UnknownTarget(StructuralNodeId::Key(
			id,
		)))
}

fn find_value(document: &BankDocument, id: ValueId) -> Result<&ValueNode, StructuralEditError> {
	document
		.sections()
		.iter()
		.flat_map(|section| section.keys())
		.flat_map(|key| key.values())
		.find(|node| node.id() == id)
		.ok_or(StructuralEditError::UnknownTarget(StructuralNodeId::Value(
			id,
		)))
}
