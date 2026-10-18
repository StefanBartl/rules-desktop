**This is not a release.** It is the newest state of `main`, rebuilt after every green CI run and replaced each time. Nothing here is signed or tested by hand; use a release for anything that matters. The program reads the rule set and can change, retire and reinstate a rule; creating rules is not there yet.

- **Windows:** `rules-hub-windows-x86_64.exe`. A double click starts the hub in the background and opens the browser (a second click finds the running hub). Windows SmartScreen warns about an unsigned program: "More info", then "Run anyway". `rules-hub.exe install --autostart --shortcut` copies it to a fixed place, starts it with your login and puts a link on the desktop.
- **Linux:** `rules-hub-linux-x86_64.tar.gz`. Unpack it and run `./rules-hub` (without an argument it does the same as the double click).
- `SHA256SUMS` holds the checksums of both files.

The program needs **Neovim 0.12 or newer**, checkouts of `rules.nvim` and `lib.nvim` (the engine; found through `RULES_ENGINE_DIR` and `LIB_NVIM_DIR`, `REPOS_DIR`, or the settings file) and your rules vault (`RULES_VAULT`). Without them the page says what is missing. `rules-hub --version` names the commit this file was built from; `rules-hub stop` ends the hub. See `docs/USAGE.md` and `docs/WORKFLOW.md`.
