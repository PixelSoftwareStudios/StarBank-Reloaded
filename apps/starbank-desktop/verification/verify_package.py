#!/usr/bin/env python3
"""Check freshly built desktop package payloads without launching or installing them."""

import argparse
import configparser
from email.parser import Parser
import json
from pathlib import Path, PurePath
import plistlib
import stat
import struct
import subprocess
import tempfile

PRODUCT = "StarBank Reloaded"
PACKAGE = "starbank-reloaded"
EXECUTABLE = "starbank-desktop"
BUNDLE_ID = "org.starbank-reloaded.desktop"
BANK_EXTENSION = "SC2Bank"
BANK_UTI = "org.starbank-reloaded.sc2bank"
DEBIAN_RUNTIME_PACKAGES = {"libgtk-3-0", "libwebkit2gtk-4.1-0"}
DEBIAN_ARCH = {"X64": "amd64", "ARM64": "arm64"}
ELF_MACHINE = {"X64": 62, "ARM64": 183}  # EM_X86_64, EM_AARCH64
MACH_CPU = {"X64": 0x01000007, "ARM64": 0x0100000C}  # 64-bit x86, ARM
MACH_MAGIC_64 = 0xFEEDFACF
MACH_EXECUTE = 2
MACH_HEADER_BYTES = 32
ELF_HEADER_BYTES = 64
ELF_EXECUTABLE_TYPES = {2, 3}  # ET_EXEC, ET_DYN (including PIE)
PNG_HEADER = b"\x89PNG\r\n\x1a\n"
INSTALL_HOOKS = ("preinst", "postinst", "prerm", "postrm", "triggers")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def plain_file(path, executable=False):
    mode = path.lstat().st_mode
    require(stat.S_ISREG(mode), f"{path.name} must be a regular file")
    if executable:
        require(mode & 0o111, f"{path.name} must be executable")
    return path


def header(path, length):
    with path.open("rb") as data:
        result = data.read(length)
    require(len(result) == length, f"{path.name} has an incomplete file header")
    return result


def verify_macos(app, version, architecture):
    require(app.is_dir() and not app.is_symlink(), f"Missing app bundle {app.name}")
    contents = app / "Contents"
    with plain_file(contents / "Info.plist").open("rb") as file:
        info = plistlib.load(file)
    require(isinstance(info, dict), "Info.plist must contain a dictionary")
    expected = {
        "CFBundleIdentifier": BUNDLE_ID,
        "CFBundleDisplayName": PRODUCT,
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": version,
        "CFBundleExecutable": EXECUTABLE,
    }
    for key, value in expected.items():
        require(info.get(key) == value, f"Info.plist {key} must be {value!r}")

    document_types = info.get("CFBundleDocumentTypes", [])
    require(
        isinstance(document_types, list) and len(document_types) == 1
        and isinstance(document_types[0], dict),
        "Expected exactly one macOS document association",
    )
    document = document_types[0]
    require(
        document.get("CFBundleTypeExtensions") == [BANK_EXTENSION]
        and document.get("LSItemContentTypes") == [BANK_UTI]
        and document.get("CFBundleTypeRole") == "Editor"
        and document.get("LSHandlerRank") == "Alternate",
        "macOS bank association must use the SC2Bank Editor/Alternate document type",
    )
    imports = info.get("UTImportedTypeDeclarations", [])
    require(
        isinstance(imports, list) and len(imports) == 1 and isinstance(imports[0], dict),
        "Expected exactly one imported macOS type",
    )
    imported = imports[0]
    tags = imported.get("UTTypeTagSpecification")
    require(
        imported.get("UTTypeIdentifier") == BANK_UTI
        and imported.get("UTTypeConformsTo") == ["public.xml"]
        and isinstance(tags, dict)
        and tags.get("public.filename-extension") == [BANK_EXTENSION],
        "macOS imported bank type must map SC2Bank to public.xml",
    )
    require(
        not info.get("UTExportedTypeDeclarations"),
        "macOS must import the bank type without declaring ownership",
    )
    binary = plain_file(contents / "MacOS" / EXECUTABLE, executable=True)
    magic, cpu, _, kind = struct.unpack_from("<4I", header(binary, MACH_HEADER_BYTES))
    require(
        (magic, cpu, kind) == (MACH_MAGIC_64, MACH_CPU[architecture], MACH_EXECUTE),
        f"Expected a native {architecture} Mach-O executable, found {cpu:#x}/{kind}",
    )
    icon = info.get("CFBundleIconFile", "")
    require(
        isinstance(icon, str) and icon.endswith(".icns") and PurePath(icon).name == icon,
        "Expected a bundle-local .icns icon",
    )
    require(
        header(plain_file(contents / "Resources" / icon), 4) == b"icns",
        "macOS bundle icon is missing its ICNS signature",
    )


