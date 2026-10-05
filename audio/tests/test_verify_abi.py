# SPDX-License-Identifier: CC0-1.0
"""Synthetic debug-info fixtures: no local kernel dumps or identifiers."""
import importlib.util
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("verify_abi", Path(__file__).parents[1] / "verify-abi.py")
abi = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = abi
spec.loader.exec_module(abi)

SIMPLE = """struct example {
    unsigned int enabled:1; /* 0: 0 4 */
    unsigned int mode:2; /* 0: 1 4 */
    /* XXX 29 bits hole, try to pack */
    void (*hook)(struct other *, int); /* 8 8 */
    /* size: 16, cachelines: 1, members: 3 */
};
"""


class LayoutTests(unittest.TestCase):
    def test_cosmetic_dwarf_btf_differences(self):
        alternative = SIMPLE.replace("struct other *, int", "struct other *codec, int event")
        alternative = alternative.replace("/* XXX 29 bits hole, try to pack */", "/* --- cacheline boundary --- */")
        abi.compare_layouts("example", abi.parse_layout(SIMPLE, "example"), abi.parse_layout(alternative, "example"))

    def test_alignment_attributes_do_not_hide_numeric_layout(self):
        aligned = SIMPLE.replace("};", "} __attribute__((__aligned__(8)));")
        abi.compare_layouts("example", abi.parse_layout(SIMPLE, "example"), abi.parse_layout(aligned, "example"))
        with self.assertRaises(abi.VerificationError):
            abi.parse_layout(SIMPLE.replace("};", "} __attribute__((__aligned__(8));"), "example")

    def test_known_layout_mismatch_classes(self):
        for old, new in (("8 8", "4 8"), ("0: 1", "0: 2"), ("mode:2", "mode:3"),
                         ("size: 16", "size: 24"), ("enabled:1", "changed:1")):
            with self.subTest(change=(old, new)), self.assertRaises(abi.VerificationError):
                abi.compare_layouts("example", abi.parse_layout(SIMPLE, "example"),
                                    abi.parse_layout(SIMPLE.replace(old, new), "example"))

    def test_nested_union_members_are_compared(self):
        text = """struct example {
            union {
                unsigned long integer; /* 0 8 */
                void (*hook)(int); /* 0 8 */
            } value; /* 0 8 */
            /* size: 8, cachelines: 1, members: 1 */
        };"""
        layout = abi.parse_layout(text, "example")
        self.assertEqual(len(layout.members), 3)
        with self.assertRaises(abi.VerificationError):
            abi.compare_layouts("example", layout, abi.parse_layout(text.replace("integer", "different"), "example"))

    def test_zero_sized_anonymous_aggregate_and_flexible_array(self):
        text = """struct example {
            struct {
            }; /* 0 0 */
            unsigned int count; /* 0 4 */
            char tail[]; /* 4 0 */
            /* size: 4, cachelines: 1, members: 3 */
        };"""
        layout = abi.parse_layout(text, "example")
        self.assertEqual([m.size for m in layout.members], [0, 4, 0])

    def test_incomplete_or_unfamiliar_output_fails_closed(self):
        for text in ("", "struct example;", SIMPLE.replace("members: 3", "members: 4"),
                     SIMPLE.replace("/* 8 8 */", ""), SIMPLE.replace("/* 8 8 */", "/* 15 8 */"),
                     SIMPLE.replace("/* size: 16, cachelines: 1, members: 3 */", ""),
                     SIMPLE.replace("mode:2", "mode:40"), SIMPLE.replace("};", "")):
            with self.subTest(text=text), self.assertRaises(abi.VerificationError):
                abi.parse_layout(text, "example")

    def test_tool_diagnostics_are_not_exposed(self):
        result = abi.subprocess.CompletedProcess([], 0, SIMPLE, "private path or arbitrary diagnostic")
        with patch.object(abi, "run_tool", return_value=result), self.assertRaisesRegex(
                abi.VerificationError, "unexpected debug-info diagnostic"):
            abi.read_layout("example", Path("candidate.ko"))

    def test_known_unrelated_tag_warning_still_requires_complete_type(self):
        result = abi.subprocess.CompletedProcess([], 0, SIMPLE, "WARNING: BTF_KIND_DECL_TAG for unknown BTF id 123\n")
        with patch.object(abi, "run_tool", return_value=result):
            self.assertEqual(abi.read_layout("example", Path("candidate.ko")).size, 16)
        result.stdout = ""
        with patch.object(abi, "run_tool", return_value=result), self.assertRaises(abi.VerificationError):
            abi.read_layout("example", Path("candidate.ko"))


class VerificationTests(unittest.TestCase):
    def setUp(self):
        for target, kwargs in (
            ("shutil.which", {"return_value": "inspection-tool"}),
            ("platform.release", {"return_value": "example-kernel"}),
            ("Path.is_file", {"return_value": True}),
            ("run_tool", {"return_value": abi.subprocess.CompletedProcess([], 0, "example-kernel SMP", "")}),
        ):
            mock = patch.object(abi, target, **kwargs) if "." not in target else patch("verify_abi." + target, **kwargs)
            mock.start()
            self.addCleanup(mock.stop)
        self.layout = abi.parse_layout(SIMPLE, "example")

    def test_all_types_compare_to_unchanged_live_modules(self):
        with patch.object(abi, "read_layout", return_value=self.layout) as read:
            result = abi.verify(Path("candidate.ko"))
        self.assertEqual(len(result), 19)
        self.assertEqual(len(read.call_args_list), 38)
        live_paths = [call.args[1].name for call in read.call_args_list if len(call.args) == 3]
        self.assertEqual(set(live_paths), {"snd_hda_codec_generic"})

    def test_missing_live_type_fails_even_if_candidate_has_it(self):
        def read(name, path, base=None):
            if base:
                raise abi.VerificationError("Type unavailable")
            return self.layout
        with patch.object(abi, "read_layout", side_effect=read), self.assertRaisesRegex(
                abi.VerificationError, "no complete layout in live HDA BTF"):
            abi.verify(Path("candidate.ko"))

    def test_wrong_kernel_is_rejected_before_layouts(self):
        result = abi.subprocess.CompletedProcess([], 0, "different-kernel SMP", "")
        with patch.object(abi, "run_tool", return_value=result), patch.object(abi, "read_layout") as read:
            with self.assertRaisesRegex(abi.VerificationError, "does not target the running kernel"):
                abi.verify(Path("candidate.ko"))
        read.assert_not_called()


if __name__ == "__main__":
    unittest.main()
