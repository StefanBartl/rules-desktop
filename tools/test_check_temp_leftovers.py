"""Tests of check-temp-leftovers.py: what is counted, what --strict and --allow do, and that a temp dir
that cannot be read is no pass. CI runs it on the temp dirs of its jobs (.github/workflows/ci.yml).

    python -m unittest discover tools
"""

import contextlib
import importlib.util
import io
import os
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location("check_temp_leftovers", Path(__file__).with_name("check-temp-leftovers.py"))
leftover_check = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(leftover_check)


def run(argv):
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        try:
            code = leftover_check.main(argv)
        except SystemExit as e:
            code = e.code
    return code, out.getvalue(), err.getvalue()


class Leftovers(unittest.TestCase):
    def setUp(self):
        self._dir = tempfile.TemporaryDirectory(prefix="check-temp-leftovers-test-")
        self.dir = self._dir.name

    def tearDown(self):
        self._dir.cleanup()

    def make(self, *names):
        for name in names:
            os.mkdir(os.path.join(self.dir, name))

    def test_an_empty_dir_passes(self):
        code, out, _ = run(["--dir", self.dir])
        self.assertEqual(code, 0)
        self.assertIn("nothing", out)

    def test_the_folders_of_the_tests_fail_and_others_do_not_count(self):
        self.make("rules-desktop-cli-version-abc", "rules-hub-routes-hash-12", "nvim", "chrome_cache")
        code, _, err = run(["--dir", self.dir])
        self.assertEqual(code, 1)
        self.assertIn("2 rules-desktop-*, rules-hub-* left", err)
        self.assertIn("rules-desktop-cli-version-abc", err)
        self.assertNotIn("chrome_cache", err)

    def test_a_file_counts_like_a_folder(self):
        Path(self.dir, "rules-desktop-e2e-state-42.json").write_text("{}")
        self.assertEqual(run(["--dir", self.dir])[0], 1)

    def test_a_prefix_replaces_the_default_list(self):
        self.make("rules-desktop-x", "other-y")
        self.assertEqual(run(["--dir", self.dir, "--prefix", "other-"])[0], 1)
        self.assertEqual(run(["--dir", self.dir, "--prefix", "none-"])[0], 0)

    def test_strict_counts_every_entry_but_the_allowed_ones(self):
        self.make("nvim", "nvim.0")
        self.assertEqual(run(["--dir", self.dir, "--strict", "--allow", "nvim", "--allow", "nvim.0"])[0], 0)
        self.make("whatever-a-test-called-it")
        code, _, err = run(["--dir", self.dir, "--strict", "--allow", "nvim", "--allow", "nvim.0"])
        self.assertEqual(code, 1)
        self.assertIn("whatever-a-test-called-it", err)

    def test_many_are_listed_up_to_a_limit(self):
        self.make(*[f"rules-desktop-{i:02}" for i in range(35)])
        code, _, err = run(["--dir", self.dir])
        self.assertEqual(code, 1)
        self.assertIn("... and 5 more", err)

    def test_a_missing_dir_is_no_pass(self):
        code, _, err = run(["--dir", os.path.join(self.dir, "no-such")])
        self.assertEqual(code, 2)
        self.assertIn("cannot read", err)

    def test_wrong_arguments_are_no_pass(self):
        self.assertEqual(run(["--allow", "nvim"])[0], 2)
        self.assertEqual(run(["--strict", "--prefix", "x"])[0], 2)
        self.assertEqual(run(["--frobnicate"])[0], 2)


if __name__ == "__main__":
    unittest.main()
