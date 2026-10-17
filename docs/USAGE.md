# Using rules-desktop

`rules-hub` is a small web server on your own machine that shows the rule set of
[rules.nvim](https://github.com/StefanBartl/rules.nvim) in the browser: families with their counters, the rules with
filters and a search over titles and texts, one rule with its file and its check, and what the engine could not load.
It only **reads**. Writing rules is for a later phase, and running the checks is for
[docmap-desktop](https://github.com/StefanBartl/docmap-desktop).

## What you need

- Neovim 0.12 or newer (`RULES_NVIM`, else the one on `PATH`).
- A checkout of rules.nvim (`RULES_ENGINE_DIR`, else `$REPOS_DIR/rules.nvim`) and of lib.nvim (`LIB_NVIM_DIR`, else
  `$REPOS_DIR/lib.nvim`).
- The rules vault: the folder whose `rulesets/` holds one folder per rule set (`RULES_VAULT`, or `vault` in
  `config.json`).

Each of these can also be a key of `config.json` in the config folder (`%APPDATA%\rules-desktop`,
`~/.config/rules-desktop`, `RULES_DESKTOP_CONFIG_DIR` moves it): `vault`, `nvim`, `engine_dir`, `port`.

## Start

```
rules-hub serve        # in the foreground, Ctrl-C stops it
rules-hub start        # in the background; the log is in the config folder
rules-hub open --launch   # a one-time link, opened in the default browser
rules-hub stop
```

The first `open` prints a link `http://127.0.0.1:17616/#pair=...`. The page takes the code out of the address at once,
trades it for a session and keeps the session in that tab only. A second tab needs a second `open`. The port is 17616
(`RULES_DESKTOP_PORT`, `--port`, or `port` in `config.json`); when it is taken the hub says so and uses a free one.

`rules-hub install [--autostart] [--shortcut]` copies the program to a fixed place; `autostart on|off|status` starts the
hub with your login; `uninstall [--purge-settings]` takes all of that away again (`scripts/uninstall.ps1` or
`scripts/uninstall.sh` then removes the program file).

## Which rule sets may be shown as what: `sources.json`

The file lives in the config folder, never in the vault (the vault is synced and changed by pull requests; a release
written there could be set by someone else). It says per device what each rule set is:

```json
{ "schema": 1,
  "default": "internal",
  "sources": { "lua": { "class": "internal" },
               "native": { "class": "private" } } }
```

A key is the name of a rule set as the engine writes it at the start of a rule's `source`. A file of a rule set with no
class is **unclassified**: it is still shown to you, marked as "not classified", and nothing unclassified or private is
released for anything that leaves the hub. A `sources.json` that cannot be read stops the start: a typo must not turn
into "everything unclassified" without anyone seeing it. The details (strictest key wins, the `#n` names) are in the
comment at the top of `crates/rules-hub/src/sources.rs`.

## The page

| View | What it shows |
| --- | --- |
| Overview | Counters, the split by severity, and the findings: what the engine could not load, gate families that have no rule, files without a class, retired rules. |
| Families | One row per family with its counters; a row opens the list narrowed to it. |
| Rules | The list, with filters for family, severity and kind of check, and a search over titles and, once the hub has read them, over the texts. |

A rule opens beside the list: its text as text, its file and line (copy, or **Open in Neovim**, which starts an editor on
the machine the hub runs on), the version of its file, and its check and agent question as data. A check is shown, never
run. The page follows the rule files: when one changes on disk, the list changes by itself, and a window that gets the
focus back asks for a scan.

Keys: `g h`, `g f`, `g l` switch the view; `j` and `k` move; `Enter` opens; `/` goes to the search; `Esc` closes;
`Ctrl+K` opens the palette (actions and rules in one box). The single keys can be switched off in the Help menu.
Language (English, German) and colours (as the system, light, dark) are in the menus; both are remembered in the browser.

## When something is wrong

| You see | It means |
| --- | --- |
| "This tab is not paired with the hub." | Run `rules-hub open` and open its link in this tab. |
| "The hub does not answer." | The hub is not running (`rules-hub start`) or runs on another port. |
| "The hub runs, but its engine is not ready: ..." | The word says why: `no_vault`, `nvim_not_found`, `engine_not_found`, `sources_invalid` ... Set the variables above. |
| Every rule says "not classified" | There is no `sources.json`, or it names no key for these rule sets. |
| "The hub is still reading the texts" | The first search comes a moment after the start; titles are searched meanwhile. |

Variables the hub reads: `RULES_DESKTOP_PORT`, `RULES_DESKTOP_CONFIG_DIR`, `RULES_VAULT`, `RULES_NVIM`,
`RULES_ENGINE_DIR`, `LIB_NVIM_DIR`, `RULES_HUB_UI_DIR` (a built `ui/dist` to serve instead of the one in the binary, for
development) and `RULES_HUB_ENGINE=process` (one engine process per question instead of a persistent one).
