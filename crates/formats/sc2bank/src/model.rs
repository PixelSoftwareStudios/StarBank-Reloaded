use std::ops::Range;

use crate::{Limits, ParseError, XmlEncoding, encoding, offsets::NativeXml};

/// An occurrence in a parsed document, independent of the section name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SectionId(pub(crate) u32);

impl SectionId {
	/// Returns the document-local numeric identifier.
	pub fn get(self) -> u32 {
		self.0
	}
}

/// An occurrence in a parsed document, independent of the key name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyId(pub(crate) u32);

impl KeyId {
	/// Returns the document-local numeric identifier.
	pub fn get(self) -> u32 {
		self.0
	}
}

/// An occurrence in a parsed document, independent of the value node name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueId(pub(crate) u32);

impl ValueId {
	/// Returns the document-local numeric identifier.
	pub fn get(self) -> u32 {
		self.0
	}
}

/// Parsed XML and the exact source bytes needed for a lossless export.
#[derive(Clone, Debug)]
pub struct BankDocument {
	// Working UTF-8 lexical text. Public source() exposes the original native bytes.
	pub(crate) source: String,
	pub(crate) native: Option<NativeXml>,
	pub(crate) limits: Limits,
	pub(crate) root_range: Range<usize>,
	pub(crate) version: Option<String>,
	pub(crate) signature: Option<StoredSignature>,
	pub(crate) sections: Vec<Section>,
	pub(crate) has_unprojected_bank_nodes: bool,
}

impl BankDocument {
	/// Returns the original bytes, including BOM and lexical formatting if present.
	pub fn source(&self) -> &[u8] {
		self.native
			.as_ref()
			.map_or(self.source.as_bytes(), |native| &native.bytes)
	}

	/// Returns the native file encoding.
	pub fn encoding(&self) -> XmlEncoding {
		self.native
			.as_ref()
			.map_or(XmlEncoding::Utf8, |native| native.encoding)
	}

	/// Returns decoded lexical XML, including a leading BOM marker when present.
	///
	/// UTF-16 source declarations and line endings remain as written. Use [Self::source]
	/// for native bytes and for offsets returned by the document's nodes.
	pub fn xml_text(&self) -> &str {
		&self.source
	}

	/// Encodes a working XML draft in the bank's native encoding and byte order.
	///
	/// A UTF-16 draft preserves the required native byte-order mark even when its UTF-8
	/// presentation omits the marker. The returned bytes still require [crate::parse]
	/// to validate their XML and encoding declaration.
	///
	/// # Errors
	///
	/// Rejects native or working-text output above the document byte limit.
	pub fn encode_xml_text(&self, text: &str, limits: Limits) -> Result<Vec<u8>, ParseError> {
		encoding::encode(text, self.encoding(), limits)
	}

	pub(crate) fn encode_owned_xml_text(
		&self,
		text: String,
		limits: Limits,
	) -> Result<Vec<u8>, ParseError> {
		match self.encoding() {
			XmlEncoding::Utf8 if text.len() <= limits.document_bytes => Ok(text.into_bytes()),
			_ => self.encode_xml_text(&text, limits),
		}
	}

	pub(crate) fn text_range(&self, native: Range<usize>) -> Option<Range<usize>> {
		let Some(encoded) = &self.native else {
			return Some(native);
		};
		Some(
			encoded.offsets.text_byte(&self.source, native.start)?
				..encoded.offsets.text_byte(&self.source, native.end)?,
		)
	}

	pub(crate) fn native_range(&self, text: Range<usize>) -> Option<Range<usize>> {
		match &self.native {
			Some(encoded) => encoded.offsets.native_range(&self.source, text),
			None => Some(text),
		}
	}

	/// Returns the version attribute, distinguishing missing from empty.
	pub fn version(&self) -> Option<&str> {
		self.version.as_deref()
	}

	/// Returns the stored signature element, when present.
	pub fn signature(&self) -> Option<&StoredSignature> {
		self.signature.as_ref()
	}

	/// Returns sections in source order, preserving every occurrence.
	pub fn sections(&self) -> &[Section] {
		&self.sections
	}

	/// Reports whether bank or section children outside the projection occur in the source.
	///
	/// Unknown elements or non-whitespace text in structural positions may affect compatibility.
	pub fn has_unprojected_bank_nodes(&self) -> bool {
		self.has_unprojected_bank_nodes
	}
}

/// A stored signature element whose value is preserved independently of verification.
#[derive(Clone, Debug)]
pub struct StoredSignature {
	pub(crate) value: Option<String>,
	pub(crate) value_range: Option<Range<usize>>,
	pub(crate) source_range: Range<usize>,
}

impl StoredSignature {
	/// Returns the decoded value, distinguishing a missing attribute from an empty one.
	pub fn value(&self) -> Option<&str> {
		self.value.as_deref()
	}

	/// Returns the range of the complete original signature element.
	pub fn source_range(&self) -> Range<usize> {
		self.source_range.clone()
	}
}

/// An ordered section occurrence in bank XML.
#[derive(Clone, Debug)]
pub struct Section {
	pub(crate) id: SectionId,
	pub(crate) name: Option<String>,
	pub(crate) name_source_range: Option<Range<usize>>,
	pub(crate) source_range: Range<usize>,
	pub(crate) keys: Vec<Key>,
}

