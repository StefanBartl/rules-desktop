**This is not a release.** It is the newest state of `main`, rebuilt after every green CI run and replaced each time. Nothing here is signed or tested by hand; use a release for anything that matters. The program reads the rule set and can make, change, retire, reinstate and move rules, with Undo and a conflict center; a rule set with no class on the device is never written.

**Install it with the script** (for the current user, no administrator rights, and no SmartScreen stop: the script downloads the program itself and checks it against `SHA256SUMS`):

```
powershell -ExecutionPolicy Bypass -c "irm https://github.com/StefanBartl/rules-desktop/releases/download/latest/install.ps1 | iex"   # Windows
curl -fsSL https://github.com/StefanBartl/rules-desktop/releases/download/latest/install.sh | sh                                  # Linux x86-64
```

Run it again to update to the newest rolling build.

Or take the file yourself:

- **Windows:** `rules-hub-windows-x86_64.exe`. A double click starts the hub in the background and opens the browser (a second click finds the running hub). Windows SmartScreen warns about an unsigned program: "More info", then "Run anyway". `rules-hub.exe install --autostart --shortcut` copies it to a fixed place, starts it with your login and puts a link on the desktop.
- **Linux:** `rules-hub-linux-x86_64.tar.gz`. Unpack it and run `./rules-hub` (without an argument it does the same as the double click).
- `uninstall.ps1`, `uninstall.sh` remove it again; `SHA256SUMS` holds the checksums of all files.

The program needs **Neovim 0.12 or newer**, checkouts of `rules.nvim` and `lib.nvim` (the engine; found through `RULES_ENGINE_DIR` and `LIB_NVIM_DIR`, `REPOS_DIR`, or the settings file) and your rules vault (`RULES_VAULT`). Without them the page says what is missing. `rules-hub --version` names the commit this file was built from; `rules-hub stop` ends the hub. See `docs/USAGE.md` and `docs/WORKFLOW.md`.
