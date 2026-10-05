#!/usr/bin/env python3
# SPDX-License-Identifier: CC0-1.0
"""Compare 19 shared HDA layouts without loading a module or opening audio.

The candidate must retain DWARF debug information. Live types come from the
unchanged HDA core/generic modules' BTF, never from the replacement CS8409 codec.
This checks sizes, member offsets and bitfields, not every kernel ABI contract.
Source/prototype review and matching kernel headers remain necessary.
"""

import argparse
from dataclasses import dataclass
import json
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys


TYPES = (
    "hda_gen_spec", "hda_codec", "hda_multi_out", "auto_pin_cfg",
    "auto_pin_cfg_item", "nid_path", "hda_pcm_stream", "hda_pcm",
    "hda_jack_tbl", "hda_jack_callback", "hda_fixup", "hda_input_mux",
    "hda_amp_list", "hda_loopback_check", "hda_vmaster_mute_hook",
    "hda_multi_io", "automic_entry", "badness_table", "snd_array",
)
LIVE_MODULES = ("snd_hda_codec_generic", "snd_hda_codec", "snd_hda_core")
DEFAULT_MODULE = (
    Path(__file__).resolve().parent.parent / ".build/audio/source/build/hda/"
    "codecs/cirrus/snd-hda-codec-cs8409.ko"
)


class VerificationError(Exception):
    """A sanitized, user-facing failure; never contains raw tool output."""


@dataclass(frozen=True)
class Member:
    name: str
    depth: int
    offset: int
    size: int
    bit_offset: int | None
    bit_width: int | None


@dataclass(frozen=True)
class Layout:
    size: int
    members: tuple[Member, ...]
    aggregates: tuple[tuple[int, str], ...]


LOCATION = re.compile(
    r"^(?P<decl>.+;)\s*/\*\s*(?P<offset>\d+)"
    r"(?::\s*(?P<bit>\d+))?\s+(?P<size>\d+)\s*\*/$"
)
SUMMARY = re.compile(r"^/\* size: (\d+),.*\bmembers: (\d+)(?:,.*)? \*/$")
COMMENT = re.compile(r"/\*.*?\*/")


def strip_attributes(text: str) -> str:
    """DWARF prints alignment attributes which pahole's BTF output omits."""
    while "__attribute__" in text:
        start = text.index("__attribute__")
        cursor = start + len("__attribute__")
        while cursor < len(text) and text[cursor].isspace():
            cursor += 1
        if cursor == len(text) or text[cursor] != "(":
            raise VerificationError("Unsupported debug-info attribute syntax")
        depth = 0
        while cursor < len(text):
            char = text[cursor]
            depth += (char == "(") - (char == ")")
            cursor += 1
            if depth == 0:
                break
        if depth:
            raise VerificationError("Incomplete debug-info attribute syntax")
        text = text[:start].rstrip() + text[cursor:]
    return text


def parse_layout(text: str, type_name: str) -> Layout:
    """Parse pahole's annotated C; reject incomplete or unfamiliar output.

    Function-pointer parameter spelling and whitespace can differ between DWARF
    and BTF. Names plus numeric layout avoid mistaking those for ABI changes.
    Nested unions retain their members, nesting and container location.
    """
    lines = [strip_attributes(line.strip()) for line in text.splitlines() if line.strip()]
    if not lines or lines[0] != f"struct {type_name} {{" or lines[-1] != "};":
        raise VerificationError(f"{type_name}: missing or unsupported type output")
    depth = 1
    members = []
    aggregates = []
    top_members = 0
    size = declared_members = None
    for line in lines[1:-1]:
        summary = SUMMARY.fullmatch(line)
        if summary:
            if depth != 1 or size is not None:
                raise VerificationError(f"{type_name}: ambiguous size summary")
            size, declared_members = map(int, summary.groups())
            continue
        if not COMMENT.sub("", line).strip():
            continue
        if line in ("union {", "struct {"):
            aggregates.append((depth, line.split()[0]))
            depth += 1
            continue
        match = LOCATION.fullmatch(line)
        if not match:
            raise VerificationError(f"{type_name}: unrecognized member output")
        decl = match["decl"].strip()
        closing = decl.startswith("}")
        if closing:
            depth -= 1
            if depth < 1:
                raise VerificationError(f"{type_name}: unbalanced aggregate")
        pointer = re.search(r"\(\s*\*\s*(\w+)\s*\)", decl)
        ordinary = re.search(r"(\w+)(?:\[[^\]]*\])*\s*(?::\s*(\d+))?;$", decl)
        anonymous = re.fullmatch(r"}\s*;", decl)
        if pointer:
            name, width = pointer[1], None
        elif ordinary:
            name, width = ordinary[1], ordinary[2]
        elif anonymous:
            name, width = "<anonymous>", None
        else:
            raise VerificationError(f"{type_name}: unrecognized member declaration")
        bit = int(match["bit"]) if match["bit"] is not None else None
        width = int(width) if width is not None else None
        if (bit is None) != (width is None):
            raise VerificationError(f"{type_name}: incomplete bitfield annotation")
        members.append(Member(name, depth, int(match["offset"]), int(match["size"]), bit, width))
        if depth == 1:
            top_members += 1
    if depth != 1 or size is None or size <= 0 or not members or top_members != declared_members:
        raise VerificationError(f"{type_name}: incomplete layout or member count")
    for member in members:
        if member.offset + member.size > size:
            raise VerificationError(f"{type_name}: member exceeds structure size")
        if member.bit_offset is not None and member.bit_offset + member.bit_width > member.size * 8:
            raise VerificationError(f"{type_name}: bitfield exceeds storage size")
    return Layout(size, tuple(members), tuple(aggregates))


