<#
.SYNOPSIS
Install rules-hub for the current user on Windows, without administrator rights.

.DESCRIPTION
Downloads the release archive from GitHub, checks it against SHA256SUMS, and hands over to
`rules-hub install`, which copies the program to its fixed place (%LOCALAPPDATA%\rules-desktop\bin,
or $env:RULES_DESKTOP_INSTALL_DIR) in one step and brings a running hub back from there. Run it
again to repair or to update: an installation that is there is found and replaced, never a second
one beside it.

Run it in your own PowerShell, not elevated and not from inside another program's sandbox (a
packaged app redirects AppData and the registry, and the hub would be installed for that app only).

.PARAMETER Version
The release tag to install (default: the latest published one).

.PARAMETER Repo
Where the releases are (default: StefanBartl/rules-desktop).

.PARAMETER Archive
A release archive (.zip) you already have: nothing is downloaded. The checksum then comes from
-Sums, -Sha256, or a SHA256SUMS file next to the archive.

.PARAMETER Sums
A SHA256SUMS file ("<hash>  <file>" per line).

.PARAMETER Sha256
The expected SHA-256 of the archive.

.PARAMETER Rolling
Install the rolling build of main (the pre-release `latest`, a bare rules-hub.exe) instead of a
tagged release. It is also taken when the repository has no published release yet.

.PARAMETER Autostart
Start the hub with your login (the Run key of the current user).

.PARAMETER Shortcut
Put a link "Rules" on the desktop.

.PARAMETER AllowElevated
Install although the session is elevated (for a build machine; the program is then installed for the
administrator account, not for you).

.EXAMPLE
.\install.ps1 -Autostart -Shortcut
#>
[CmdletBinding()]
param(
    [string] $Version = '',
    [string] $Repo = 'StefanBartl/rules-desktop',
    [string] $Archive = '',
    [string] $Sums = '',
    [string] $Sha256 = '',
    [switch] $Rolling,
    [switch] $Autostart,
    [switch] $Shortcut,
    [switch] $AllowElevated
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Exit-Install([string] $Message) {
    throw "install.ps1: $Message"
}

# The program belongs to one user: elevated, it would be the administrator account.
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $AllowElevated -and $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Exit-Install 'do not run this elevated: it installs for one user, without administrator rights (-AllowElevated for a build machine)'
}
if ($Repo -notmatch '^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$') {
    Exit-Install '-Repo must be OWNER/NAME'
}

