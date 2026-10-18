# Working with rules-desktop

A walk through one sitting, from the start of the hub to the last rule you looked at. What each setting and variable
means is in [USAGE.md](USAGE.md); what the hub does about security is in [SECURITY.md](SECURITY.md). The page reads the rule
set and changes a rule through the engine (see section 5a); new rules are made in Neovim (`:Rules new`) or by hand in the
vault for now. The page follows the files and shows every change by itself.

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

## 3a. See how the rules apply (`g c`)

Coverage counts, per family and per gate, the rules with a check that runs by itself, those with only a question for an
agent, and the manual ones. A gate is a list of families in the configuration; its row sums the rules of those families
and names the families that match no rule. A row opens the list. The page runs no check: that is for docmap-desktop.

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

## 5a. Change a rule

For a rule that is a file of its own (one file per rule) in a rule set that has a class on this device, the open rule
has an **Edit** button: severity, title (one line), text and, when the check is data, check and agent as JSON (empty
takes it away). **Save** sends only what changed, with the version of the file you saw; the engine reads the result back
before it writes, and only those lines of the file move. A line that says *Saved.* offers **Undo**, which sends the
engine's own way back. If someone changed the file since you opened the rule, nothing is written and you are told so.

**Retire …** in the editor takes the rule out of the rule set (its id stays taken); the **Retired rules** list on the
overview has **Reinstate**. A rule in a document with many rules, one whose rule set has no class, and the check of a rule
whose check is a Lua function are shown and not written: the page says why.

**New rule …** (button in the rules view, or `Ctrl+K`) opens a dialog: the family (the families of the rule set are
offered), the next id of that family as the engine names it, title, severity, text, check and agent as JSON, and the name
of the rule set (empty: the first one, which may be a document rule set: name the one that has one file per rule).
**Preview** asks the engine for a dry run and shows what would be made and any warning (for example *pull before you
create*); **Create** writes it and opens the new rule. The id is not reserved while you type: the engine picks it under
the lock of the rule set when the rule is made.

**Several at once:** the boxes in the list mark rules (only those the page may write); the bar above the list sets one
severity for all marked rules or retires them, in a single request, each rule with the version you saw. One **Undo** takes
all of it back.

**Move …** in the editor puts the rule after or before another rule of the document that holds it (the file of the rule
is not touched).

**If the file changed meanwhile** the *Changed in the meantime* box opens and nothing is written. It shows the fields you
changed next to what the file says now. You choose: write your change onto the new version (the other change stays, only
the lines you changed move), look at the rule, or drop your change. The page never merges for you.

## 5b. Waivers (`g w`)

A waiver says that a finding is known and accepted for now. It is kept in the `.rules-waivers.json` of the project you set in
`config.json` (`project`), not in a rule set. The view lists the waivers with their reason, last day and paths, and marks the
expired ones and those for a rule that is not loaded. **Waive** takes a rule (the *Waive …* button of an open rule fills it), a
reason and, usually, a last day. **Renew …** gives a new last day or none; **Remove** drops it. *Saved.* offers **Undo**. The
engine writes the file as a whole with the keys sorted, so an untouched entry may move; a change made to the file by someone
else meanwhile is a clash and nothing is written.

## 5c. Data classes (`g s`)

The view lists the keys of `sources.json` with their class and the number of files each one holds, the class of a file no key
names, and the files that have no class at all. Pick another class in a row, **Take away** a key, or add a key with a class
(the name is the start of the source of the rule set; the files offer it). The hub writes the file in the config folder and
the classes hold at once, also for what is written. If the change would **release more** than before, the page asks first
(*Yes, release more* / *No*); tightening needs no question.

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
released for anything that leaves the hub. Set the classes per device (view `g s`, section 5c) before you rely on that.

## 9. Stop

`rules-hub stop` ends the hub. `rules-hub autostart off` takes it out of the login, `rules-hub uninstall
[--purge-settings]` removes the installed program and, with the flag, the settings.
