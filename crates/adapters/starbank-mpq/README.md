# StarBank MPQ metadata reader

This read-only adapter reads three fixed metadata names from a caller-authorized seekable source. It supports offset-zero MPQ V1, V2 and V4 archives with uncompressed classic tables and clear stored or zlib members. V4 archives can include extended tables when the classic tables remain usable. Missing entries return `None`, and a present zero-length entry returns an empty byte buffer. Unsupported archive layouts and codecs, inconsistent data, resource limits and source I/O failures return distinct error variants.

The adapter limits the file length to 512 MiB, lookup tables to 65,536 records each, packed and expanded metadata to 16 MiB per entry, and zlib metadata to 4,096 sectors. It reads the header and classic tables on open and seeks to the requested entry only. It bounds each decompressor output and requires an exact-length stream with a valid end marker. The caller supplies a regular file opened for reading and owns path authorization, source-change handling, and metadata interpretation.

`stormlib-rs` 0.0.3 supplies the fixed-name table primitives with default features disabled. `flate2` 1.1.11 provides the zlib decoder with its Rust backend. The adapter uses bounded reads around those primitives. [StormLib Rust package source](https://docs.rs/crate/stormlib-rs/0.0.3/source/) · [Flate2 package source](https://docs.rs/crate/flate2/1.1.11/source/)

Archive metadata is a discovery hint. Archive authenticity, V3, classic-table compression, extended-only lookup, sector checksums, encrypted members, archive writing and browser integration remain outside this subset. Verification uses synthetic StormLib fixtures. Conformance with game-generated SC2 archives remains open. [Fixtures and reproduction](tests/fixtures/README.md)
