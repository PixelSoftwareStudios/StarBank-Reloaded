# GLib 0.18.5 security backport

`crates/vendor/glib` contains the 121 files shipped in `glib-0.18.5.crate`.
The crates.io archive SHA-256 is
`233daaf6e83ae6a12a52055f568f9d7cf4671dabb78ff9560ab6da230ce00ee5`.
The package metadata identifies gtk-rs-core revision
`42b9caf98e03ded086362d9653ca58fe94dc8658`. The packaged MIT license,
copyright notice and VCS metadata are retained. [Crates.io release](https://crates.io/crates/glib/0.18.5) · [Package source revision](https://github.com/gtk-rs/gtk-rs-core/tree/42b9caf98e03ded086362d9653ca58fe94dc8658/glib)

The Linux desktop GTK 3 dependency graph selects this crate version.
The only source change applies upstream revision
`b5a4071e439bef2b5eea76c3aa25e5ae84839e34` to the two affected pointer
casts in `src/variant_iter.rs` for RUSTSEC-2024-0429. To reproduce the
source, verify and unpack the archive, then apply that commit's patch
from the unpacked crate root with `git apply -p2`. The manifest records
the upstream patch SHA-256 and the original and patched file hashes.
[RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2024-0429.html) · [Upstream fix](https://github.com/gtk-rs/gtk-rs-core/commit/b5a4071e439bef2b5eea76c3aa25e5ae84839e34)

Check the snapshot from the repository root with Python 3.11 or later on
Linux or macOS.

```sh
python3 crates/vendor/verification/glib/check.py
python3 crates/vendor/verification/glib/test_check.py
```

Pass `--original-crate /path/to/glib-0.18.5.crate` to check the archive
checksum too. The check covers paths and file bytes and rejects
symlinks under the source path.

On 2026-11-27, a standalone copy with the same backport passed all 11
selected iterator unit tests with `cargo test --release --locked --offline --lib variant_iter`
using Rust 1.100.0 and GLib 2.90.0 on macOS 14.7.2 arm64. Linux application
build, runtime and distribution verification remain open.
