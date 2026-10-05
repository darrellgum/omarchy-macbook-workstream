# SPDX-License-Identifier: CC0-1.0
"""Offline preparation tests; synthetic files, no hardware access or installation."""

import copy
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("audio_prepare", Path(__file__).parents[1] / "prepare.py")
prepare = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(prepare)


def fixture():
    """Tiny real patch inputs exercise the same preparation path as the driver."""
    apple_commit = "a" * 40
    package_commit = "b" * 40
    linux_commit = "c" * 40
    manifest = {
        "schema_version": 1, "kernel": prepare.KERNEL, "architecture": "x86_64",
        "linux_stable_commit": linux_commit, "apple_driver_commit": apple_commit,
        "omarchy_package_commit": package_commit, "expected_abi_types": ["hda_codec"],
        "inputs": [], "output_files": [],
    }
    downloads = {}

    def add(path, data, destination=None):
        downloads[path] = data
        item = {"path": path, "sha256": prepare.digest(data), "bytes": len(data),
                "url": "https://raw.githubusercontent.com/davidjo/snd_hda_macbookpro/" + apple_commit + "/" + path}
        if destination:
            item["copy_to"] = destination
            manifest["output_files"].append({"path": destination, "sha256": prepare.digest(b"new\n")})
        manifest["inputs"].append(item)

    for name in ("cs8409.c", "cs8409.h"):
        add("linux/" + name, b"old\n", "build/hda/codecs/cirrus/" + name)
        change = f"--- a/codecs/cirrus/{name}\n+++ b/codecs/cirrus/{name}\n@@ -1 +1 @@\n-old\n+new\n".encode()
        add("apple/patch_" + name + ".diff", change)
    changes = b""
    for name in prepare.HEADERS:
        add("linux/" + name, b"old\n", "build/hda/common/" + name)
        path = "sound/hda/common/" + name
        changes += f"diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-old\n+new\n".encode()
    add("omarchy/0510-sound-updates.patch", changes)
    return manifest, downloads


class PreparationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.work = Path(self.temporary.name) / "audio"
        self.manifest, self.downloads = fixture()

    def run_prepare(self):
        with patch.object(prepare, "fetch", side_effect=lambda item, cache: self.downloads[item["path"]]):
            return prepare.prepare(self.manifest, self.work)

    def test_actual_patch_output_matches_expected_hashes(self):
        result = self.run_prepare()
        for item in self.manifest["output_files"]:
            self.assertEqual((result / item["path"]).read_bytes(), b"new\n")
        self.assertEqual(len(list(result.rglob("*.orig"))), 0)
        metadata = json.loads((result / "BUILD-PROVENANCE.json").read_text())
        self.assertNotIn("abi_types_verified", metadata)
        self.assertIn("Not performed", metadata["abi_verification"])

    def test_idempotence_preserves_build_products_without_network(self):
        result = self.run_prepare()
        (result / "candidate.ko").write_bytes(b"synthetic build artifact")
        with patch.object(prepare, "fetch", side_effect=AssertionError("unexpected download")):
            self.assertEqual(prepare.prepare(self.manifest, self.work), result)
        self.assertEqual((result / "candidate.ko").read_bytes(), b"synthetic build artifact")

    def test_existing_modified_source_is_preserved(self):
        result = self.run_prepare()
        edited = result / self.manifest["output_files"][0]["path"]
        edited.write_bytes(b"user edits\n")
        with self.assertRaisesRegex(prepare.PreparationError, "SHA256 mismatch"):
            self.run_prepare()
        self.assertEqual(edited.read_bytes(), b"user edits\n")

    def test_missing_existing_source_is_not_replaced(self):
        result = self.run_prepare()
        edited = result / self.manifest["output_files"][0]["path"]
        edited.unlink()
        with self.assertRaises(prepare.PreparationError):
            self.run_prepare()
        self.assertFalse(edited.exists())

    def test_bad_final_hash_never_publishes(self):
        self.manifest["output_files"][0]["sha256"] = "0" * 64
        with self.assertRaisesRegex(prepare.PreparationError, "SHA256 mismatch"):
            self.run_prepare()
        self.assertFalse((self.work / "source").exists())
        self.assertEqual(list(self.work.glob(".prepare-*")), [])

    def test_failed_download_leaves_existing_siblings(self):
        self.work.mkdir()
        marker = self.work / "keep.txt"
        marker.write_text("user content")
        with patch.object(prepare, "fetch", side_effect=OSError("network interrupted")):
            with self.assertRaises(OSError):
                prepare.prepare(self.manifest, self.work)
        self.assertFalse((self.work / "source").exists())
        self.assertEqual(marker.read_text(), "user content")

    def test_source_symlink_is_rejected(self):
        self.work.mkdir()
        elsewhere = Path(self.temporary.name) / "elsewhere"
        elsewhere.mkdir()
        (self.work / "source").symlink_to(elsewhere, target_is_directory=True)
        with self.assertRaisesRegex(prepare.PreparationError, "regular directory"):
            self.run_prepare()
        self.assertEqual(list(elsewhere.iterdir()), [])

    def test_provenance_change_is_preserved_and_rejected(self):
        result = self.run_prepare()
        metadata = result / "BUILD-PROVENANCE.json"
        metadata.write_text("{}")
        with self.assertRaisesRegex(prepare.PreparationError, "provenance differs"):
            self.run_prepare()
        self.assertEqual(metadata.read_text(), "{}")

    def test_mismatched_cache_is_preserved_and_not_redownloaded(self):
        self.work.mkdir()
        item = self.manifest["inputs"][0]
        cached = self.work / item["sha256"]
        cached.write_bytes(b"corrupt")
        with patch.object(prepare.urllib.request, "urlopen", side_effect=AssertionError("unexpected network")):
            with self.assertRaisesRegex(prepare.PreparationError, "SHA256 mismatch"):
                prepare.fetch(item, self.work)
        self.assertEqual(cached.read_bytes(), b"corrupt")

    def test_download_checks_size_and_hash_before_caching(self):
        self.work.mkdir()
        item = self.manifest["inputs"][0]
        response = io.BytesIO(b"bad\n")
        response.geturl = lambda: item["url"]
        with patch.object(prepare.urllib.request, "urlopen", return_value=response):
            with self.assertRaisesRegex(prepare.PreparationError, "Download SHA256/size mismatch"):
                prepare.fetch(item, self.work)
        self.assertEqual(list(self.work.iterdir()), [])

    def test_valid_download_can_be_reused_offline(self):
        self.work.mkdir()
        item = self.manifest["inputs"][0]
        response = io.BytesIO(self.downloads[item["path"]])
        response.geturl = lambda: item["url"]
        with patch.object(prepare.urllib.request, "urlopen", return_value=response):
            first = prepare.fetch(item, self.work)
        with patch.object(prepare.urllib.request, "urlopen", side_effect=AssertionError("unexpected network")):
            self.assertEqual(prepare.fetch(item, self.work), first)

    def test_missing_or_duplicate_header_diff_is_rejected(self):
        data = self.downloads["omarchy/0510-sound-updates.patch"]
        for bad in (b"", data + data):
            with self.subTest(data=bool(bad)), self.assertRaises(prepare.PreparationError):
                prepare.header_patch(bad)

    def test_manifest_paths_and_pins_are_checked(self):
        for key, value in (("path", "../outside"), ("path", "/absolute"),
                           ("url", "https://example.invalid/unpinned"), ("sha256", "wrong")):
            changed = copy.deepcopy(self.manifest)
            changed["inputs"][0][key] = value
            with self.subTest(key=key, value=value), self.assertRaises(prepare.PreparationError):
                prepare.validate_manifest(changed)

    def test_environment_rejects_root_and_other_kernel(self):
        with patch.object(prepare.os, "geteuid", return_value=0):
            with self.assertRaisesRegex(prepare.PreparationError, "without sudo"):
                prepare.check_environment()
        with patch.object(prepare.os, "geteuid", return_value=1000), patch.object(prepare.os, "getuid", return_value=1000), \
                patch.object(prepare.platform, "release", return_value="other-kernel"):
            with self.assertRaisesRegex(prepare.PreparationError, "no fallback"):
                prepare.check_environment()


if __name__ == "__main__":
    unittest.main()
