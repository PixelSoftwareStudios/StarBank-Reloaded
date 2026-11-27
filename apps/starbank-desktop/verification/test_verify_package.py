"""Small valid and corrupted package metadata fixtures for the headless gate."""

from pathlib import Path
import plistlib
import struct
import tempfile
import unittest

from verify_package import verify_debian, verify_macos

VERSION = "0.1.0"
DOCUMENT_TYPE = {
    "CFBundleTypeExtensions": ["SC2Bank"],
    "LSItemContentTypes": ["org.starbank-reloaded.sc2bank"],
    "CFBundleTypeRole": "Editor",
    "LSHandlerRank": "Alternate",
}
IMPORTED_TYPE = {
    "UTTypeIdentifier": "org.starbank-reloaded.sc2bank",
    "UTTypeConformsTo": ["public.xml"],
    "UTTypeTagSpecification": {"public.filename-extension": ["SC2Bank"]},
}
LAUNCHER = """[Desktop Entry]
Name=StarBank Reloaded
Type=Application
Exec=starbank-desktop
Icon=starbank-desktop
Terminal=false
Categories=Utility;
"""


class PackageChecks(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def write(self, name, data):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        return path

    def macos(self, **overrides):
        info = {
            "CFBundleIdentifier": "org.starbank-reloaded.desktop",
            "CFBundleDisplayName": "StarBank Reloaded",
            "CFBundlePackageType": "APPL",
            "CFBundleShortVersionString": VERSION,
            "CFBundleExecutable": "starbank-desktop",
            "CFBundleIconFile": "icon.icns",
            "CFBundleDocumentTypes": [DOCUMENT_TYPE],
            "UTImportedTypeDeclarations": [IMPORTED_TYPE],
        }
        info.update(overrides)
        self.write("Contents/Info.plist", plistlib.dumps(info))
        self.write("Contents/Resources/icon.icns", b"icns")
        mach_header = struct.pack("<8I", 0xFEEDFACF, 0x0100000C, 0, 2, 0, 0, 0, 0)
        self.write("Contents/MacOS/starbank-desktop", mach_header).chmod(0o755)

    def debian(self, name="amd64", machine=62, launcher=LAUNCHER, dependencies=None):
        if dependencies is None:
            dependencies = "libwebkit2gtk-4.1-0, libgtk-3-0"
        control = f"""Package: starbank-reloaded
Version: {VERSION}
Architecture: {name}
Section: utils
Depends: {dependencies}
Description: Local bank file editor
 Has a multiline description.
"""
        self.write("DEBIAN/control", control.encode())
        self.write("usr/share/applications/starbank-reloaded.desktop", launcher.encode())
        self.write("usr/share/icons/hicolor/256x256/apps/starbank-desktop.png", b"\x89PNG\r\n\x1a\n")
        elf_header = b"\x7fELF\x02\x01\x01" + bytes(9) + struct.pack("<HHI", 3, machine, 1) + bytes(40)
        self.write("usr/bin/starbank-desktop", elf_header).chmod(0o755)

    def test_arm64_app_with_alternate_editor_and_imported_uti(self):
        self.macos()
        verify_macos(self.root, VERSION, "ARM64")

    def test_claiming_owner_rank_is_refused(self):
        self.macos(CFBundleDocumentTypes=[{**DOCUMENT_TYPE, "LSHandlerRank": "Owner"}])
        with self.assertRaisesRegex(ValueError, "Editor/Alternate"):
            verify_macos(self.root, VERSION, "ARM64")

    def test_incorrect_imported_extension_is_rejected(self):
        self.macos(UTImportedTypeDeclarations=[{**IMPORTED_TYPE, "UTTypeTagSpecification": {}}])
        with self.assertRaisesRegex(ValueError, "must map SC2Bank"):
            verify_macos(self.root, VERSION, "ARM64")

    def test_macos_wrong_cpu_is_rejected(self):
        self.macos()
        with self.assertRaisesRegex(ValueError, "native X64 Mach-O"):
            verify_macos(self.root, VERSION, "X64")

    def test_debian_native_architectures(self):
        for architecture, name, machine in (("X64", "amd64", 62), ("ARM64", "arm64", 183)):
            with self.subTest(architecture=architecture):
                self.debian(name, machine)
                verify_debian(self.root, VERSION, architecture)

    def test_debian_binary_cannot_disagree_with_package_architecture(self):
        self.debian(machine=183)
        with self.assertRaisesRegex(ValueError, "native X64 ELF"):
            verify_debian(self.root, VERSION, "X64")

    def test_debian_requires_standalone_runtime_dependencies(self):
        self.debian(dependencies="libwebkit2gtk-4.1-0 | other, libgtk-3-0")
        with self.assertRaisesRegex(ValueError, "needs GTK3 and WebKitGTK"):
            verify_debian(self.root, VERSION, "X64")

    def test_debian_launcher_cannot_advertise_an_association(self):
        self.debian(launcher=LAUNCHER + "MimeType=application/x-sc2bank;\n")
        with self.assertRaisesRegex(ValueError, "leave file association to the user"):
            verify_debian(self.root, VERSION, "X64")

    def test_debian_package_cannot_introduce_install_hooks(self):
        self.debian()
        self.write("DEBIAN/postinst", b"#!/bin/sh\nexit 0\n")
        with self.assertRaisesRegex(ValueError, "Unexpected Debian maintainer script"):
            verify_debian(self.root, VERSION, "X64")


if __name__ == "__main__":
    unittest.main()
