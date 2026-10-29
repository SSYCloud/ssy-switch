import base64
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from nacl.signing import SigningKey
from build_version import release_version, prepare
from updater_assets import verify_signature, write_manifest


class VersionTests(unittest.TestCase):
    def test_automatic_builds_are_ordered_and_fit_msi(self):
        self.assertEqual(release_version("3.21.0", 2), "3.21.2")
        self.assertEqual(release_version("3.21.0", 3), "3.21.3")
        for base, number in [("3.21.0", 65536), ("3.21.0", 0), ("3.21.0", True), ("3.21.0-beta", 2), ("256.0.0", 2)]:
            with self.assertRaises(ValueError):
                release_version(base, number)

    def test_prepare_updates_package_cargo_config_and_locks(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "src-tauri").mkdir()
            for name in ("package.json", "src-tauri/tauri.conf.json"):
                (root / name).write_text('{"version":"3.21.0"}')
            (root / "src-tauri/Cargo.toml").write_text('[package]\nname = "ssy-switch"\nversion = "3.21.0"\n')
            (root / "Cargo.lock").write_text('name = "ssy-switch"\nversion = "3.21.0"\n\nname = "other"\nversion = "3.21.0"\n')
            prepare(root, 7)
            self.assertEqual(json.loads((root / "package.json").read_text())["version"], "3.21.7")
            self.assertIn('version = "3.21.7"', (root / "src-tauri/Cargo.toml").read_text())
            self.assertIn('name = "other"\nversion = "3.21.0"', (root / "Cargo.lock").read_text())


class SignatureTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.key = SigningKey.generate()
        self.key_id = b"12345678"
        self.public = base64.b64encode(b"untrusted comment: test\n" + base64.b64encode(b"Ed" + self.key_id + bytes(self.key.verify_key))).decode()

    def signed(self, name):
        path = self.root / name
        path.write_bytes(b"example installation bundle")
        sig = self.key.sign(hashlib.blake2b(path.read_bytes()).digest()).signature
        comment = b"test artifact"
        global_sig = self.key.sign(sig + comment).signature
        text = b"untrusted comment: test\n" + base64.b64encode(b"ED" + self.key_id + sig) + b"\ntrusted comment: " + comment + b"\n" + base64.b64encode(global_sig) + b"\n"
        encoded = base64.b64encode(text).decode()
        Path(str(path) + ".sig").write_text(encoded)
        return path, encoded

    def test_accepts_valid_signature_rejects_modified_bytes(self):
        path, sig = self.signed("app.exe")
        verify_signature(path, sig, self.public)
        path.write_bytes(b"tampered bundle")
        with self.assertRaisesRegex(ValueError, "verification failed"):
            verify_signature(path, sig, self.public)

    def test_rejects_wrong_public_key_and_modified_comment(self):
        path, sig = self.signed("app.tar.gz")
        wrong = base64.b64encode(b"untrusted comment: test\n" + base64.b64encode(b"Ed" + self.key_id + bytes(SigningKey.generate().verify_key))).decode()
        with self.assertRaises(ValueError):
            verify_signature(path, sig, wrong)
        altered = base64.b64encode(base64.b64decode(sig).replace(b"test artifact", b"changed artifact")).decode()
        with self.assertRaises(ValueError):
            verify_signature(path, altered, self.public)

    def test_manifest_covers_both_macs_and_windows_installer_types(self):
        mac, _ = self.signed("SSY-Switch.app.tar.gz")
        exe, _ = self.signed("app-setup.exe")
        msi, _ = self.signed("app.msi")
        write_manifest(self.root, "SSYCloud/ssy-switch", "v3.21.2-build.10", "3.21.2", self.public, mac, exe, msi)
        manifest = json.loads((self.root / "latest.json").read_text(encoding="utf-8"))
        self.assertEqual(manifest["version"], "3.21.2")
        platforms = manifest["platforms"]
        self.assertEqual(platforms["darwin-aarch64"], platforms["darwin-x86_64"])
        self.assertTrue(platforms["windows-x86_64-msi"]["url"].endswith(".msi"))
        self.assertTrue(platforms["windows-x86_64-nsis"]["url"].endswith(".exe"))
        for entry in platforms.values():
            self.assertTrue(entry["url"].startswith("https://github.com/SSYCloud/ssy-switch/releases/download/"))

    def test_no_manifest_is_written_for_unsigned_artifacts(self):
        mac, _ = self.signed("app.tar.gz")
        exe, _ = self.signed("app.exe")
        msi, _ = self.signed("app.msi")
        msi.write_bytes(b"different")
        with self.assertRaises(ValueError):
            write_manifest(self.root, "SSYCloud/ssy-switch", "tag", "3.21.2", self.public, mac, exe, msi)
        self.assertFalse((self.root / "latest.json").exists())


if __name__ == "__main__":
    unittest.main()
