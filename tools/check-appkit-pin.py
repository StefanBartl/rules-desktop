#!/usr/bin/env python3
"""Fail when the appkit pin of Cargo.lock is not the commit its tag names now.

The hub depends on https://github.com/StefanBartl/appkit by a tag. Cargo.lock pins the commit the
tag named when the lock was written, and a build with --locked takes that commit whatever the tag
says later. A tag that was moved (by mistake, or by someone who got write access) would go
unnoticed. This reads every appkit package of Cargo.lock and asks the repository where its tag
points now.

    python tools/check-appkit-pin.py [--lock Cargo.lock]

Exit 0: every appkit package is pinned by a tag, all by the same tag and commit, the page pins the
kit's folder (`appkit-ui` in ui/pnpm-lock.yaml) to that same tag and commit, and the tag names that
commit. Exit 1: what is wrong is listed. Exit 2: the check itself could not run (git, network); that
is not a pass.

For Cargo.lock what check-lock-age.py is for crates.io: the one git source of the tree.
"""

import argparse
import re
import subprocess
import sys

APPKIT = "https://github.com/StefanBartl/appkit"
SOURCE = re.compile(
    r"^git\+(?P<url>[^?#]+)(?:\?(?P<query>[^#]*))?#(?P<sha>[0-9a-f]{40})$"
)


def appkit_packages(lock_text):
    """Yield (name, source) of every package whose source is the appkit repository."""
    for block in lock_text.split("[[package]]")[1:]:
        name = re.search(r'^name = "([^"]+)"', block, re.M)
        source = re.search(r'^source = "([^"]+)"', block, re.M)
        if name and source and source.group(1).startswith(f"git+{APPKIT}"):
            yield name.group(1), source.group(1)


def pinned(packages):
    """The one (tag, sha) of the packages, or the list of what is wrong."""
    problems = []
    pins = set()
    for name, source in packages:
        match = SOURCE.match(source)
        if not match or match.group("url") != APPKIT:
            problems.append(f"{name}: not a source of {APPKIT}: {source}")
            continue
        query = match.group("query") or ""
        tag = re.fullmatch(r"tag=([^&]+)", query)
        if not tag:
            problems.append(f"{name}: pinned by '{query or 'the default branch'}', not by a tag")
            continue
        pins.add((tag.group(1), match.group("sha")))
    if len(pins) > 1:
        problems.append(f"the appkit packages are pinned to different commits: {sorted(pins)}")
    return pins, problems


# pnpm writes the specifier as `github:StefanBartl/appkit#v0.3.0&path:ui-kit` (also when it was given as a
# git+https URL) and the resolved version as the tarball of the commit, `https://codeload.github.com/
# StefanBartl/appkit/tar.gz/<sha>#path:ui-kit`, with the peers in brackets after it.
UI_SPECIFIER = re.compile(
    r"^\s*specifier: (?:github:StefanBartl/appkit|git\+https://github\.com/StefanBartl/appkit(?:\.git)?)"
    r"#(?P<tag>[^&\s]+)&path:ui-kit\s*$",
    re.M,
)
UI_VERSION = re.compile(
    r"^\s*version: (?:https://codeload\.github\.com/StefanBartl/appkit/tar\.gz/(?P<sha>[0-9a-f]{40})#path:ui-kit"
    r"|git\+https://github\.com/StefanBartl/appkit(?:\.git)?#(?P<sha2>[0-9a-f]{40})&path:ui-kit)(?:\([^)]*\))?\s*$",
    re.M,
)


def ui_pin(lock_text):
    """The (tag, sha) the page pins the kit's folder to in pnpm-lock.yaml, or the list of what is wrong.

    The page takes `appkit-ui` from the folder `ui-kit` of the same repository, by the same tag as the
    hub takes the crates; a `link:` to a checkout, a branch or no kit at all is a mistake here.
    """
    problems = []
    block = re.search(r"^\s+appkit-ui:\n(?P<body>(?:\s+\S.*\n){1,3})", lock_text, re.M)
    if not block:
        problems.append("the page does not pin appkit-ui (ui/pnpm-lock.yaml has no entry)")
        return None, problems
    body = block.group("body")
    spec = UI_SPECIFIER.search(body)
    version = UI_VERSION.search(body)
    if not spec:
        problems.append(f"appkit-ui: not the folder ui-kit of {APPKIT} by a tag: {body.strip().splitlines()[0]}")
        return None, problems
    if not version:
        problems.append("appkit-ui: pnpm-lock.yaml pins no commit for it")
        return None, problems
    return (spec.group("tag"), version.group("sha") or version.group("sha2")), problems


def remote_commit(tag):
    """The commit the tag names now: the peeled commit of an annotated tag."""
    refs = [f"refs/tags/{tag}", f"refs/tags/{tag}^{{}}"]
    result = subprocess.run(
        ["git", "ls-remote", APPKIT, *refs],
        capture_output=True,
        text=True,
        timeout=60,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(f"git ls-remote failed: {result.stderr.strip()}")
    found = {}
    for line in result.stdout.splitlines():
        sha, ref = line.split("\t", 1)
        found[ref] = sha
    return found.get(refs[1]) or found.get(refs[0])


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--lock", default="Cargo.lock")
    parser.add_argument("--ui-lock", default="ui/pnpm-lock.yaml")
    args = parser.parse_args()
    with open(args.lock, encoding="utf-8") as f:
        packages = list(appkit_packages(f.read()))
    if not packages:
        print(f"no package of {APPKIT} in {args.lock}")
        return 1
    pins, problems = pinned(packages)
    with open(args.ui_lock, encoding="utf-8") as f:
        ui, ui_problems = ui_pin(f.read())
    problems.extend(ui_problems)
    # The page and the hub take one version of the kit: the same tag, and the commit it named for both.
    if ui is not None and pins and ui not in pins:
        problems.append(f"the page pins the kit to {ui}, the hub to {sorted(pins)}: one tag for both")
    if problems:
        for problem in problems:
            print(problem)
        return 1
    (tag, sha) = pins.pop()
    try:
        now = remote_commit(tag)
    except (OSError, RuntimeError, subprocess.SubprocessError) as err:
        print(f"the check could not run: {err}")
        return 2
    if now is None:
        print(f"the tag {tag} is not in {APPKIT}")
        return 1
    if now != sha:
        print(f"the tag {tag} names {now} now, Cargo.lock pins {sha}: the tag was moved")
        return 1
    names = ", ".join(sorted(name for name, _ in packages))
    print(f"ok: {names} and appkit-ui at {tag} = {sha[:12]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
