#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Prepare the pinned CS8409 source without installing or loading a driver.

Only the original upstream files and patches in sources.json are downloaded.
Fetched GPL code retains its original license. The source hashes describe a
previously tested baseline; run verify-abi.py on a new build before installation.
"""

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.request

KERNEL = "7.2.5-3-omarchy"
ARCHITECTURE = "x86_64"
MAX_DOWNLOAD = 8 * 1024 * 1024
HEADERS = ("hda_auto_parser.h", "hda_local.h")


class PreparationError(Exception):
    """A check failed; no prepared source should be published."""


def digest(data):
    return hashlib.sha256(data).hexdigest()


def relative_path(value):
    path = PurePosixPath(value)
    if not value or path.is_absolute() or ".." in path.parts or str(path) != value:
        raise PreparationError(f"Unsafe manifest path: {value!r}")
    return path


def validate_manifest(manifest):
    if (manifest["schema_version"] != 1 or manifest["kernel"] != KERNEL
            or manifest["architecture"] != ARCHITECTURE):
        raise PreparationError("This helper only supports the pinned x86_64 kernel.")
    for key in ("linux_stable_commit", "apple_driver_commit", "omarchy_package_commit"):
        if not re.fullmatch(r"[0-9a-f]{40}", manifest[key]):
            raise PreparationError(f"Invalid pinned commit: {key}")
    prefixes = (
        "https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/plain/",
        "https://raw.githubusercontent.com/davidjo/snd_hda_macbookpro/"
        + manifest["apple_driver_commit"] + "/",
        "https://raw.githubusercontent.com/omacom/omarchy-pkgs/"
        + manifest["omarchy_package_commit"] + "/pkgbuilds/linux-omarchy/",
    )
    for group in ("inputs", "output_files"):
        seen = set()
        for item in manifest[group]:
            relative_path(item["path"])
            if item["path"] in seen or not re.fullmatch(r"[0-9a-f]{64}", item["sha256"]):
                raise PreparationError(f"Invalid or duplicate manifest entry: {item['path']}")
            seen.add(item["path"])
            if "copy_to" in item:
                relative_path(item["copy_to"])
            if group == "inputs":
                url = item["url"]
                if not url.startswith(prefixes):
                    raise PreparationError(f"Unapproved source URL for {item['path']}")
                if url.startswith(prefixes[0]) and not url.endswith(
                        "?id=" + manifest["linux_stable_commit"]):
                    raise PreparationError("Linux source must use the exact pinned commit.")
                if not 0 < item["bytes"] <= MAX_DOWNLOAD:
                    raise PreparationError(f"Invalid source size: {item['path']}")


def check_environment():
    if os.geteuid() == 0 or os.getuid() == 0:
        raise PreparationError("Run source preparation as your regular user, without sudo.")
    if platform.release() != KERNEL or platform.machine() != ARCHITECTURE:
        raise PreparationError(f"Requires running {KERNEL} on {ARCHITECTURE}; no fallback.")
    if shutil.which("patch") is None:
        raise PreparationError("GNU patch is required for source preparation.")
    if not Path(f"/usr/lib/modules/{KERNEL}/build/Makefile").is_file():
        raise PreparationError(f"Matching kernel headers are missing for {KERNEL}.")


def verified_read(path, expected_hash):
    if path.is_symlink() or not path.is_file():
        raise PreparationError(f"Missing regular source file: {path.name}")
    data = path.read_bytes()
    if digest(data) != expected_hash:
        raise PreparationError(f"SHA256 mismatch: {path.name}; existing files were preserved.")
    return data


def fetch(item, cache):
    """Cache by content hash; corrupt cache entries fail closed."""
    target = cache / item["sha256"]
    if target.exists() or target.is_symlink():
        return verified_read(target, item["sha256"])
    request = urllib.request.Request(item["url"], headers={"User-Agent": "curl/8 source-verification"})
    with urllib.request.urlopen(request, timeout=60) as response:
        if not response.geturl().startswith("https://"):
            raise PreparationError("Refusing a download redirected away from HTTPS.")
        data = response.read(item["bytes"] + 1)
    if len(data) != item["bytes"] or digest(data) != item["sha256"]:
        raise PreparationError(f"Download SHA256/size mismatch: {item['path']}")
    # The enclosing preparation lock serializes cache writers; replacement is atomic.
    with tempfile.NamedTemporaryFile(dir=cache, prefix=".download-", delete=False) as output:
        temporary = Path(output.name)
        output.write(data)
    try:
        temporary.replace(target)
    finally:
        temporary.unlink(missing_ok=True)
    return data


def header_patch(data):
    """Extract only the two reviewed private-header diffs from Omarchy's patch."""
    blocks = re.split(rb"(?=^diff --git )", data, flags=re.MULTILINE)
    selected = []
    for name in HEADERS:
        path = f"sound/hda/common/{name}"
        prefix = f"diff --git a/{path} b/{path}\n".encode()
        matches = [block for block in blocks if block.startswith(prefix)]
        if len(matches) != 1:
            raise PreparationError(f"Expected exactly one Omarchy diff for {name}.")
        selected.append(matches[0])
    return b"".join(selected)


