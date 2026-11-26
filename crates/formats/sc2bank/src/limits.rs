const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;
const MAX_DEPTH: usize = 64;
const MAX_NODES: u32 = 200_000;
const MAX_ATTRIBUTES_PER_ELEMENT: usize = 64;
const MAX_SCALAR_BYTES: usize = 1024 * 1024;
const MAX_MARKUP_MARKERS: usize = 400_000;

// The validator deduplicates declarations globally, and may copy inherited bindings
// on a nested declaration. Bound both dimensions independently of general node size.
pub(crate) const MAX_NAMESPACE_DECLARATIONS: usize = 1024;
pub(crate) const MAX_NAMESPACE_SCOPE: usize = 128;

/// Resource bounds shared by the XML reader and writer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
	/// Maximum complete input or output size in native and working UTF-8 bytes.
	pub document_bytes: usize,
	/// Maximum nesting of XML elements, including the root.
	pub element_depth: usize,
	/// Maximum count of XML nodes, including opaque elements and character data.
	pub nodes: u32,
	/// Maximum attributes on an element, including namespace declarations.
	pub attributes_per_element: usize,
	/// Maximum byte length of an attribute value or text-like XML token.
	pub scalar_bytes: usize,
	/// Maximum count of either literal markup marker `<` or `=` in the source.
	///
	/// The XML validator estimates allocations using these counters even inside opaque text.
	pub markup_markers: usize,
}

impl Default for Limits {
	fn default() -> Self {
		Self {
			document_bytes: MAX_DOCUMENT_BYTES,
			element_depth: MAX_DEPTH,
			nodes: MAX_NODES,
			attributes_per_element: MAX_ATTRIBUTES_PER_ELEMENT,
			scalar_bytes: MAX_SCALAR_BYTES,
			markup_markers: MAX_MARKUP_MARKERS,
		}
	}
}

/// Resource controlled by an XML size limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LimitKind {
	/// Complete serialized input or output.
	DocumentBytes,
	/// Nested elements.
	ElementDepth,
	/// Total XML nodes.
	Nodes,
	/// Attributes on one element.
	AttributesPerElement,
	/// Text or attribute length.
	ScalarBytes,
	/// Namespace declarations across the document.
	NamespaceDeclarations,
	/// Namespace declarations active along one ancestor chain.
	NamespaceScope,
	/// Markup-marker count used to bound validator allocations.
	MarkupMarkers,
}
