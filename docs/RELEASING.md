# Releasing

A release is a deliberate act that ends with a person trying the result. The workflow
[`release.yml`](../.github/workflows/release.yml) makes a **draft**; nothing is published by a
machine.

## What a release holds

| File | What |
| ---- | ---- |
| `rules-hub-X.Y.Z-x86_64-pc-windows-msvc.zip` | the program for Windows |
| `install.ps1` | the install script of appkit's release-kit, stamped for this tag: it installs exactly this release |
| `uninstall.ps1` | the script of `scripts/` at this tag |
| `SHA256SUMS` | the checksum of every file above |

(Earlier plans had a Linux and a macOS archive, `install.sh` and `uninstall.sh`; they need hosted runners,
which a private repository without a paid plan does not have. See *Private repositories and the runner*
below.)

The native shell with a Setup (`*-setup.exe`, and, if they return, `.dmg` and a Linux package) is the Tauri task; this
workflow gets a `bundle` job when it exists. The scripts stay in any case: they are the way for a
person without administrator rights or with a program that is not signed.

The rolling build `latest` ([`rolling.yml`](../.github/workflows/rolling.yml)) carries the same
script, stamped for `latest`, beside its Windows program. `rolling.yml` takes the actions `prepare`
and `publish` of the release-kit, `release.yml` takes `publish` (its own `check` decides), both by
the commit of the appkit tag that `Cargo.lock` pins; `tools/check-appkit-pin.py` fails when the two
disagree, so a raised appkit tag raises them too.

## Steps

1. **Main is green** (the `ci` workflow, all jobs), the engine pin `engine.lock` is the one you mean.
2. **Bump the version** in `crates/rules-hub/Cargo.toml`, in its own commit, and push it. The tag
   must be `v` plus exactly that version; the workflow refuses otherwise.
3. **Wait for the CI of that commit.** The workflow looks for a green `ci` run of the very commit
   that the tag points at and stops without one.
4. **Tag and push:** `git tag vX.Y.Z && git push origin vX.Y.Z`. The workflow
   - checks the tag, the version, that the commit is on `main`, the green CI, and that no published
     release of the tag exists (`check`),
   - builds the interface and then the program for Windows, packs it, unpacks it again in
     another folder to ask it for its version, and runs the install script of the release-kit and
     the uninstall script against it in a folder of their own (`build`, rule `NREL-12`),
   - makes the draft with the notes of `.github/release-notes.md` (`{tag}` in them becomes the tag; the
     notes stay on the page when it is published, so they say nothing about being a draft) and adds the
     archive, the stamped install script, the uninstall script and `SHA256SUMS` (`publish`).
5. **Open the draft on a clean user account.** The files of a draft can
   be downloaded only by people who may write to the repository (logged in, in the browser), so the
   script cannot fetch them by itself yet: download the archive, `SHA256SUMS` and
   `install.ps1` into one folder and run `install.ps1 -File <the archive>` there
   (the sums are found beside it). Start the hub, run it again (repair), run `uninstall.ps1` with and
   without `--purge-settings`. The download path of the script (no `-File`) is tried after
   publishing, once, with the line of the notes. A green build is not a program that was tried
   (`NREL-07`). Do this in a normal terminal of the user, not from an agent session (`WIN-03`). A
   build machine that runs elevated needs `-AllowElevated` (PowerShell) for the scripts; a user
   session does not and should not.
6. **Publish the draft** within about a day (`NREL-09`). If the tree moved on, delete the draft and
   the tag and cut again from `main`.
7. A **published release is never cut again** (`NREL-10`): a mistake is fixed in the next patch
   version.

## When something is wrong

- *the tag says X but Cargo.toml says Y*: bump the version (step 2) and tag again; delete the tag
  first (`git push origin :refs/tags/vX.Y.Z`).
- *no green CI run for the commit*: the tag points at a commit CI has not passed. Wait, or fix.
- *a published release exists already*: it is never cut again (step 7); make a patch release. A draft
  of the same tag is reused: its files and notes are replaced by this cut.
- *a file is missing from the sums*: the `build` job failed; read it, delete the draft, tag again.
- *nothing happens after the tag*: the runner is not running; `runners.ps1 status rules-desktop`.

## The rolling build

After every green `ci` run on `main`, `rolling.yml` builds the program of exactly that commit (the
interface first, then `cargo build --release --locked`) for Windows and replaces the files of
the pre-release `latest`. It is for looking at the newest state with a click, not a release: no
version tag, not signed. `rules-hub --version` names the commit (`rules-hub 0.0.0 (build abc1234)`).
Only the tip of `main` is built, and a newer tip replaces a build that is still running (a late CI run
of an older commit lands in a group of its own and builds nothing). The page of the release says what
it is: `.github/rolling-notes.md`.

## The engine pin

`engine.lock` names the commits of `rules.nvim` and `lib.nvim` this app is built and tested
against. CI checks both out at exactly these commits, so a typo in a pin fails the build. Moving
a pin is its own commit, with the specs green against the new engine.

## Shared kit (`appkit`)

The parts shared with tasks-desktop (server guard, sessions, shell, design tokens, and the release
kit) live in the repository [appkit](https://github.com/WKDSteve/appkit):

- the Rust parts come in as a **Git dependency with a tag** (never a branch), so `Cargo.lock` pins
  the exact commit; `cargo deny` allows that one Git source by name in `deny.toml`;
- the interface parts come in by the same rule, as a tagged Git dependency in `ui/package.json`;
- the actions of the release-kit in the two workflows are pinned by the full commit of the same tag;
- a change in `appkit` is released there first, then taken over here by raising the tag in all
  these places, in its own commit (`cargo update -p appkit-hub`, `cd ui && pnpm install`,
  `python tools/check-appkit-pin.py`).

## Private repositories and the runner

The repository is private and has no paid plan, so CI (`ci.yml`), the rolling build (`rolling.yml`) and the release
(`release.yml`) all run on the self-hosted Windows runner of the author (`runs-on: [self-hosted, windows]`, Git Bash
by its full path). Start it with `pwsh $env:NVIM_CONFIG_DIR\scripts\runners.ps1 ensure <repo>`; without it a push or a
tag stays "queued" (no red mark, discarded after 24 h). Set-up and traps: `tasks-vault/TOOLS/self-hosted-runner.md`.

- Only Windows is built and tested: there is no Linux or macOS archive and no `install.sh` any more.
- The private sources (the engine at the pinned commit of `engine.lock`, appkit through Cargo and pnpm) are read with
  the login of `gh` and Git's own credentials (`.github/actions/pnpm-auth` hands the token to pnpm for the length of
  one job). There is no token secret.
- `release-kit` of appkit runs on the runner from v0.23.1 on (Git Bash by its full path); the apps pin it by the
  commit of the tag that `Cargo.lock` pins.
- Installing from this private repository needs `GH_TOKEN` (`$env:GH_TOKEN = (gh auth token)`): see the notes of the
  release page and the release-kit README of appkit.

## Not yet

Code signing (Authenticode, notarisation), the installer of the native shell (Tauri), and the
updater key are separate decisions and arrive with the `shell` phase.