impl Section {
	/// Returns the document-local identifier.
	pub fn id(&self) -> SectionId {
		self.id
	}

	/// Returns the decoded name, distinguishing missing from empty.
	pub fn name(&self) -> Option<&str> {
		self.name.as_deref()
	}

	/// Returns the range of the complete original section element.
	pub fn source_range(&self) -> Range<usize> {
		self.source_range.clone()
	}

	/// Returns key occurrences in source order.
	pub fn keys(&self) -> &[Key] {
		&self.keys
	}
}

/// An ordered key occurrence whose named children comprise one group.
#[derive(Clone, Debug)]
pub struct Key {
	pub(crate) id: KeyId,
	pub(crate) name: Option<String>,
	pub(crate) name_source_range: Option<Range<usize>>,
	pub(crate) source_range: Range<usize>,
	pub(crate) values: Vec<ValueNode>,
}

impl Key {
	/// Returns the document-local identifier.
	pub fn id(&self) -> KeyId {
		self.id
	}

	/// Returns the decoded name, distinguishing missing from empty.
	pub fn name(&self) -> Option<&str> {
		self.name.as_deref()
	}

	/// Returns the range of the complete original key element.
	pub fn source_range(&self) -> Range<usize> {
		self.source_range.clone()
	}

	/// Returns the child elements in source order, with no invented value for empty keys.
	pub fn values(&self) -> &[ValueNode] {
		&self.values
	}
}

/// A named child of a key and its interpreted type attribute, if any.
#[derive(Clone, Debug)]
pub struct ValueNode {
	pub(crate) id: ValueId,
	pub(crate) name: String,
	pub(crate) namespace: Option<String>,
	pub(crate) source_range: Range<usize>,
	pub(crate) value: Result<TypedAttribute, OpaqueReason>,
}

impl ValueNode {
	/// Returns the identifier used to target a value edit.
	pub fn id(&self) -> ValueId {
		self.id
	}

	/// Returns the element's local name.
	pub fn name(&self) -> &str {
		&self.name
	}

	/// Returns the namespace URI when the element has a namespace.
	pub fn namespace(&self) -> Option<&str> {
		self.namespace.as_deref()
	}

	/// Returns the range of the complete original value element.
	pub fn source_range(&self) -> Range<usize> {
		self.source_range.clone()
	}

	/// Returns the single unambiguous type attribute when one is present.
	pub fn attribute(&self) -> Option<&TypedAttribute> {
		self.value.as_ref().ok()
	}

	/// Returns the reason the value element has no interpreted attribute.
	pub fn opaque_reason(&self) -> Option<OpaqueReason> {
		self.value.as_ref().err().copied()
	}
}

/// Reason a value element can only be inspected in its original XML.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum OpaqueReason {
	/// There is no type attribute.
	MissingType,
	/// Multiple attributes could describe the type.
	AmbiguousType,
	/// Namespace syntax affects interpretation of the value element or type.
	Namespace,
	/// The element has nested markup or significant text beyond a type attribute.
	StructuredContent,
	/// The XML attribute has no safely identifiable source range.
	InvalidSourceSpan,
}

/// Type and content of a child element's single unqualified attribute.
#[derive(Clone, Debug)]
pub struct TypedAttribute {
	pub(crate) kind: ValueKind,
	pub(crate) name_source_range: Range<usize>,
	pub(crate) value: String,
	pub(crate) source_range: Range<usize>,
}

impl TypedAttribute {
	/// Returns the parsed type without discarding unknown type names.
	pub fn kind(&self) -> &ValueKind {
		&self.kind
	}

	/// Returns the XML-decoded attribute value without numeric coercion.
	pub fn value(&self) -> &str {
		&self.value
	}

	/// Returns the original content range inside the attribute quotes.
	pub fn source_range(&self) -> Range<usize> {
		self.source_range.clone()
	}
}

/// Known SC2 type attribute names and preserved unrecognized names.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValueKind {
	/// Exact fixed-point text.
	Fixed,
	/// Flag text.
	Flag,
	/// Integer text.
	Int,
	/// Point text.
	Point,
	/// String text.
	String,
	/// Text value.
	Text,
	/// Unit value.
	Unit,
	/// Type name absent from the known vocabulary.
	Unknown(String),
}

impl ValueKind {
	const FIXED: &str = "fixed";
	const FLAG: &str = "flag";
	const INT: &str = "int";
	const POINT: &str = "point";
	const STRING: &str = "string";
	const TEXT: &str = "text";
	const UNIT: &str = "unit";

	pub(crate) fn from_name(name: &str) -> Self {
		match name {
			Self::FIXED => Self::Fixed,
			Self::FLAG => Self::Flag,
			Self::INT => Self::Int,
			Self::POINT => Self::Point,
			Self::STRING => Self::String,
			Self::TEXT => Self::Text,
			Self::UNIT => Self::Unit,
			name => Self::Unknown(name.to_owned()),
		}
	}

	/// Returns the case-sensitive XML attribute name.
	pub fn as_str(&self) -> &str {
		match self {
			Self::Fixed => Self::FIXED,
			Self::Flag => Self::FLAG,
			Self::Int => Self::INT,
			Self::Point => Self::POINT,
			Self::String => Self::STRING,
			Self::Text => Self::TEXT,
			Self::Unit => Self::UNIT,
			Self::Unknown(name) => name,
		}
	}
}
