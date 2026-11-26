# Bank format and lossless model

Reference: [v2.0 BankReader.cs](https://github.com/BlueRaja/Starbank/blob/v2.0/StarBank/Bank%20Stuffs/BankReader.cs) (child node handling), [BankWriter.cs](https://github.com/BlueRaja/Starbank/blob/v2.0/StarBank/Bank%20Stuffs/BankWriter.cs). Source checked 2026-11-26 on master; tag is the acceptance baseline. Bank XML is not an MPQ archive. No StormLib, ZIP extraction, or game installation required for editing an external bank.

Illustrative synthetic example (not a captured or signed SC2 fixture):

```xml
<?xml version="1.0" encoding="utf-8" standalone="yes"?>
<Bank version="1">
	<Section name="Player">
		<Key name="Gold"><Value int="1000000"/></Key>
		<Key name="Example"><Value string="A &amp; B"/></Key>
		<Key name="UnitA"><XP fixed="1.5"/><Type string="odkt"/></Key>
		<Key name="Empty"/>
	</Section>
</Bank>
```

Root has version attribute; optional Signature child has `value` attribute. Each Section and Key is ordered; each Key may contain zero, one or many named child values. Type is the **attribute name**, content the attribute value. The ordinary child is named Value, but arbitrary named value children exist. Keep original order and group identity, not just a hash map. Old flat Go model cannot faithfully represent this.

## Model contract

- `BankDocument` owns original input/provenance, parse version, ordered sections, optional stored signature, source representation needed for round-trip. `Section`, `Key`, `ValueNode` have stable IDs independent of name/index. A Key owns a vector of child nodes; no circular pointers. `BankInfo` is separate external metadata (path, name, modified time, player handle and author handle, discovery source, confidence). Use `Option` for missing context, not empty-string magic.
- Value kind typed enum for observed fixed, flag, int, point, string, text, unit and unknown attribute name. Retain lexical string exactly: values are not universally numbers. No coercion of fixed-point representation, trailing zeroes, leading zeros, whitespace, case or XML entity content. Structured attributes that cannot safely be interpreted remain opaque.
- Preserve missing vs empty, repeated name occurrences, arbitrary order, comments, processing instructions, namespace data and unexpected children/attributes. Store raw spans or lossless token tree separately from editable semantic projection; avoid duplicate full DOM. Do not silently drop unfamiliar nodes. Reject ambiguous edits or unsafe canonicalization; allow read-only or raw inspection where appropriate. If the original is unchanged, exporting without signing must yield identical bytes. For an edit, preserve untouched bytes where supported; otherwise use a documented lossless semantic serialization and explicitly disclose formatting changes. Never claim arbitrary byte-for-byte XML roundtrip if serializer changes lexical layout.
- Strict parse root/document structure, bounded bytes, depth, attributes, nodes, string size and decoded lengths; external entities/DTD fetching disabled, reject unsupported DTD rather than expand; no untrusted archive script execution. Encoding declaration/BOM handling must be validated with fixtures; support observed encoding before claiming others. Detect trailing junk, invalid XML, duplicate signature or ambiguous type attributes. Use structured location diagnostics, with byte/line/column positions when available.
- Keep XML parsing, validation, serialization, signature policy and file IO separate. `quick-xml` streaming API candidate; preserve unknown source text/attributes rather than using a simple Serde projection that drops them. XML escaping must be correct. Version preservation vs newly created default version `1` separate.

## Editing and validation

Raw View is not a different format. It uses the same engine, including named children. Raw XML may temporarily be syntactically invalid; do not replace last good model until validated application. Do not silently lose draft on tab change or save. Signed XML must verify after serialization and reparse. Formatting explicitly requested only. Reject unsupported signing ambiguities without preventing safe read-only inspection. See SIGNATURE_SPEC and EDITOR.

## Fixtures

Synthetic simple, multi-value, empty, duplicate, missing names/types, text, Unicode, XML escape, unknown, malformed/truncated, large/deep and encoding variants. Actual user-authorized, anonymized or generated SC2 fixtures with exact signing context are separate from synthetic specimens. Do not publish private handles/data or copyrighted map payloads without permission. Fixture provenance and checksums recorded.