def verify_debian(payload, version, architecture):
    control = payload / "DEBIAN"
    info = Parser().parsestr(plain_file(control / "control").read_text(encoding="utf-8"))
    expected = {
        "Package": PACKAGE,
        "Version": version,
        "Architecture": DEBIAN_ARCH[architecture],
        "Section": "utils",
    }
    for key, value in expected.items():
        require(info.get_all(key) == [value], f"Debian control {key} must be {value!r}")
    dependencies = {
        atom.partition("(")[0].strip()
        for atom in info.get("Depends", "").split(",")
        if "|" not in atom
    }
    require(DEBIAN_RUNTIME_PACKAGES <= dependencies, "Debian runtime needs GTK3 and WebKitGTK 4.1")
    require(
        not ({path.name for path in control.iterdir()} & set(INSTALL_HOOKS)),
        "Unexpected Debian maintainer script or triggers",
    )

    share = payload / "usr/share"
    launcher = configparser.ConfigParser(interpolation=None)
    launcher.optionxform = str
    launcher.read_string(
        plain_file(share / "applications" / f"{PACKAGE}.desktop").read_text(encoding="utf-8")
    )
    entry = launcher["Desktop Entry"]
    expected = {
        "Type": "Application",
        "Name": PRODUCT,
        "Exec": EXECUTABLE,
        "Icon": EXECUTABLE,
        "Terminal": "false",
    }
    for key, value in expected.items():
        require(entry.get(key) == value, f"Desktop Entry {key} must be {value!r}")
    require("Utility" in entry.get("Categories", "").split(";"), "Expected Utility category")
    require(
        "MimeType" not in entry and not (share / "mime/packages").exists(),
        "Linux package must leave file association to the user",
    )

    icons = sorted(share.glob(f"icons/hicolor/*/apps/{EXECUTABLE}.png"))
    require(icons, "Debian package is missing its launcher icon")
    for icon in icons:
        require(header(plain_file(icon), len(PNG_HEADER)) == PNG_HEADER, "Invalid launcher PNG")

    binary = plain_file(payload / "usr/bin" / EXECUTABLE, executable=True)
    elf = header(binary, ELF_HEADER_BYTES)
    kind, cpu, elf_version = struct.unpack_from("<HHI", elf, 16)
    require(
        elf[:7] == b"\x7fELF\x02\x01\x01"  # ELF64, little-endian, current version.
        and kind in ELF_EXECUTABLE_TYPES
        and cpu == ELF_MACHINE[architecture]
        and elf_version == 1,
        f"Expected a native {architecture} ELF executable, found {cpu:#x}/{kind}",
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bundle_directory", type=Path)
    parser.add_argument("platform", choices=("Linux", "macOS"))
    parser.add_argument("architecture", choices=tuple(DEBIAN_ARCH))
    args = parser.parse_args()
    config = Path(__file__).resolve().parents[1] / "src-tauri" / "tauri.conf.json"
    try:
        version = json.loads(config.read_text(encoding="utf-8"))["version"]
        if args.platform == "macOS":
            package = args.bundle_directory / "macos" / f"{PRODUCT}.app"
            verify_macos(package, version, args.architecture)
        else:
            filename = f"{PACKAGE}_{version}_{DEBIAN_ARCH[args.architecture]}.deb"
            package = args.bundle_directory / "deb" / filename
            with tempfile.TemporaryDirectory(prefix="starbank-deb-check-") as directory:
                subprocess.run(
                    ["dpkg-deb", "--raw-extract", str(plain_file(package)), directory],
                    check=True,
                )
                verify_debian(Path(directory), version, args.architecture)
    except (
        OSError,
        ValueError,
        KeyError,
        TypeError,
        configparser.Error,
        plistlib.InvalidFileException,
        subprocess.CalledProcessError,
    ) as error:
        parser.exit(1, f"Package verification failed: {error}\n")
    print(f"Verified {package.name} metadata and {args.architecture} executable")


if __name__ == "__main__":
    main()
