# The vilan installer for Windows — downloads the latest release into
# %USERPROFILE%\.vilan\bin (override with $env:VILAN_INSTALL_DIR):
#
#   irm https://github.com/vilan-lang/vilan/releases/latest/download/install.ps1 | iex
#
# Idempotent: re-running it updates in place. It only ever touches the install
# directory and the *user* PATH, so it needs no administrator rights.

$repo = 'vilan-lang/vilan'
$baseUrl = "https://github.com/$repo/releases/latest/download"
$binDir = if ($env:VILAN_INSTALL_DIR) {
    $env:VILAN_INSTALL_DIR
} else {
    Join-Path $env:USERPROFILE '.vilan\bin'
}

function Say([string] $message) {
    Write-Host $message
}

# `throw`, not `exit`: run the documented way (`irm … | iex`) this script shares
# the user's session, and `exit` would close the window — taking the message
# explaining what went wrong with it. A throw stops the script and prints.
function Fail([string] $message) {
    throw "install: $message"
}

# The released Windows target for this machine. There is no native arm64 build
# yet (windows-support.md §1 leaves it for when it is asked for), but Windows
# on ARM runs the x64 one under emulation — so that is what arm64 gets, said
# out loud rather than silently.
function Get-Target {
    $architecture = $env:PROCESSOR_ARCHITECTURE
    if ($env:PROCESSOR_ARCHITEW6432) {
        # A 32-bit shell on a 64-bit machine reports the shell, not the machine.
        $architecture = $env:PROCESSOR_ARCHITEW6432
    }
    switch ($architecture) {
        'AMD64' { return 'x86_64-pc-windows-msvc' }
        'ARM64' {
            Say "no native arm64 build yet — installing the x86_64 one, which Windows runs under emulation"
            return 'x86_64-pc-windows-msvc'
        }
        default { Fail "unsupported Windows architecture: $architecture (see https://github.com/$repo/releases)" }
    }
}

# The hash sha256sums.txt records for $name, or $null. The format is
# sha256sum's own: the hash, two spaces (or ' *' in binary mode), the name.
function Get-RecordedChecksum([string] $sumsPath, [string] $name) {
    foreach ($line in Get-Content -LiteralPath $sumsPath) {
        $fields = @($line -split '\s+' | Where-Object { $_ -ne '' })
        if ($fields.Count -eq 2 -and $fields[1].TrimStart('*') -eq $name) {
            return $fields[0]
        }
    }
    return $null
}

function Main {
    # Set here rather than at script scope: preference variables are
    # function-scoped, so an `iex`-run install cannot leave the user's session
    # in Stop mode after it finishes.
    $ErrorActionPreference = 'Stop'

    if ($PSVersionTable.PSVersion.Major -lt 5) {
        Fail "PowerShell 5.0 or newer is required (this is $($PSVersionTable.PSVersion))"
    }
    # Older Windows PowerShell defaults to TLS 1.0, which GitHub refuses.
    [Net.ServicePointManager]::SecurityProtocol =
        [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $asset = "vilan-$(Get-Target).zip"
    $workdir = Join-Path ([System.IO.Path]::GetTempPath()) "vilan-install-$([System.Guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Path $workdir | Out-Null
    try {
        Say "downloading $asset ..."
        try {
            Invoke-WebRequest -Uri "$baseUrl/$asset" -OutFile (Join-Path $workdir $asset) -UseBasicParsing
        } catch {
            Fail "download failed — https://github.com/$repo/releases"
        }
        try {
            Invoke-WebRequest -Uri "$baseUrl/sha256sums.txt" -OutFile (Join-Path $workdir 'sha256sums.txt') -UseBasicParsing
        } catch {
            Fail "download failed (sha256sums.txt)"
        }

        $expected = Get-RecordedChecksum (Join-Path $workdir 'sha256sums.txt') $asset
        if (-not $expected) {
            Fail "sha256sums.txt has no entry for $asset"
        }
        # -ne on strings is case-insensitive, which is what we want:
        # Get-FileHash writes upper-case hex, sha256sum lower-case.
        $actual = (Get-FileHash -LiteralPath (Join-Path $workdir $asset) -Algorithm SHA256).Hash
        if ($actual -ne $expected) {
            Fail "checksum mismatch for $asset — aborting (expected $expected, got $actual)"
        }

        New-Item -ItemType Directory -Path $binDir -Force | Out-Null
        # Windows refuses to overwrite a running executable but does allow it
        # to be renamed aside — the same dance `vilan upgrade` performs, so
        # re-running this while a vilan is up still updates in place. The
        # leftovers go on the next run, when nothing holds them open.
        foreach ($name in 'vilan.exe', 'vilan-lsp.exe') {
            $installed = Join-Path $binDir $name
            Remove-Item -LiteralPath "$installed.old" -Force -ErrorAction SilentlyContinue
            if (Test-Path -LiteralPath $installed) {
                Move-Item -LiteralPath $installed -Destination "$installed.old" -Force
            }
        }
        Expand-Archive -LiteralPath (Join-Path $workdir $asset) -DestinationPath $binDir -Force
    } finally {
        Remove-Item -Recurse -Force -LiteralPath $workdir -ErrorAction SilentlyContinue
    }

    $version = & (Join-Path $binDir 'vilan.exe') --version
    Say ""
    Say "installed $version to $binDir"

    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $entries = @($userPath -split ';' | Where-Object { $_ -ne '' })
    if ($entries -contains $binDir) {
        Say ""
        Say "verify with: vilan --version"
    } else {
        [Environment]::SetEnvironmentVariable('Path', (($entries + $binDir) -join ';'), 'User')
        $env:Path = "$env:Path;$binDir"
        Say ""
        Say "added $binDir to your user PATH — open a new terminal for other"
        Say "programs (and your editor) to see it. This session has it already."
        Say ""
        Say "verify with: vilan --version"
    }
    Say ""
    Say "get started: https://vilan-lang.github.io/vilan/"
}

Main
