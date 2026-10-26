The hub as a program for Windows, with the interface inside it. The program reads the rule set and can make, change, retire, reinstate and move rules, with Undo and a conflict center; a rule set with no class on the device is never written.

## Install

Without administrator rights, for one user. The `install.ps1` of this release installs exactly this release: it
downloads the archive, checks it against `SHA256SUMS` and hands over to `rules-hub install`. The repository is private,
so the script needs a GitHub login that can read it: save it from this page and run it in your own terminal
(PowerShell not elevated) with a token:

```
gh release download {tag} -R WKDSteve/rules-desktop -p install.ps1
$env:GH_TOKEN = (gh auth token); .\install.ps1
```

To register the hub with your login and add a link, pass the arguments: `.\install.ps1 -InstallArgs '--autostart','--shortcut'`.
The program downloaded by the script is not stopped by SmartScreen; what is checked instead is that it is the file
`SHA256SUMS` of this release lists.

Run the same script again to repair this release (to update, take the script of a newer release); remove the program
with `uninstall.ps1` (your settings stay unless you add `-PurgeSettings`; your vault is never touched). The install
script comes from the release-kit of appkit, the uninstall script from `scripts/` at this tag; both are listed in
`SHA256SUMS`.

## Files

| System | Archive |
| ------ | ------- |
| Windows (x86-64) | `rules-hub-<version>-x86_64-pc-windows-msvc.zip` |

There is no Linux or macOS build any more: the release is made on the author's Windows machine.

`SHA256SUMS` lists the checksum of every file (archive and scripts):

```
sha256sum -c SHA256SUMS --ignore-missing
```

The program is not signed yet: Windows may show SmartScreen. The script does not need that.

The rules engine (`rules.nvim`, Neovim 0.12 or newer) is not part of the archive.