def apply_patch(directory, data, strip, fuzz):
    subprocess.run(
        ["patch", "--batch", "--forward", "--no-backup-if-mismatch",
         f"--fuzz={fuzz}", f"-p{strip}"],
        input=data, cwd=directory, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )


def provenance(manifest):
    return {
        "kernel": manifest["kernel"],
        "architecture": manifest["architecture"],
        "linux_stable_commit": manifest["linux_stable_commit"],
        "apple_driver_commit": manifest["apple_driver_commit"],
        "omarchy_package_commit": manifest["omarchy_package_commit"],
        "source_manifest_sha256": digest(json.dumps(manifest, sort_keys=True).encode()),
        "expected_abi_types": manifest["expected_abi_types"],
        "abi_verification": "Not performed by source preparation. Run audio/verify-abi.py on the built module.",
        "files": manifest["output_files"],
    }


def verify_output(directory, manifest, exact=False):
    if directory.is_symlink() or not directory.is_dir():
        raise PreparationError("Prepared source must be a regular directory.")
    for item in manifest["output_files"]:
        path = directory / item["path"]
        if any(parent.is_symlink() for parent in path.parents if parent != directory.parent):
            raise PreparationError("Refusing symlinks in prepared source paths.")
        verified_read(path, item["sha256"])
    if exact:
        actual = {p.relative_to(directory).as_posix() for p in directory.rglob("*") if p.is_file()}
        expected = {item["path"] for item in manifest["output_files"]}
        if actual != expected:
            raise PreparationError("Prepared source has an unexpected file set.")


def assemble(directory, manifest, downloads):
    for item in manifest["inputs"]:
        if "copy_to" in item:
            path = directory / item["copy_to"]
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(downloads[item["path"]])
    hda = directory / "build/hda"
    # These are the original upstream patches. Their older context needs fuzz 2;
    # all resulting source bytes are verified against the tested baseline below.
    for name in ("patch_cs8409.c.diff", "patch_cs8409.h.diff"):
        apply_patch(hda, downloads["apple/" + name], strip=1, fuzz=2)
    apply_patch(hda, header_patch(downloads["omarchy/0510-sound-updates.patch"]), strip=3, fuzz=0)
    verify_output(directory, manifest, exact=True)


def prepare(manifest, work):
    validate_manifest(manifest)
    if work.is_symlink():
        raise PreparationError("Refusing a symlink as the preparation directory.")
    work.mkdir(parents=True, exist_ok=True)
    with (work / ".prepare.lock").open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise PreparationError("Another source preparation is running.") from error
        output = work / "source"
        expected_provenance = provenance(manifest)
        if output.exists() or output.is_symlink():
            verify_output(output, manifest)
            metadata = output / "BUILD-PROVENANCE.json"
            if metadata.is_symlink() or not metadata.is_file() or json.loads(metadata.read_text()) != expected_provenance:
                raise PreparationError("Existing source provenance differs; it was preserved.")
            return output
        cache = work / "downloads"
        if cache.is_symlink():
            raise PreparationError("Refusing a symlink as the download cache.")
        cache.mkdir(exist_ok=True)
        downloads = {item["path"]: fetch(item, cache) for item in manifest["inputs"]}
        with tempfile.TemporaryDirectory(dir=work, prefix=".prepare-") as temporary:
            staged = Path(temporary) / "source"
            staged.mkdir()
            assemble(staged, manifest, downloads)
            (staged / "BUILD-PROVENANCE.json").write_text(json.dumps(expected_provenance, indent=2) + "\n")
            staged.rename(output)
        return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    here = Path(__file__).resolve().parent
    try:
        check_environment()
        manifest = json.loads((here / "sources.json").read_text())
        output = prepare(manifest, here.parent / ".build/audio")
    except (PreparationError, OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(f"Audio source preparation failed: {error}", file=sys.stderr)
        return 1
    print(f"Verified {len(manifest['output_files'])} source files in {output}")
    print("Nothing installed or loaded. Build next, then run audio/verify-abi.py.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