$work = Join-Path ([IO.Path]::GetTempPath()) ('rules-desktop-install-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
try {
    if ($Archive -eq '') {
        $cpu = $env:PROCESSOR_ARCHITECTURE
        if ($cpu -ne 'AMD64') {
            Exit-Install "no release for Windows on $cpu"
        }
        $triple = 'x86_64-pc-windows-msvc'
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        if ($Version -eq '' -and -not $Rolling) {
            Write-Output "looking for the latest release of $Repo"
            try {
                $latest = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -UseBasicParsing
                $Version = [string] $latest.tag_name
            }
            catch {
                if ($_.Exception.Response -and [int] $_.Exception.Response.StatusCode -eq 404) {
                    Write-Output 'no published release yet: taking the rolling build of main'
                    $Rolling = [switch] $true
                }
                else { throw }
            }
            if (-not $Rolling -and $Version -eq '') { Exit-Install 'no published release found (pass -Version TAG or -Rolling)' }
        }
        if ($Rolling) {
            $name = 'rules-hub-windows-x86_64.exe'
            $base = "https://github.com/$Repo/releases/download/latest"
        }
        else {
            # A tag is a plain word: it goes into a URL and a file name.
            if ($Version -notmatch '^[A-Za-z0-9._-]+$') {
                Exit-Install "the version has a character that is not allowed: $Version"
            }
            $name = 'rules-hub-{0}-{1}.zip' -f $Version.TrimStart('v'), $triple
            $base = "https://github.com/$Repo/releases/download/$Version"
        }
        Write-Output "downloading $name"
        Invoke-WebRequest -Uri "$base/$name" -OutFile (Join-Path $work $name) -UseBasicParsing
        try {
            Invoke-WebRequest -Uri "$base/SHA256SUMS" -OutFile (Join-Path $work 'SHA256SUMS') -UseBasicParsing
        }
        catch {
            if ($Rolling) { Exit-Install 'SHA256SUMS of the rolling build is missing (a build is running: try again in a few minutes)' }
            throw
        }
        $Archive = Join-Path $work $name
        $Sums = Join-Path $work 'SHA256SUMS'
    }
    elseif (-not (Test-Path -LiteralPath $Archive -PathType Leaf)) {
        Exit-Install "no such file: $Archive"
    }

    # The checksum: never skipped. A file you have already needs one of the three sources.
    $actual = (Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($Sha256 -eq '') {
        if ($Sums -eq '') {
            $beside = Join-Path (Split-Path -Parent (Resolve-Path -LiteralPath $Archive)) 'SHA256SUMS'
            if (Test-Path -LiteralPath $beside -PathType Leaf) { $Sums = $beside }
        }
        if ($Sums -eq '') { Exit-Install "no checksum for ${Archive}: give -Sums FILE or -Sha256 HEX" }
        if (-not (Test-Path -LiteralPath $Sums -PathType Leaf)) { Exit-Install "no such file: $Sums" }
        $file = Split-Path -Leaf $Archive
        foreach ($line in Get-Content -LiteralPath $Sums) {
            # Lines are "<hash>  <file>"; the file name must match whole.
            if ($line -match '^([0-9A-Fa-f]{64})\s+\*?(.+)$' -and $Matches[2] -eq $file) {
                $Sha256 = $Matches[1]
                break
            }
        }
        if ($Sha256 -eq '') { Exit-Install "$file is not listed in $Sums" }
    }
    $expected = $Sha256.ToLowerInvariant()
    if ($actual -ne $expected) {
        Exit-Install "the checksum of $Archive does not match (expected $expected, got $actual): nothing was installed"
    }
    Write-Output 'checksum ok'

    if ($Archive -like '*.exe') {
        $program = $Archive
    }
    else {
        # A path that leaves the folder it is unpacked into is refused before anything is written (the
        # Expand-Archive of Windows PowerShell 5.1 does not look).
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $zip = [IO.Compression.ZipFile]::OpenRead((Resolve-Path -LiteralPath $Archive).Path)
        try {
            foreach ($entry in $zip.Entries) {
                if ($entry.FullName -match '(^|[\\/])\.\.([\\/]|$)' -or [IO.Path]::IsPathRooted($entry.FullName)) {
                    Exit-Install "the archive holds a path that leaves its folder ($($entry.FullName)): nothing was installed"
                }
            }
        }
        finally {
            $zip.Dispose()
        }

        $unpacked = Join-Path $work 'unpacked'
        Expand-Archive -LiteralPath $Archive -DestinationPath $unpacked
        $program = Join-Path $unpacked 'rules-hub.exe'
        if (-not (Test-Path -LiteralPath $program -PathType Leaf)) {
            Exit-Install 'the archive holds no rules-hub.exe'
        }
    }
    $newVersion = (& $program --version)
    if ($LASTEXITCODE -ne 0) { Exit-Install 'the program in the archive does not run' }

    # An installation that is there: told, then replaced in place (repair or update).
    if ($env:RULES_DESKTOP_INSTALL_DIR) {
        $dir = $env:RULES_DESKTOP_INSTALL_DIR
    }
    else {
        $dir = Join-Path $env:LOCALAPPDATA 'rules-desktop\bin'
    }
    $existing = Join-Path $dir 'rules-hub.exe'
    if (Test-Path -LiteralPath $existing -PathType Leaf) {
        $oldVersion = (& $existing --version 2>$null)
        if ($LASTEXITCODE -ne 0) { $oldVersion = 'unreadable' }
        if ($oldVersion -eq $newVersion) {
            Write-Output "found $oldVersion in ${dir}: repairing it"
        }
        else {
            Write-Output "found $oldVersion in ${dir}: replacing it with $newVersion"
        }
    }

    # `install` copies beside the target, moves over it, and brings a running hub back (it never
    # leaves the hub down); its exit status is ours.
    $flags = @()
    if ($Autostart) { $flags += '--autostart' }
    if ($Shortcut) { $flags += '--shortcut' }
    & $program install @flags
    if ($LASTEXITCODE -ne 0) { Exit-Install 'rules-hub install failed (see above)' }

    Write-Output ''
    Write-Output "installed $newVersion in $dir"
    Write-Output "start it with: & '$existing' start --open"
}
finally {
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}
