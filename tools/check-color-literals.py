#!/usr/bin/env python3
"""Fail when a colour is written anywhere in a page but in the tokens.

Every colour of a page comes from the tokens of appkit-ui (ui-kit/src/tokens.css): a hex colour or a colour
function written in a component looks right in one theme and wrong in the other, and does not move with the tokens
to the other apps. This reads the tracked .css and .svelte files under the given folders and refuses, in the value
of a declaration (`prop: value;`, also inside a style attribute):

  * a hex colour (#rgb, #rgba, #rrggbb, #rrggbbaa);
  * a colour function: rgb(), rgba(), hsl(), hsla(), hwb(), lab(), lch(), oklab(), oklch(), color().

A selector (`a:hover #abc {`), a comment and the <script> part of a component are not looked at. `color-mix()`
over tokens, `transparent` and `currentColor` pass. The same file runs in appkit and in every app:

    python tools/check-color-literals.py --root ui-kit/src --tokens ui-kit/src/tokens.css   (appkit)
    python tools/check-color-literals.py --root ui/src                                      (an app)

Exit 0: clean. Exit 1: the places are listed (file, line, literal). Exit 2: git could not be asked.
"""

import argparse
import re
import subprocess
import sys

SCRIPT = re.compile(r"<script\b[^>]*>.*?</script>", re.S | re.I)
CSS_COMMENT = re.compile(r"/\*.*?\*/", re.S)
HTML_COMMENT = re.compile(r"<!--.*?-->", re.S)
# A declaration: a property, a colon, and a value that ends where a declaration ends (`;`, `}`, or the quote of a
# style attribute). A selector ends with `{` and so is no declaration.
DECLARATION = re.compile(r"(?<![\w-])[a-zA-Z-]+\s*:\s*([^;{}<>\"']*)(?=[;}\"'])")
LITERAL = re.compile(
    r"#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{3,4})\b"
    r"|\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch|color)\("
)


def blank(match):
    """The text of a match with everything but its line feeds removed, so line numbers stay."""
    return re.sub(r"[^\n]", " ", match.group())


def problems(path, text):
    """The (line, literal) pairs of the colour literals of one file."""
    if path.endswith(".svelte"):
        text = SCRIPT.sub(blank, text)
        text = HTML_COMMENT.sub(blank, text)
    text = CSS_COMMENT.sub(blank, text)
    found = []
    for declaration in DECLARATION.finditer(text):
        start = declaration.start(1)
        for literal in LITERAL.finditer(declaration.group(1)):
            line = text.count("\n", 0, start + literal.start()) + 1
            found.append((line, literal.group()))
    return found


def checked(path, roots, tokens):
    """Is this tracked file one to read: a stylesheet or component under a root, and not the tokens?"""
    inside = any(path.startswith(root.rstrip("/") + "/") for root in roots)
    return inside and path.endswith((".css", ".svelte")) and path != tokens


def tracked_files():
    try:
        out = subprocess.run(["git", "ls-files", "-z"], check=True, capture_output=True).stdout
    except (OSError, subprocess.CalledProcessError) as error:
        print(f"cannot list the tracked files: {error}", file=sys.stderr)
        sys.exit(2)
    return [p.decode("utf-8", "surrogateescape") for p in out.split(b"\0") if p]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--root", action="append", default=[], help="a folder to read (repeatable)")
    parser.add_argument("--tokens", default="", help="the one file that may hold colours")
    args = parser.parse_args(argv)
    roots = args.root or ["ui/src"]
    bad = 0
    for path in tracked_files():
        if not checked(path, roots, args.tokens):
            continue
        try:
            with open(path, encoding="utf-8") as handle:
                text = handle.read()
        except (OSError, UnicodeDecodeError):
            continue
        for line, literal in problems(path, text):
            bad += 1
            print(f"{path}:{line}: {literal}")
    if bad:
        print(f"{bad} colour literal(s) outside the tokens: use a token of appkit-ui", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
