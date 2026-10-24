The hub as a program for each system, with the interface inside it. It reads the rule set and can make, change, retire, reinstate and move rules, with Undo and a conflict center; a rule set with no class on the device is never written.

**This is a draft.** Before it is published a person installs from it and clicks through the
result (install, start, repair, uninstall). A green build is not a tried program.

## Install

Without administrator rights, for one user. The `install` script of this release installs exactly
this release: it downloads the archive of your system, checks it against `SHA256SUMS` and hands
over to `rules-hub install`. Run it in your own terminal (PowerShell not elevated):

```
# Windows
powershell -ExecutionPolicy Bypass -c "irm https://github.com/StefanBartl/rules-desktop/releases/download/<tag>/install.ps1 | iex"

# Linux and macOS
curl -fsSL https://github.com/StefanBartl/rules-desktop/releases/download/<tag>/install.sh | sh
```

With the script saved: `.\install.ps1 -InstallArgs '--autostart','--shortcut'` or
`sh install.sh -- --autostart --shortcut` registers the hub with your login and adds a link. The
program downloaded by the script is not stopped by SmartScreen; what is checked instead is that it
is the file `SHA256SUMS` of this release lists.

Run the same script again to repair this release (to update, take the script of a newer release);
remove the program with `uninstall.sh` or `uninstall.ps1` (your settings stay unless you add
`--purge-settings` or `-PurgeSettings`; your vault and rule sets are never touched). The install
scripts come from the release-kit of appkit, the uninstall scripts from `scripts/` at this tag; all
are listed in `SHA256SUMS`.

## Files

| System | Archive |
| ------ | ------- |
| Windows (x86-64) | `rules-hub-<version>-x86_64-pc-windows-msvc.zip` |
| Linux (x86-64) | `rules-hub-<version>-x86_64-unknown-linux-gnu.tar.gz` |
| macOS (Apple silicon) | `rules-hub-<version>-aarch64-apple-darwin.tar.gz`, **built but not tried on a Mac** |

`SHA256SUMS` lists the checksum of every file (archives and scripts):

```
sha256sum -c SHA256SUMS --ignore-missing
```

The program is not signed yet: Windows may show SmartScreen, macOS may refuse to open it until
you allow it. The scripts do not need either.

The rules engine (`rules.nvim` and `lib.nvim`, Neovim 0.12 or newer) is not part of the archive.
