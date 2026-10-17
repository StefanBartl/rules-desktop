# rules-desktop

A local web UI and desktop app for the rule set of [rules.nvim](https://github.com/StefanBartl/rules.nvim):
browse the rules, search them, and later create, change, retire and move them, in the browser first and as a small
native app after that. The sibling of [tasks-desktop](https://github.com/StefanBartl/tasks-desktop), built on the same
parts and meant to look and behave like it.

**Status: planning. Nothing is built and nothing is released.** This repository holds the project description so far;
the code follows the phases below. Do not expect a download.

## What it is for

`rules.nvim` loads a rule set from Markdown files, where every rule is a fenced `rule` block with the prose under it,
and checks a project against it. It is an engine with no rules of its own. What it does not have yet is a way to *write*
rules: find the next free id of a family, add a block without touching the text around it, retire a rule without ever
reusing its id. That is what `tasks.nvim` has for tasks, and this app is the window onto it.

It is **not** a place to run the checks or to let an agent work the manual rules: that stays in
[docmap-desktop](https://github.com/StefanBartl/docmap-desktop). This app is about the rule set itself.

## Principles

- **The engine is the only writer.** The app never parses or formats a rule block. It talks to rules.nvim through
  versioned documents (`rules-export/1` to read, `rules-ops/1` to write), the same shape tasks.nvim uses for tasks.
- **The text around a rule is not touched.** Writing replaces exactly one block; everything else in the file stays
  byte for byte. A block that holds code (a function, a comment) is shown, not rewritten.
- **Loopback only, token always.** Host and Origin are checked, there is no data endpoint without a session.
- **Data classes fail closed.** A rule set source without an entry in the per-device configuration is treated as not
  released; public and private rule sets are never mixed.
- **A rule set is code.** The app never runs a rule's `check`, and rule text is shown as text.
- rules.nvim stays a pure Lua plugin. This repository holds the hub, the UI and the native shell.

## Plan

| Phase | What |
| --- | --- |
| format | decide the storage format by measuring a round trip over every rule block of the real rule sets |
| lib, engine | the shared contract modules in lib.nvim, then `rules-export/1` and `rules-ops/1` in rules.nvim |
| nvim | `:Rules new`, `set`, `retire`, `move`, `list` in Neovim, useful without this app |
| shared | the parts this app has in common with tasks-desktop (server guard, sessions, shell, design tokens) move into one shared kit |
| ui-read | the hub and a read-only interface: families, rules, findings, search |
| ui-write | create, change, retire and move rules, conflict center, undo |
| shell | a thin native shell (Tauri), installer |
| waiver | waivers and coverage |

The planning notes live outside this repository, in the maintainer's task vault.

## Layout (planned)

```
crates/rules-hub/   the hub: routes, engine method list, adapters, on appkit (see below)
ui/                 the browser interface (Svelte 5, Vite, TypeScript)
e2e/                browser tests against the real binary and the real engine, with a synthetic rule set
src-tauri/          the native shell (later)
docs/               usage, security, releasing
engine.lock         the rules.nvim and lib.nvim commits this is built and tested against
```

## The shared kit

The server guard, sessions, pairing, limits and the login entry come from
[appkit](https://github.com/StefanBartl/appkit), the kit tasks-desktop is built on too. It is a git dependency pinned
by its tag in `Cargo.toml`, and the page takes the kit's browser part, `appkit-ui` (design tokens, base rules, menu bar,
markdown reader and more), from the same tag in `ui/package.json`. `tools/check-appkit-pin.py` checks that both lock
files hold one tag and one commit and that the tag still names it; `tools/check-color-literals.py` keeps every colour of
the page a token of the kit.
The hub already starts on it and passes the kit's security matrix (`crates/rules-hub/tests/security.rs`); it has its
own names (config folder `rules-desktop`, program `rules-hub`, port 17616, routes under `/api/rules/`) and one route
behind a session, `about`. Its commands and views come with the read-only UI phase.

To try a change of appkit before it has a tag, point the crates at a local checkout in a `.cargo/config.toml` that is
not committed (`[patch."https://github.com/StefanBartl/appkit"]`). That rewrites `Cargo.lock`: restore it with
`git checkout -- Cargo.lock` before a commit, then run the pin check.

## License

MIT, see [LICENSE](LICENSE).
