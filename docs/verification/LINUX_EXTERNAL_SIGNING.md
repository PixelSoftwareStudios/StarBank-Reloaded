# Linux external signing verification

This page records the installed Linux GTK acceptance fixture for REQ-SIG-02, REQ-SIG-03, REQ-SAVE-01/02 and REQ-EDIT-07. External signing is the fifth fresh WebDriver session in the [native WebView harness](../../apps/starbank-desktop/verification/linux-native-webview.mjs). It follows editing/export/navigation, unapplied-draft close, Save As on close, and Unicode clipboard sessions. Earlier qualified workflows retain their recorded scope in the [implementation evidence](IMPLEMENTATION.md#native-unsaved-dialog-response-compatibility).

The fixture runs the verified Debian package's `/usr/bin/starbank-desktop` on ephemeral Ubuntu x64 and arm64 runners. Xvfb owns the display, a private D-Bus session supplies accessibility services, and each session redirects config/cache/data into its own temporary directory. `GTK_USE_PORTAL=0` selects the GTK fallback; `G_DEBUG=fatal-criticals` makes a GTK critical fail the run. Rust 1.100.0, Node 24.22.3 and pnpm 12.23.0 match the project pins. [CI workflow](../../.github/workflows/ci.yml)

## Independent synthetic vector

The input is UTF-8 without a BOM. The two `\r\n` sequences below represent literal CRLF bytes; `000123` retains its leading zeroes.

```text
<Bank version="1">\r\n<Section name="Wallet"><Key name="Credits"><Value int="000123"/></Key></Section>\r\n</Bank>
```

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `Foreign.SC2Bank` | 109 | `6d440abe3637eeefee843391ad4a7c3721c4632a6124d0f99e44b8a414a36086` |
| `Foreign signed.sc2bank` | 170 | `8d30fd0e25e887c5ef73325df14759da460c2bb5173a08e853ba23e527ed73f9` |

| Form field | Exact value |
| --- | --- |
| Map author handle | `1-S2-1-9911` |
| Player handle | `1-S2-1-9922` |
| Bank name without extension | `DigestBank` |

The expected signature is `89FC4F469AFCE71ADBFA9232810AB818EAF35D74`. The fixed vector comes from the [independent Python calculation recorded for macOS](MACOS_EXTERNAL_SIGNING.md): SHA-1 over the concatenated ASCII strings `1-S2-1-9911`, `1-S2-1-9922`, `DigestBank`, `Wallet`, `Credits`, `Value`, `int` and `000123`. The expected output inserts exactly `<Signature value="89FC4F469AFCE71ADBFA9232810AB818EAF35D74"/>` immediately before `</Bank>`.

The Linux harness asserts both fixed SHA-256 values before the workflow and compares the output against all 170 expected bytes. Its expected signature and output bytes are constants independent of the product's signing reply. The original foreign file must remain byte exact. These are synthetic context values and an unsigned fixture; in-game acceptance remains separate.

## Fifth-session acceptance assertions

Only `Current.SC2Bank` is passed through startup arguments. The foreign file is created exclusively after the session begins, so the fixture does not queue it as another bank to open. Through actual Settings controls, the harness turns automatic re-sign off, edits the current bank's Gold from `001` to `1234`, and applies the edit. Table View displays `1,234`, reports unsaved changes in one section, has no unapplied draft and offers enabled Undo.

The [external-signing driver](../../apps/starbank-desktop/verification/linux-external-signing.mjs) requires:

1. Tools → Re-sign external bank… opens the actual GTK `Open Bank` chooser. The uniquely titled chooser receives focus. A slash shortcut opens Location; the [native AT-SPI observer](../../apps/starbank-desktop/verification/linux-accessibility.c) requires exact `/` text in the focused, editable, enabled single-line Location entry before the remaining absolute fixture path is typed. It then requires the complete exact path and checks chooser focus again before Return.
2. The signing form identifies `Foreign.SC2Bank · Unsigned`, starts with all three context fields empty, accepts the exact context above, and reports no stored signature after Check signature. The current bank's name, displayed edited value and dirty status remain visible, no foreign open request appears, both source files are unchanged, and the destination is absent. While the modal keeps the editor busy, its table value can be a read-only span instead of an input.
3. Save signed copy… opens the real GTK `Save Signed Bank Copy` chooser at a destination checked to be absent. The product reports the verified signed-copy result and closes its signing form. The harness requires both native chooser X windows, including hidden windows, to have disappeared, then checks the exact signed output and both unchanged sources.
4. After the modal closes, the current bank must again have its editable Table value `1,234`, the same applied dirty edit, enabled Undo, no draft and no queued foreign bank. Settings still shows automatic re-sign disabled. Returning to the editor preserves that state.
5. Native window close followed by explicit Discard closes the main window. Both original sources and the signed output are checked again after close. WebDriver session deletion, owned process-group shutdown and temporary-directory cleanup are bounded; cleanup errors fail the harness. The installed-package job separately checks owned package purge and executable/launcher absence.

These are the acceptance assertions, with executed scope recorded below. The source Location path is typed directly and verified through native accessibility; that path does not by itself prove that the chooser's visible file-list filter accepts every bank-extension casing.

## Hosted attempts

| Revision / run | Scoped outcome |
| --- | --- |
| `0295e4f` / [46116730166](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46116730166) | Both architectures completed the first four sessions. The fifth source-chooser attempt hit a fatal `gtk_widget_event` / `WIDGET_REALIZED_FOR_EVENT` critical. This run lacked the later fine-grained path-entry instrumentation; no signing form or output was qualified. Owned package purge passed. |
| `1b83b88` / [46117529663](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46117529663) | Both source choosers received focus, then the combined Ctrl+L / Ctrl+A sequence hit a fatal critical before path typing or Return. No signing form was observed. Owned package purge passed. |
| `e1bd1ec` / [46119888858](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46119888858) | On x64, SIGTRAP followed the fatal GTK critical; the captured stack included `gtk_widget_event` and `gtk_window_propagate_key_event`. On arm64, typing and Return completed, but the chooser stayed open and the signing form did not appear before timeout. Owned package purge passed on both architectures. These observations do not establish the source of the GTK failure. |
| `a51a535` / [46121524421](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46121524421) | Both architectures passed exact native Location observations for slash and the full path, reached the unsigned signing form, released the source chooser, entered context and observed the no-stored-signature result. The subsequent active-editor fixture assertion failed while the modal kept the editor busy. The current bank still showed dirty status. Save signed copy, output-byte verification and post-close preservation were not reached. Owned package purge passed on both architectures. |

The `a51a535` assertion read only the editable Gold input. Source inspection shows that the external-signing promise keeps the editor busy throughout the modal; when Gold loses focus, the table renders its value as a read-only span. A missing input under that condition does not establish loss of the active edit. The successor `e739ed0b219bcd1f340d26c1426ce16e93b140b7` checks the unique Gold row's input or sole displayed span during the modal. After the modal closes, the original strict editable-input and enabled-Undo checks still apply.

In [successor run 46122005467](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46122005467), the real Linux WebView step completed successfully on both x64 and arm64. This executed the full fifth-session assertions above: native source and destination selection, unsigned checking, exact signed-copy bytes, unchanged sources, retained active dirty edit and enabled Undo after the modal, persisted opt-out and post-close file checks. Both dedicated GTK filter steps also passed. The full matrix and later package-purge results were still pending when this record was written. This scoped pass does not resolve the intermittent source-chooser failure observed in the preceding run.

## Separate Open-filter follow-up

`e09a755` changes the Linux bank Open filter's extension patterns and adds an ignored test of GTK's actual `FileFilter` predicate over all ASCII casings of `.SC2Bank`, with non-bank controls. Its dedicated Xvfb CI step is separate from direct path entry in the fifth session. [Filter implementation and predicate test](../../apps/starbank-desktop/src-tauri/src/selection.rs) · [CI selection](../../.github/workflows/ci.yml)

In run 46121814070, the named GTK predicate test executed and passed on both architectures: one test, zero failures and zero ignored on each. The test covers all 64 ASCII spellings and six excluded filenames using GTK's actual predicate. This qualifies that filter behavior in the tested GTK environment. [x64 job 140907770256](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46121814070/job/140907770256) · [arm64 job 140907770303](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46121814070/job/140907770303)

The separate fifth-session source choosers hit the fatal GTK critical at the first slash after focus on both architectures, before any native Location match. No path, signing form or signed output was qualified in those attempts; owned package purge passed. The earlier successful slash observations at `a51a535` do not establish a stable chooser workflow.

A successful direct-path selection must not be recorded as file-list filter coverage. Portal dialogs, other GTK versions and in-game acceptance remain outside this qualification.

## Mapped-entry diagnostic follow-up

The next driver separates Slash keydown from keyup. Between them, the native probe requires the exact `/` in an entry that is focused, visible and showing. It retains the existing editability, line-mode, exact-text and deadline checks. Keyup is attempted even if submission or observation fails; both errors are retained if release also fails. A second exact observation and X focus check precede path typing. Other chooser drivers and fatal-critical handling are unchanged.

This adds a mapping precondition and stage evidence without a fixed delay. The earlier `e739ed0` pass used a combined key press/release and did not require the new visibility states. Results from that pass do not qualify the stronger observer or the timing of the split-key diagnostic. Hosted execution of this follow-up is pending; no product GTK fix or resolution of ordinary keyboard timing is claimed.
