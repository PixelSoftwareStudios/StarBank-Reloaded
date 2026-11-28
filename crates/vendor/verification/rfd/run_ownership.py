#!/usr/bin/env python3
"""Run real vendored GTK ownership functions against root-graph test shims."""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib


ROOT = Path(__file__).resolve().parents[4]
VENDOR = ROOT / "crates/vendor/rfd"
TESTS = (
    "backend::gtk3::file_dialog::dialog_ffi::gtk_ownership_getters",
    "backend::gtk3::message_dialog::gtk_ownership_children",
)
# These are the direct externs referenced by RFD's GTK-selected source. Portal
# dependencies can have distinct build/target units and are deliberately absent.
REQUIRED = {"gtk_sys", "glib_sys", "gobject_sys", "raw_window_handle"}


def run(arguments, *, env=None):
    result = subprocess.run(
        arguments, cwd=ROOT, env=env, text=True, capture_output=True, check=False
    )
    if result.returncode:
        raise RuntimeError(
            f"Command failed ({result.returncode}): {arguments!r}\n"
            f"{result.stdout[-12000:]}\n{result.stderr[-12000:]}"
        )
    return result.stdout


def main():
    if sys.platform != "linux":
        raise RuntimeError("This verification requires Linux root-graph GTK artifacts")
    version = run(["rustc", "-vV"])
    pinned = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    fields = dict(line.split(": ", 1) for line in version.splitlines() if ": " in line)
    if fields.get("release") != pinned:
        raise RuntimeError(f"rustc release differs from pinned {pinned}: {version}")
    metadata = json.loads(run([
        "cargo", "metadata", "--locked", "--format-version", "1",
        "--filter-platform", fields["host"],
    ]))
    package = next(
        package for package in metadata["packages"]
        if Path(package["manifest_path"]).resolve() == VENDOR / "Cargo.toml"
    )
    if package["name"] != "rfd" or package["version"] != "0.17.2":
        raise RuntimeError("Unexpected vendored RFD identity")
    messages = [json.loads(line) for line in run([
        "cargo", "build", "--locked", "-p", "starbank-desktop", "--message-format=json",
    ]).splitlines() if line.startswith("{")]
    artifacts = [message for message in messages if message.get("reason") == "compiler-artifact"]
    rfd = [artifact for artifact in artifacts if artifact["package_id"] == package["id"] and "lib" in artifact["target"]["kind"]]
    if len(rfd) != 1:
        raise RuntimeError(f"Expected exactly one root RFD artifact, got {len(rfd)}")
    features = rfd[0]["features"]
    if "gtk3" not in features or "xdg-portal" in features:
        raise RuntimeError(f"Unexpected root RFD backend features: {features}")
    node = next(node for node in metadata["resolve"]["nodes"] if node["id"] == package["id"])
    externs = {}
    directories = set()
    for dependency in node["deps"]:
        if dependency["name"] not in REQUIRED:
            continue
        if not any(kind["kind"] is None for kind in dependency["dep_kinds"]):
            continue
        candidates = {
            Path(filename).resolve()
            for artifact in artifacts if artifact["package_id"] == dependency["pkg"]
            for filename in artifact["filenames"] if filename.endswith(".rlib")
        }
        if not candidates:
            continue  # Locked target-specific dependency absent from this build.
        if len(candidates) != 1:
            raise RuntimeError(f"Ambiguous root artifact for {dependency['name']}: {candidates}")
        artifact = candidates.pop()
        externs[dependency["name"]] = artifact
        directories.add(artifact.parent)
    if not REQUIRED <= externs.keys():
        raise RuntimeError(f"Missing root artifacts: {REQUIRED - externs.keys()}")
    print(f"Root RFD package={package['id']} features={features}", flush=True)
    for name, artifact in sorted(externs.items()):
        print(f"Root extern {name}={artifact}", flush=True)
    env = os.environ.copy()
    for key in ("DISPLAY", "WAYLAND_DISPLAY", "LD_PRELOAD", "LD_AUDIT"):
        env.pop(key, None)
    env["CARGO_MANIFEST_DIR"] = str(VENDOR)
    env["CARGO_PKG_VERSION"] = package["version"]
    with tempfile.TemporaryDirectory(prefix="rfd-ownership-", dir=os.environ.get("RUNNER_TEMP")) as temporary:
        binary = Path(temporary) / "rfd-gtk-ownership"
        command = [
            "rustc", "--test", str(VENDOR / "src/lib.rs"), "--edition=2021",
            "--crate-name", "rfd", "-o", str(binary),
        ]
        for feature in features:
            command.extend(["--cfg", f'feature="{feature}"'])
        for directory in sorted(directories):
            command.extend(["-L", f"dependency={directory}"])
        for name, artifact in sorted(externs.items()):
            command.extend(["--extern", f"{name}={artifact}"])
        for message in messages:
            if message.get("reason") == "build-script-executed":
                for path in message["linked_paths"]:
                    command.extend(["-L", path])
                if message["package_id"] == package["id"]:
                    for cfg in message["cfgs"]:
                        command.extend(["--cfg", cfg])
                    env.update(message["env"])
                    env["OUT_DIR"] = message["out_dir"]
        run(command, env=env)
        listing = run([str(binary), "--list"], env=env)
        discovered = {
            line.removesuffix(": test") for line in listing.splitlines()
            if "gtk_ownership_" in line and line.endswith(": test")
        }
        if discovered != set(TESTS):
            raise RuntimeError(f"Ownership test discovery differs: {listing}")
        for test in TESTS:
            output = run([str(binary), "--exact", test, "--test-threads=1", "--nocapture"], env=env)
            if "test result: ok. 1 passed; 0 failed; 0 ignored;" not in output:
                raise RuntimeError(f"Expected exactly one successful ownership test: {output}")
            print(output, end="")
    print(f"Verified root RFD {package['version']} features={features}, rustc={pinned}")
    print(f"Cargo.lock SHA256={hashlib.sha256((ROOT / 'Cargo.lock').read_bytes()).hexdigest()}")


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, KeyError, StopIteration) as error:
        sys.exit(str(error))
