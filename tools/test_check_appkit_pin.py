"""Tests of check-appkit-pin.py: every way a pin can be wrong is refused, and only a tag that still
names the pinned commit passes. The remote is never asked: `remote_commit` and `subprocess.run`
are replaced.

    python -m unittest discover tools
"""

import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

SPEC = importlib.util.spec_from_file_location(
    "check_appkit_pin", Path(__file__).with_name("check-appkit-pin.py")
)
pin = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(pin)

SHA = "6111a9b9f78065d411dc502114b244ccf19357b2"
OTHER = "56df6b11ba66e1d9dd918a7284709bfb84e36dcd"
URL = "https://github.com/StefanBartl/appkit"


def lock(*sources):
    """A Cargo.lock with one appkit package per source, and one package from crates.io."""
    names = ["appkit-core", "appkit-hub", "appkit-testkit"]
    blocks = [
        '[[package]]\nname = "serde"\nversion = "1.0.0"\n'
        'source = "registry+https://github.com/rust-lang/crates.io-index"\n'
    ]
    for name, source in zip(names, sources):
        blocks.append(f'[[package]]\nname = "{name}"\nversion = "0.0.0"\nsource = "{source}"\n')
    return "version = 4\n\n" + "\n".join(blocks)


def tagged(tag="v0.1.0", sha=SHA, url=URL):
    return f"git+{url}?tag={tag}#{sha}"


class Pinned(unittest.TestCase):
    def test_a_tag_and_one_commit_is_a_pin(self):
        packages = list(pin.appkit_packages(lock(tagged(), tagged(), tagged())))
        self.assertEqual(len(packages), 3, "the crates.io package is not an appkit one")
        pins, problems = pin.pinned(packages)
        self.assertEqual(problems, [])
        self.assertEqual(pins, {("v0.1.0", SHA)})

    def test_a_branch_a_rev_or_no_tag_is_refused(self):
        for source in [
            f"git+{URL}?branch=main#{SHA}",
            f"git+{URL}?rev=6111a9b#{SHA}",
            f"git+{URL}?rev=6111a9b&tag=v0.1.0#{SHA}",
            f"git+{URL}#{SHA}",
        ]:
            with self.subTest(source=source):
                _, problems = pin.pinned(pin.appkit_packages(lock(source)))
                self.assertEqual(len(problems), 1, problems)
                self.assertIn("not by a tag", problems[0])

    def test_another_spelling_of_the_repository_is_refused(self):
        for url in [f"{URL}.git", f"{URL}/"]:
            with self.subTest(url=url):
                _, problems = pin.pinned(pin.appkit_packages(lock(tagged(url=url))))
                self.assertEqual(len(problems), 1, problems)
                self.assertIn("not a source of", problems[0])

    def test_packages_on_two_commits_are_refused(self):
        _, problems = pin.pinned(
            pin.appkit_packages(lock(tagged(), tagged(sha=OTHER), tagged()))
        )
        self.assertTrue(any("different commits" in p for p in problems), problems)


class Main(unittest.TestCase):
    def run_main(self, text, remote):
        with tempfile.TemporaryDirectory() as dir:
            path = os.path.join(dir, "Cargo.lock")
            with open(path, "w", encoding="utf-8") as f:
                f.write(text)
            argv = ["check-appkit-pin.py", "--lock", path]
            with mock.patch.object(sys, "argv", argv), mock.patch.object(
                pin, "remote_commit", side_effect=remote
            ), mock.patch("builtins.print"):
                return pin.main()

    def test_a_tag_that_still_names_the_commit_passes(self):
        self.assertEqual(self.run_main(lock(tagged(), tagged(), tagged()), [SHA]), 0)

    def test_a_moved_tag_fails(self):
        self.assertEqual(self.run_main(lock(tagged(), tagged(), tagged()), [OTHER]), 1)

    def test_a_tag_that_is_gone_fails(self):
        self.assertEqual(self.run_main(lock(tagged(), tagged(), tagged()), [None]), 1)

    def test_a_check_that_cannot_ask_is_no_pass(self):
        self.assertEqual(
            self.run_main(lock(tagged(), tagged(), tagged()), RuntimeError("no network")), 2
        )

    def test_a_lock_without_appkit_fails(self):
        self.assertEqual(self.run_main(lock(), [SHA]), 1)


class RemoteCommit(unittest.TestCase):
    def answer(self, stdout):
        done = subprocess.CompletedProcess(args=[], returncode=0, stdout=stdout, stderr="")
        return mock.patch.object(pin.subprocess, "run", return_value=done)

    def test_the_commit_of_an_annotated_tag_is_the_peeled_one(self):
        lines = f"225ac5e43900b0f95daa5163423c6bbb970fddf7\trefs/tags/v0.1.0\n{SHA}\trefs/tags/v0.1.0^{{}}\n"
        with self.answer(lines):
            self.assertEqual(pin.remote_commit("v0.1.0"), SHA)

    def test_a_lightweight_tag_names_its_commit_itself(self):
        with self.answer(f"{SHA}\trefs/tags/v0.1.0\n"):
            self.assertEqual(pin.remote_commit("v0.1.0"), SHA)

    def test_a_tag_that_is_not_there_is_none(self):
        with self.answer(""):
            self.assertIsNone(pin.remote_commit("v9.9.9"))


if __name__ == "__main__":
    unittest.main()
