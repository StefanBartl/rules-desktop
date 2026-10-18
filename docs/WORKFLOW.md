# Working with rules-desktop

A walk through one sitting, from the start of the hub to the last rule you looked at. What each setting and variable
means is in [USAGE.md](USAGE.md); what the hub does about security is in [SECURITY.md](SECURITY.md). The page only
**reads** the rule set. Rules are written in Neovim (`:Rules new`, `set`, `retire`, `move`) or by hand in the vault; the
page follows the files and shows the change by itself.

## 1. Start

1. `rules-hub start` (once per login, or `rules-hub autostart on`). If it does not start, the log in the config folder
   says why: usually Neovim older than 0.12, or `RULES_VAULT` pointing at a folder without `rulesets/`.
2. `rules-hub open --launch` opens a one-time link in the browser. The page takes the pairing code out of the address
   at once and keeps the session in this tab only. A second tab needs a second `open`.
3. The status in the menu bar says **live** when the page follows the rule files.

## 2. Find out where things stand: Overview (`g h`)

The counters and the split by severity say how big the rule set is. Read the **findings** first, they are what needs
work:

- what the engine could not load (a broken rule block),
- gate families that have no rule,
- files that have no class on this device (see `sources.json`),
- retired rules.

The page does not run any check. That is for [docmap-desktop](https://github.com/StefanBartl/docmap-desktop).

## 3. Go through the families (`g f`)

One row per family with its counters. A row opens the list **narrowed to that family**.

## 4. Find a rule (`g l`)

- Filter by family, severity and kind of check (a rule without a check is its own entry).
- The search box (`/`) looks at titles at once and, when the hub has read them, at the texts too. A short note says when
  the hub is still reading, when a search failed, and when more rules matched than are listed.
- `j` and `k` move the cursor, `Enter` opens the rule. `Ctrl+K` is the palette: actions and rules in one box.

## 5. Read one rule

The rule opens beside the list: its text (as text, never as markup), its source file and line, the version of its file,
and its check and agent question as data. Hidden or reversing characters are shown as `U+` marks.

- **Copy** puts the file and line on the clipboard.
- **Open in Neovim** starts an editor on the machine the hub runs on, at the line. The hub asks the engine where the
  file is and refuses a file that lies outside the rule set the engine names.
- `Esc` closes the rule.

## 6. Back and Forward

Each view, and each rule you open, is an entry of the history of the tab. **Back** and **Forward** of the browser (or
the mouse buttons, `Alt+Left` and `Alt+Right`) walk through them: from a rule back to the list, from the list back to
the family table, and on. Filters, the search text and the cursor are not entries, so typing does not fill the history.

The address stays `http://127.0.0.1:<port>/` all the time: the places are kept in the history state of the tab, never
in the URL, so a copied address or a synced history carries no rule id and no pairing code.

## 7. Change a rule and see it

Edit or add a rule in Neovim or in the vault. The hub watches `<vault>/rulesets` and `.rules.json`; the list, the
counters and the search follow within a moment. A window that gets the focus back asks for a scan as well; **View >
Rescan** does it on demand. A rule that is open and disappears is closed with a note once the hub confirms it is gone.

## 8. What is shown as what

A rule set without an entry in `sources.json` is shown as **not classified**, and nothing unclassified or private is
released for anything that leaves the hub. Set the classes per device before you rely on that.

## 9. Stop

`rules-hub stop` ends the hub. `rules-hub autostart off` takes it out of the login, `rules-hub uninstall
[--purge-settings]` removes the installed program and, with the flag, the settings.
