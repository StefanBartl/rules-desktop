# Releasing

Nothing is released yet: the hub is a placeholder and there is no release workflow. This page fixes
the rules the first release follows, so they are not decided under time pressure.

## Rules

- A release is a deliberate act that ends with a person trying the result; a machine only prepares a
  **draft**, it never publishes.
- Main is green (every job of the `ci` workflow) and `engine.lock` is the pin you mean.
- The version is bumped in `crates/rules-hub/Cargo.toml`, in its own commit. The tag is `v` plus
  exactly that version.
- A published release is never cut again; a mistake is fixed in the next patch version.
- Binaries are built per system (Windows, Linux, macOS) after the interface (`ui`, `pnpm build`),
  with a `SHA256SUMS` file. The workflow is modelled on the one of
  [tasks-desktop](https://github.com/StefanBartl/tasks-desktop) and arrives with the hub.

## The engine pin

`engine.lock` names the commits of `rules.nvim` and `lib.nvim` this app is built and tested
against. CI checks both out at exactly these commits, so a typo in a pin fails the build. Moving
a pin is its own commit, with the specs green against the new engine.

## Shared kit (`appkit`)

The parts shared with tasks-desktop (server guard, sessions, shell, design tokens) move into the
repository `appkit` (tasks `appkit-rust-herausloesen` and `appkit-ui-kit-herausloesen`). Until it
exists, nothing here depends on it. Once it does:

- the Rust parts come in as a **Git dependency with a tag** (never a branch), so `Cargo.lock` pins
  the exact commit; `cargo deny` allows that one Git source by name in `deny.toml`;
- the interface parts come in by the same rule, as a tagged Git dependency in `ui/package.json`;
- a change in `appkit` is released there first, then taken over here by raising the tag, in its
  own commit.

## Not yet

Code signing (Authenticode, notarisation), the installer of the native shell (Tauri), and the
updater key are separate decisions and arrive with the `shell` phase.
