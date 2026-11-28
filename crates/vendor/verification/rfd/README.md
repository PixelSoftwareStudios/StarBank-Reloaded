# RFD 0.17.2 GTK ownership patches

`crates/vendor/rfd` contains all 61 files published in `rfd-0.17.2.crate`.
The crates.io archive SHA-256 is
`20dafead71c16a34e1ff357ddefc8afc11e7d51d6d2b9fbd07eaa48e3e540220`.
The package's VCS metadata records revision
`cca0cfa5fc38c2752c9c98f9f5168904a515fd55`. The publisher's MIT license,
copyright notice, and the nested portal license retain their exact release
bytes. [Crates.io release](https://crates.io/crates/rfd/0.17.2) · [Package source](https://github.com/PolyMeilex/rfd/tree/cca0cfa5fc38c2752c9c98f9f5168904a515fd55)

The ownership patches affect two files.

- `src/backend/gtk3/file_dialog/dialog_ffi.rs` releases the native chooser's
  constructor reference after destroying the dialog. Single selection checks
  for a missing filename and frees the returned buffer after copying the
  path. Multiple selection frees both the filenames and the list nodes after
  collection. Both selection methods retain the release's UTF-8 filtering.
  [GTK constructor ownership](https://docs.gtk.org/gtk3/ctor.FileChooserNative.new.html) · [GTK destroy contract](https://docs.gtk.org/gtk3/method.NativeDialog.destroy.html) · [GTK single filename](https://docs.gtk.org/gtk3/method.FileChooser.get_filename.html) · [GTK filename list](https://docs.gtk.org/gtk3/method.FileChooser.get_filenames.html)
- `src/backend/gtk3/message_dialog.rs` frees the container's returned list
  nodes after making the labels selectable. The child widgets remain
  borrowed. [GTK container children](https://docs.gtk.org/gtk3/method.Container.get_children.html)

The manifest also records private Linux GTK test hooks in the two backend
files and a test module declaration in `src/lib.rs`. They use test-only FFI
bindings to exercise the same ownership code without changing the production
API. The fixture uses the repository's locked desktop dependency graph.
It runs separately from the source integrity check.

Use Python 3.11 or later on Linux or macOS to verify the vendored tree from
the repository root.

```sh
python3 -B crates/vendor/verification/rfd/check.py
python3 -B crates/vendor/verification/rfd/test_check.py
```

The verifier requires the exact file inventory and digests recorded in
`manifest.json`. For each patched file it reverses the listed hunks, verifies
the restored release digest, then reapplies the same hunks. Each hunk must
match exactly once, and matches must be disjoint. Add
`--original-crate /path/to/rfd-0.17.2.crate` to verify the archive digest, its
complete stock inventory, and the patched digests computed directly from the
archived bytes. These checks cover source integrity.

After building the desktop renderer, run the ownership fixture on Linux with
the normal GTK development dependencies installed:

```sh
python3 -B crates/vendor/verification/rfd/run_ownership.py
```

The runner uses the pinned Rust toolchain and the root locked desktop build's
dependency artifacts. It compiles the actual vendored functions with private
test-only FFI bindings, clears display and preload variables, and requires both
named tests to pass. Six getter cases cover valid, missing and invalid-UTF-8
filenames, empty and populated lists, ordering and a defensive NULL list entry.
The message-helper case checks list cleanup while retaining borrowed children.
Payload poisoning detects copying after release; real GLib allocates and frees
the fixture buffers and links. The checks do not initialize GTK or measure heap
usage. CI runs them on Linux x64 and arm64 separately from installed Xvfb tests.

Cargo 1.100 places libraries in separate build-unit directories. The runner
validates all reported `.rlib` paths and supplies their parent directories for
transitive metadata lookup; the four direct `--extern` paths remain exact.
It does not discover dependencies by globbing prior build outputs. A local
Rust 1.100 probe against the root CLI build reproduced E0463 with only the
direct `serde_json` directory, then compiled and passed its single test with
all 93 Cargo-reported directories.

The fixture is authored Rust with the root formatting configuration:

```sh
rustfmt --check --edition 2021 crates/vendor/verification/rfd/ownership.rs
```
