"""Tests of check-color-literals.py: a colour in a declaration is found, in a .css file, in the style of a component
and in a style attribute; a selector, a comment, a script and the tokens pass.

    python -m unittest discover tools
"""

import importlib.util
import unittest
from pathlib import Path
from unittest import mock

SPEC = importlib.util.spec_from_file_location(
    "check_color_literals", Path(__file__).with_name("check-color-literals.py")
)
colors = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(colors)


class Found(unittest.TestCase):
    def test_hex_and_functions_in_a_declaration(self):
        css = ".a {\n  color: #fff;\n  background: rgb(0 0 0 / 0.45);\n  border-color: #1f5fbf80;\n}\n"
        self.assertEqual(
            colors.problems("ui/src/app.css", css),
            [(2, "#fff"), (3, "rgb("), (4, "#1f5fbf80")],
        )

    def test_every_colour_function(self):
        for fn in ["rgba", "hsl", "hsla", "hwb", "lab", "lch", "oklab", "oklch", "color"]:
            with self.subTest(fn=fn):
                found = colors.problems("ui/src/x.css", f".a {{ color: {fn}(1 2 3); }}")
                self.assertEqual(found, [(1, f"{fn}(")])

    def test_the_style_of_a_component_and_a_style_attribute(self):
        svelte = (
            "<script>\n  const c = '#fff';\n</script>\n"
            '<div style="color: #abc">x</div>\n'
            "<style>\n  .a { box-shadow: 0 6px 20px rgb(0 0 0 / 0.25); }\n</style>\n"
        )
        self.assertEqual(colors.problems("ui/src/views/X.svelte", svelte), [(4, "#abc"), (6, "rgb(")])

    def test_the_last_declaration_of_a_block_without_a_semicolon(self):
        self.assertEqual(colors.problems("ui/src/x.css", ".a { color: #123 }"), [(1, "#123")])


class Passed(unittest.TestCase):
    def test_tokens_and_their_mixes(self):
        css = ".a { color: var(--text); background: color-mix(in srgb, var(--accent) 14%, transparent); }"
        self.assertEqual(colors.problems("ui/src/x.css", css), [])

    def test_a_selector_with_a_hex_looking_id(self):
        css = "a:hover #add, #face { color: var(--text); }"
        self.assertEqual(colors.problems("ui/src/x.css", css), [])

    def test_comments_and_scripts(self):
        svelte = (
            "<!-- color: #fff; -->\n<script>\n  el.style.color = 'rgb(1 2 3)';\n</script>\n"
            "<style>\n  /* was color: #fff; */\n  .a { color: var(--text); }\n</style>\n"
        )
        self.assertEqual(colors.problems("ui/src/views/X.svelte", svelte), [])


class Files(unittest.TestCase):
    def test_the_stylesheets_under_a_root_and_not_the_tokens(self):
        roots, tokens = ["ui-kit/src"], "ui-kit/src/tokens.css"
        self.assertTrue(colors.checked("ui-kit/src/base.css", roots, tokens))
        self.assertTrue(colors.checked("ui-kit/src/MenuBar.svelte", roots, tokens))
        self.assertFalse(colors.checked("ui-kit/src/tokens.css", roots, tokens))
        self.assertFalse(colors.checked("ui-kit/src/sse.ts", roots, tokens))
        self.assertFalse(colors.checked("ui-kit/srcx/a.css", roots, tokens))
        self.assertFalse(colors.checked("docs/style.css", roots, tokens))

    def test_main_reads_the_tracked_files_and_says_what_it_found(self):
        files = {"ui/src/a.css": ".a { color: #fff; }", "ui/src/b.css": ".b { color: var(--text); }"}
        opened = lambda path, encoding: mock.mock_open(read_data=files[path])()  # noqa: E731
        with mock.patch.object(colors, "tracked_files", return_value=list(files)), mock.patch(
            "builtins.open", side_effect=opened
        ), mock.patch("builtins.print") as printed:
            self.assertEqual(colors.main([]), 1)
        self.assertIn("ui/src/a.css:1: #fff", [c.args[0] for c in printed.call_args_list])
        with mock.patch.object(colors, "tracked_files", return_value=["ui/src/b.css"]), mock.patch(
            "builtins.open", side_effect=opened
        ):
            self.assertEqual(colors.main([]), 0)


if __name__ == "__main__":
    unittest.main()
