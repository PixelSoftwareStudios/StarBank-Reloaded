# Package qualification

## macOS arm64 disk image

The debug DMG was built at `dfe70d8f9e23b5a0596254a51681629f1a294ae9` on macOS 14.7.2 arm64 using Xcode SDK 15.2, Rust 1.100.0, Node 24.22.3, pnpm 12.23.0 and Tauri CLI 2.12.6. The source archive SHA-256 was `876db8de99fb989b1c4b56e8fc3650e5e8fec074124c7d08cf028a1fc773de31`.

```sh
export CARGO_NET_OFFLINE=true CI=true
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=3
pnpm install --offline --frozen-lockfile
cd apps/starbank-desktop
pnpm exec tauri build --debug --no-bundle --no-sign -- --locked
pnpm exec tauri bundle --debug --bundles dmg --no-sign
```

The resulting `StarBank Reloaded_0.1.0_aarch64.dmg` was 9,435,921 bytes with SHA-256 `5370a17a690bfa2bd41956d7fb98b349aade7f95df4e81c58d483ec85711d98c`. `hdiutil verify` passed. Read-only attachment confirmed the HFS volume, Applications alias, arm64 executable, bundle identifier and version. The executable had an ad-hoc signature and no signing team. The app declared `.SC2Bank` with the Editor role, Alternate rank and imported type conforming to `public.xml`. The volume detached cleanly. Finder installation from the image, notarization and default-handler behavior remain outside this check.

## Debian arm64 package

The Linux package was rebuilt at `958668c5e458dfe87fac35182455ea60ee28b16e` with the [Linux bundle configuration](../../apps/starbank-desktop/src-tauri/tauri.linux.conf.json) and [launcher template](../../apps/starbank-desktop/src-tauri/linux/starbank.desktop) applied. The clean source used the frozen frontend install and production Vite assets, Rust 1.100.0, Debian 12 arm64, and the official GNU arm64 Tauri CLI 2.12.6 release archive with SHA-256 `1961bbe130567adce85bf6ff070a5839efe184f3fce74b95a653b0663c335315`.

```sh
pnpm install --offline --frozen-lockfile
pnpm --filter starbank-desktop run build:webview
export RUSTUP_TOOLCHAIN=1.100.0 CARGO_NET_OFFLINE=true CI=true
cargo build --offline --locked -p starbank-desktop --features custom-protocol
cd apps/starbank-desktop
cargo tauri bundle --debug --bundles deb --no-sign
```

The resulting `starbank-reloaded_0.1.0_arm64.deb` was 9,309,134 bytes with SHA-256 `6ca75c4db715e948fe095c144b883b79f5cf9a802060b6b1d72c22377a82dc20`.

- The package identifies as `starbank-reloaded` in section `utils`. The launcher displays StarBank Reloaded in the Utility category. The app retains bundle identifier `org.starbank-reloaded.desktop`. The payload contains the binary, icon and launcher, and declares GTK3 and WebKitGTK 4.1 dependencies.
- A disposable container with networking disabled installed it with `dpkg -i`. It resolved the declared runtime libraries and opened the branded 850×500 welcome window in Xvfb as an unprivileged user.
- `dpkg --purge starbank-reloaded` removed the app and launcher. Checksums of the synthetic bank, its backup and existing MIME defaults remained identical. The package contains no installation scripts and declares no MIME handler.

## Windows running-instance delivery

The Windows association passes a selected bank through startup arguments. Forwarding it to an already-running editor remains unimplemented. Explorer delivery and installer registration still require native Windows verification.

Tauri single-instance 2.5.5 matches the pinned Tauri series and supports a Windows-only dependency. Its Windows receiver decodes and allocates incoming arguments before the application callback can apply its queue limit. The receiver also assumes its native pointers are valid. A bounded receiver is required before adding this dependency for running-instance delivery. [Pinned plugin manifest](https://github.com/tauri-apps/plugins-workspace/blob/128190689dbe59b59d344e01bf07b00862aa3791/plugins/single-instance/Cargo.toml) · [Windows receiver](https://github.com/tauri-apps/plugins-workspace/blob/128190689dbe59b59d344e01bf07b00862aa3791/plugins/single-instance/src/platform_impl/windows.rs#L175-L207)

## Windows x64 NSIS

The [Windows desktop CI job](../../.github/workflows/ci.yml) builds an unsigned debug installer with the current per-user NSIS hooks and runs the [silent installer checks](../../apps/starbank-desktop/verification/windows-nsis.ps1) on a fresh GitHub-hosted Windows runner. The checks cover Open With registration and the quoted executable and filename, preserve a synthetic default handler and bank through uninstall, and compare the installed executable's SHA-256 with the fresh build after applying the expected Tauri 2.12.6 bundle marker. A failed check logs the expected and observed registry values and hashes without uploading binaries. [Tauri 2.12.6 bundler](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.6/crates/tauri-bundler/src/bundle.rs#L35-L89)

Local validation passed with actionlint, the PowerShell 7.5.4 parser, and PSScriptAnalyzer 1.24.0. Synthetic probes covered the marker change, path guards, and failure reporting and cleanup. Hosted Windows execution, Explorer open flows, signed release artifacts, and final rights and license approval remain open.
