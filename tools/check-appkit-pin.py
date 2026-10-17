#!/usr/bin/env python3
"""Fail when the appkit pin of Cargo.lock is not the commit its tag names now.

The hub depends on https://github.com/StefanBartl/appkit by a tag. Cargo.lock pins the commit the
tag named when the lock was written, and a build with --locked takes that commit whatever the tag
says later. A tag that was moved (by mistake, or by someone who got write access) would go
unnoticed. This reads every appkit package of Cargo.lock and asks the repository where its tag
points now.

    python tools/check-appkit-pin.py [--lock Cargo.lock]

Exit 0: every appkit package is pinned by a tag, all by the same tag and commit, and the tag names
that commit. Exit 1: what is wrong is listed. Exit 2: the check itself could not run (git, network);
that is not a pass.

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
    args = parser.parse_args()
    with open(args.lock, encoding="utf-8") as f:
        packages = list(appkit_packages(f.read()))
    if not packages:
        print(f"no package of {APPKIT} in {args.lock}")
        return 1
    pins, problems = pinned(packages)
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
    print(f"ok: {names} at {tag} = {sha[:12]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
