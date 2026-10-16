#!/usr/bin/env python3
"""Fail when a locked crate version is younger than N days.

A freshly published version is the window in which a hijacked crate does
its harm (concept risk T7: a bad release was online for 86 minutes). Cargo
has no "minimum release age" setting, so this reads Cargo.lock and asks
crates.io when each registry version was published.

    python tools/check-lock-age.py [--min-days 7] [--lock Cargo.lock]

Exit 0: every version is old enough. Exit 1: the young ones are listed;
take an older version with `cargo update -p <crate> --precise <version>`.
Exit 2: the check itself could not run (network, API); that is not a pass.

crates.io asks for one request per second and an identifying User-Agent.
"""

import argparse
import json
import re
import sys
import time
import urllib.error
import urllib.request
from datetime import date, datetime, timezone

USER_AGENT = "rules-desktop-lock-age-check (https://github.com/StefanBartl/rules-desktop)"


def locked_registry_packages(lock_text):
    """Yield (name, version) of every package that comes from a registry."""
    for block in lock_text.split("[[package]]")[1:]:
        name = re.search(r'^name = "([^"]+)"', block, re.M)
        version = re.search(r'^version = "([^"]+)"', block, re.M)
        source = re.search(r'^source = "registry\+', block, re.M)
        if name and version and source:
            yield name.group(1), version.group(1)


def published(name, version):
    """The day a version was published, from the crates.io API."""
    url = f"https://crates.io/api/v1/crates/{name}/{version}"
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    for attempt in range(4):
        try:
            with urllib.request.urlopen(request, timeout=20) as response:
                text = json.load(response)["version"]["created_at"]
            return date.fromisoformat(text[:10])
        except urllib.error.HTTPError as err:
            if err.code == 429 and attempt < 3:
                time.sleep(5 * (attempt + 1))
                continue
            raise
    raise RuntimeError("unreachable")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--min-days", type=int, default=7)
    parser.add_argument("--lock", default="Cargo.lock")
    args = parser.parse_args()

    with open(args.lock, encoding="utf-8") as handle:
        packages = sorted(set(locked_registry_packages(handle.read())))
    today = datetime.now(timezone.utc).date()

    young = []
    for name, version in packages:
        try:
            age = (today - published(name, version)).days
        except Exception as err:  # noqa: BLE001 - any failure means "not checked"
            print(f"cannot check {name} {version}: {err}", file=sys.stderr)
            return 2
        if age < args.min_days:
            young.append((age, name, version))
        time.sleep(1.05)

    print(f"{len(packages)} registry crates checked, minimum age {args.min_days} days")
    for age, name, version in sorted(young):
        print(f"  too young: {name} {version} ({age} days)")
    return 1 if young else 0


if __name__ == "__main__":
    sys.exit(main())
