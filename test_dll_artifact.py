"""Validate published DLL identity with harmless bytes; never load a payload."""

import json
import tempfile
import unittest
from pathlib import Path

from dll_artifact import DLL_NAME, metadata, verify_artifact


class DllArtifactTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.dll = self.directory / DLL_NAME
        self.dll.write_bytes(b"harmless DLL fixture")
        self.revision = "a" * 40
        self.identity = metadata(self.dll, self.revision)
        self.write_identity(self.identity)

    def write_identity(self, identity):
        (self.directory / "plugin.json").write_text(
            json.dumps(identity), encoding="utf-8"
        )

    def test_exact_production_identity_is_accepted(self):
        self.assertEqual(
            verify_artifact(self.directory, self.revision), self.dll.resolve()
        )

    def test_wrong_revision_features_target_checksum_and_schema_are_rejected(self):
        for key, value in [
            ("toolkit_revision", "b" * 40),
            ("features", ["installer", "fixture-engine"]),
            ("target", "x86_64-pc-windows-msvc"),
            ("sha256", "0" * 64),
            ("schema", 2),
        ]:
            with self.subTest(key=key):
                self.write_identity({**self.identity, key: value})
                with self.assertRaisesRegex(ValueError, "pinned production toolkit"):
                    verify_artifact(self.directory, self.revision)

    def test_changed_empty_or_missing_dll_is_rejected(self):
        for content in (b"replaced DLL", b""):
            self.dll.write_bytes(content)
            with self.assertRaises(ValueError):
                verify_artifact(self.directory, self.revision)
        self.dll.unlink()
        with self.assertRaises(FileNotFoundError):
            verify_artifact(self.directory, self.revision)


if __name__ == "__main__":
    unittest.main()
