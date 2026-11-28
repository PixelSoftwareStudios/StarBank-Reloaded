# RFD 0.17.2 GTK chooser ownership patch

`crates/vendor/rfd` contains all 61 files published in `rfd-0.17.2.crate`.
The crates.io archive SHA-256 is
`20dafead71c16a34e1ff357ddefc8afc11e7d51d6d2b9fbd07eaa48e3e540220`.
The package's VCS metadata records revision
`cca0cfa5fc38c2752c9c98f9f5168904a515fd55`. The publisher's MIT license,
copyright notice, and the nested portal license retain their exact release
bytes. [Crates.io release](https://crates.io/crates/rfd/0.17.2) · [Package source](https://github.com/PolyMeilex/rfd/tree/cca0cfa5fc38c2752c9c98f9f5168904a515fd55)

The patch changes only `src/backend/gtk3/file_dialog/dialog_ffi.rs`. The GTK
chooser's `Drop` implementation destroys the dialog, then releases the owned
constructor reference with `g_object_unref`. GTK documents that destroying
the native dialog leaves that reference to its owner. [GTK constructor ownership](https://docs.gtk.org/gtk3/ctor.FileChooserNative.new.html) · [GTK destroy contract](https://docs.gtk.org/gtk3/method.NativeDialog.destroy.html)

Use Python 3.11 or later on Linux or macOS to verify the vendored tree from
the repository root.

```sh
python3 -B crates/vendor/verification/rfd/check.py
python3 -B crates/vendor/verification/rfd/test_check.py
```

Add `--original-crate /path/to/rfd-0.17.2.crate` to verify the original package
as well. The verifier reads each archived file, compares it to the stock
manifest, and computes the patched file hash from the exact substitution in
`manifest.json`. The checkout must contain exactly those paths and file bytes,
with the patched digest substituted for the original. This check covers source
integrity. Runtime GTK chooser teardown is verified separately.
