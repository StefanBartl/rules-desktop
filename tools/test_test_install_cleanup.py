"""Tests of the cleanup of scripts/test-install.ps1: its temp folder is removed, a file held for a
moment is tried again, and a folder that cannot be removed is reported instead of left behind in
silence. The function Remove-TempFolder is taken out of the script (PowerShell's own parser) and
called on folders of this test. Windows only: an open file blocks its removal there.

    python -m unittest discover tools
"""

import os
import shutil
import subprocess
import tempfile
import threading
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent.parent / "scripts" / "test-install.ps1"

# Defines Remove-TempFolder from the script, calls it on TARGET and prints GONE or PROBLEM <text>.
DRIVER = r"""
$ErrorActionPreference = 'Stop'
$ast = [System.Management.Automation.Language.Parser]::ParseFile($env:SCRIPT, [ref] $null, [ref] $null)
$fn = $ast.Find({
    param($n)
    $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Remove-TempFolder'
}, $true)
if ($null -eq $fn) { Write-Output 'NO FUNCTION'; exit 3 }
. ([scriptblock]::Create($fn.Extent.Text))
$r = Remove-TempFolder -Path $env:TARGET -Attempts ([int] $env:ATTEMPTS) -DelayMs ([int] $env:DELAY_MS)
if ($null -eq $r) { Write-Output 'GONE' } else { Write-Output "PROBLEM $r" }
"""


def remove_temp_folder(target, attempts=3, delay_ms=50):
    """Run Remove-TempFolder of the script on `target`; its output, stripped."""
    pwsh = shutil.which("pwsh")
    if pwsh is None:
        raise AssertionError("pwsh is not on the PATH (the tests of test-install.ps1 need it)")
    env = dict(os.environ, SCRIPT=str(SCRIPT), TARGET=str(target), ATTEMPTS=str(attempts), DELAY_MS=str(delay_ms))
    done = subprocess.run(
        [pwsh, "-NoProfile", "-NonInteractive", "-Command", DRIVER],
        env=env,
        capture_output=True,
        text=True,
        timeout=60,
        check=False,
    )
    if done.returncode != 0:
        raise AssertionError(f"pwsh exit {done.returncode}: {done.stdout}{done.stderr}")
    return done.stdout.strip()


@unittest.skipUnless(os.name == "nt", "an open file blocks its removal only on Windows")
class RemoveTempFolder(unittest.TestCase):
    def setUp(self):
        self._dir = tempfile.TemporaryDirectory(prefix="rules-desktop-cleanup-test-")
        self.target = Path(self._dir.name) / "rules-desktop-test-install-x"
        (self.target / "bin").mkdir(parents=True)
        self.held = self.target / "bin" / "rules-hub.exe"
        self.held.write_text("x", encoding="ascii")

    def tearDown(self):
        self._dir.cleanup()

    def test_a_free_folder_is_removed(self):
        self.assertEqual(remove_temp_folder(self.target), "GONE")
        self.assertFalse(self.target.exists())

    def test_a_missing_folder_is_no_problem(self):
        shutil.rmtree(self.target)
        self.assertEqual(remove_temp_folder(self.target), "GONE")

    def test_a_folder_that_cannot_be_removed_is_reported(self):
        with open(self.held, "rb"):
            out = remove_temp_folder(self.target, attempts=3, delay_ms=50)
            self.assertTrue(self.target.exists())
        self.assertTrue(out.startswith("PROBLEM could not remove "), out)
        self.assertIn(str(self.target), out)
        self.assertIn("after 3 attempts", out)

    def test_a_file_held_for_a_moment_is_tried_again(self):
        handle = open(self.held, "rb")  # closed by the timer below
        # Longer than pwsh needs to start, so the first attempts meet the open file.
        timer = threading.Timer(2.0, handle.close)
        timer.start()
        try:
            out = remove_temp_folder(self.target, attempts=60, delay_ms=100)
        finally:
            timer.cancel()
            handle.close()
        self.assertEqual(out, "GONE")
        self.assertFalse(self.target.exists())


if __name__ == "__main__":
    unittest.main()