def run_tool(arguments: list[str]) -> subprocess.CompletedProcess:
    try:
        return subprocess.run(arguments, capture_output=True, text=True, timeout=30, check=False)
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise VerificationError("A required inspection tool could not complete") from exc


def read_layout(type_name: str, path: Path, base: Path | None = None) -> Layout:
    args = ["pahole", "-F", "btf" if base else "dwarf", "-C", type_name]
    if base:
        args.append(f"--btf_base={base}")
    result = run_tool(args + [str(path)])
    if result.returncode:
        raise VerificationError(f"{type_name}: required debug type is unavailable")
    # pahole 1.31 warns about unrelated GCC declaration tags in this kernel's
    # BTF. Numeric member annotations and completeness are checked separately.
    if any(not re.fullmatch(r"WARNING: BTF_KIND_DECL_TAG for unknown BTF id \d+", line)
           for line in result.stderr.splitlines() if line.strip()):
        raise VerificationError(f"{type_name}: unexpected debug-info diagnostic")
    return parse_layout(result.stdout, type_name)


def compare_layouts(type_name: str, live: Layout, compiled: Layout) -> None:
    if live.size != compiled.size:
        raise VerificationError(f"{type_name}: structure size differs")
    if live.aggregates != compiled.aggregates:
        raise VerificationError(f"{type_name}: aggregate structure differs")
    if live.members != compiled.members:
        raise VerificationError(f"{type_name}: member offsets, sizes or bitfields differ")


def verify(module: Path, btf_root: Path = Path("/sys/kernel/btf")) -> list[dict]:
    module = module.resolve()
    if not module.is_file():
        raise VerificationError("Candidate module is missing; build the unstripped module first")
    for tool in ("pahole", "modinfo"):
        if shutil.which(tool) is None:
            raise VerificationError(f"Required inspection tool is missing: {tool}")
    vermagic = run_tool(["modinfo", "-F", "vermagic", str(module)])
    if vermagic.returncode or vermagic.stdout.split()[:1] != [platform.release()]:
        raise VerificationError("Candidate module does not target the running kernel")
    base = btf_root / "vmlinux"
    sources = [btf_root / name for name in LIVE_MODULES if (btf_root / name).is_file()]
    if not base.is_file() or not sources:
        raise VerificationError("Live HDA BTF is unavailable; no verification was performed")
    results = []
    for name in TYPES:
        compiled = read_layout(name, module)
        live = None
        for source in sources:
            try:
                live = read_layout(name, source, base)
                break
            except VerificationError:
                continue
        if live is None:
            raise VerificationError(f"{name}: no complete layout in live HDA BTF")
        compare_layouts(name, live, compiled)
        results.append({"type": name, "size": live.size, "members_checked": len(live.members), "match": True})
    return results


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("module", nargs="?", type=Path, default=DEFAULT_MODULE,
                        help="unstripped candidate .ko (default: repository .build/audio/source/...)")
    parser.add_argument("--json", action="store_true", help="print only a sanitized JSON summary")
    args = parser.parse_args(argv)
    try:
        results = verify(args.module)
    except VerificationError as exc:
        if args.json:
            print(json.dumps({"ok": False, "error": str(exc)}))
        else:
            print(f"ABI check FAILED: {exc}. Do not install this candidate.", file=sys.stderr)
        return 1
    if args.json:
        print(json.dumps({"ok": True, "types_checked": len(results), "layouts": results}, indent=2))
    else:
        print(f"ABI layouts match for all {len(results)} shared HDA types; no module was loaded.")
        print("This verifies layouts only; exact source/prototype review is still required.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
