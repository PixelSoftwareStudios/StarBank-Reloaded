# macOS external signing verification

Some measurements below refer to revisions from before the history rewrite. [Historical evidence and revision availability](VERIFICATION_REGISTER.md#historical-evidence)

On 2026-11-27, a scratch build of `e2cf03533264146e23815f38f1353c2d5c829555` completed the external signing workflow on macOS 14.7.2 (23H311), arm64. The build used Rust 1.100.0 (`0ef1bc386`), Node 24.22.3, pnpm 12.23.0, and Apple Swift 6.0.3 for the accessibility driver.

The driver opened `Current.SC2Bank` from the startup queue. It chose **Tools → Re-sign external bank…** and selected `Foreign.SC2Bank` in the actual AppKit Open sheet. The WKWebView form accepted the manual author, player and bank name and reported that the source was unsigned. **Save signed copy…** opened the actual AppKit Save sheet. The driver confirmed the uniquely named scratch source folder in **Where** and accepted Save. The app reported a verified export and continued displaying the original current bank.

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `Foreign.SC2Bank` | 109 | `6d440abe3637eeefee843391ad4a7c3721c4632a6124d0f99e44b8a414a36086` |
| `Foreign signed.sc2bank` | 170 | `8d30fd0e25e887c5ef73325df14759da460c2bb5173a08e853ba23e527ed73f9` |
| `Current.SC2Bank` | 108 | `41deb9a82a550efb93bee6f37d4927ecc44349c71857c59075d52dcf6fff8f08` |

Python independently computed SHA-1 `89FC4F469AFCE71ADBFA9232810AB818EAF35D74` from the concatenated ASCII strings `1-S2-1-9911`, `1-S2-1-9922`, `DigestBank`, `Wallet`, `Credits`, `Value`, `int`, and `000123`. The stored signature matched. Removing the single signature element yielded the original source bytes, including CRLF and the leading zeroes in `000123`. Both original files matched their pre-run bytes.

## Scratch harness

The scratch patch redirects settings, legacy lookup, inventory root records, backup data, map cache and archive discovery under `STARBANK_SHORTCUT_SMOKE_ROOT`. It also gives the test bundle a unique identifier, clears its macOS file association config, and seeds the native Open and Save dialogs with scratch starting directories. The runner creates a unique temporary root each run and launches its app bundle directly, without an install or default-handler change. The app handles picker returns, signing, IPC and commits through its normal implementations.

The Swift driver uses existing macOS accessibility and input permissions. It selects the AppKit row and raises the sheet. The file-specific AX action returned `-25205` in the passing run. The selected file reaching the signing form and the subsequent successful native save established acceptance.

The source fixture was `<Bank version="1">\r\n<Section name="Wallet"><Key name="Credits"><Value int="000123"/></Key></Section>\r\n</Bank>`, with the two literal CRLF sequences. The scratch build used the locked Tauri `custom-protocol` feature and production Vite assets. This is historical native UI evidence from before the headless-only restriction.
