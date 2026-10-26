# Packs the bundle that scripts/build.ps1 built in target/dist into a zip for
# a release: target/package/LimitPeeker-<version>-win-x64.zip, its SHA-256
# and a draft of the release notes. Only the bundle's own files go in, in one
# LimitPeeker folder. It checks that the files are of this version, that no
# path of the PC that built them is inside them, and what the zip holds. It
# only packs; it never builds or runs anything. -Forbid names more paths that
# must not be inside.
param([string[]]$Forbid = @())
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$dist = Join-Path $repo 'target/dist'

$version = (Select-String -Path (Join-Path $repo 'Cargo.toml') -Pattern '^version = "([^"]+)"' | Select-Object -First 1).Matches.Groups[1].Value
if (-not $version) { throw 'The version was not found in Cargo.toml' }

# The bundle, as it stands in target/dist and in the zip's folder. The
# documents and pictures are the ones in docs/ now, so one taken away is not
# packed from an older build.
$names = @('limitpeeker.exe', 'limitpeeker_xaml.dll', 'uninstall.exe', 'LimitPeeker.Settings.exe',
    'README.md', 'LICENSE', 'THIRD_PARTY_NOTICES.md', 'icons/README.txt')
$names += Get-ChildItem -Path (Join-Path $repo 'docs') -Filter '*.md' | Sort-Object Name | ForEach-Object { "docs/$($_.Name)" }
$names += Get-ChildItem -Path (Join-Path $repo 'docs/images') -Filter '*.png' | Sort-Object Name | ForEach-Object { "docs/images/$($_.Name)" }
$missing = @($names | Where-Object { -not (Test-Path -LiteralPath (Join-Path $dist $_)) })
if ($missing.Count) { throw "Not in target/dist; run scripts/build.ps1 first (with the .NET 10 SDK for the settings window): $($missing -join ', ')" }

# Each program's file version is the product's, or target/dist is older
# than Cargo.toml.
function Four([string]$text) {
    $parts = @((($text -split '[-+]')[0]).Split('.'))
    while ($parts.Count -lt 4) { $parts += '0' }
    [version]($parts[0..3] -join '.')
}
foreach ($program in 'limitpeeker.exe', 'uninstall.exe', 'LimitPeeker.Settings.exe') {
    $found = (Get-Item -LiteralPath (Join-Path $dist $program)).VersionInfo.FileVersion
    if (-not $found -or (Four $found) -ne (Four $version)) {
        throw "$program is version '$found', not $version; build again with scripts/build.ps1"
    }
}
Write-Output "Version check passed: $version"

# The paths of the PC that built it: the user's folder, the same user's
# folder on another drive, this checkout and the crates' folder. The user's
# name alone is not looked for, as a short one is in every program.
$paths = New-Object System.Collections.Generic.List[string]
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
foreach ($path in @($env:USERPROFILE, $repo, $cargoHome) + $Forbid) {
    if ($path) { $paths.Add($path.TrimEnd('\', '/').Replace('/', '\')) }
}
if ($env:USERNAME) { $paths.Add("\Users\$($env:USERNAME)\") }
$latin1 = [System.Text.Encoding]::GetEncoding(28591)

# A file's text read three ways: bytes as they are, and UTF-16 from either
# byte; with / as \, so both spellings of a path are found.
function Readings([byte[]]$bytes) {
    $plain = $latin1.GetString($bytes)
    $odd = if ($bytes.Length -gt 1) { [System.Text.Encoding]::Unicode.GetString($bytes, 1, $bytes.Length - 1) } else { '' }
    @($plain, [System.Text.Encoding]::Unicode.GetString($bytes), $odd) | ForEach-Object { $_.Replace('/', '\') }
}

# The debug information .NET puts inside a program (an embedded portable
# PDB, "MPDB" and then deflated data). It keeps a document's path as its
# parts, each after its length, so a path is looked for that way.
function EmbeddedPdbs([byte[]]$bytes) {
    $text = $latin1.GetString($bytes)
    $at = $text.IndexOf('MPDB', [System.StringComparison]::Ordinal)
    while ($at -ge 0) {
        try {
            $size = [System.BitConverter]::ToInt32($bytes, $at + 4)
            if ($size -gt 0 -and $size -lt 64MB) {
                $memory = New-Object System.IO.MemoryStream($bytes, ($at + 8), ($bytes.Length - $at - 8))
                $deflate = New-Object System.IO.Compression.DeflateStream($memory, [System.IO.Compression.CompressionMode]::Decompress)
                $pdb = New-Object byte[] $size
                $read = 0
                while ($read -lt $size) {
                    $n = $deflate.Read($pdb, $read, $size - $read)
                    if ($n -le 0) { break }
                    $read += $n
                }
                $deflate.Dispose()
                if ($read -eq $size -and $latin1.GetString($pdb, 0, 4) -eq 'BSJB') { , $pdb }
            }
        } catch {
            # Not a PDB after all: the four letters were by chance.
        }
        $at = $text.IndexOf('MPDB', $at + 4, [System.StringComparison]::Ordinal)
    }
}
function AsParts([string]$path) {
    $out = New-Object System.Text.StringBuilder
    foreach ($part in $path.Split('\') | Where-Object { $_ }) {
        $utf8 = [System.Text.Encoding]::UTF8.GetBytes($part)
        [void]$out.Append([char]$utf8.Length).Append($latin1.GetString($utf8))
    }
    $out.ToString()
}

$traces = New-Object System.Collections.Generic.List[string]
foreach ($name in $names) {
    $bytes = [System.IO.File]::ReadAllBytes((Join-Path $dist $name))
    $readings = Readings $bytes
    foreach ($path in $paths) {
        foreach ($reading in $readings) {
            if ($reading.IndexOf($path, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
                $traces.Add("${name}: $path")
                break
            }
        }
    }
    foreach ($pdb in EmbeddedPdbs $bytes) {
        $text = $latin1.GetString($pdb)
        foreach ($path in $paths) {
            if ($text.IndexOf((AsParts $path), [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
                $traces.Add("$name (its debug information): $path")
            }
        }
    }
}
if ($traces.Count) {
    $traces | ForEach-Object { Write-Output "Path of this PC inside: $_" }
    throw "$($traces.Count) paths of this PC are inside the files; see scripts/build.ps1 for how they are taken out"
}
Write-Output "No path of this PC inside the $($names.Count) files"

# The zip, its entries named with / as zip files must be: Windows
# PowerShell 5.1's Compress-Archive may write \ instead.
$package = Join-Path $repo 'target/package'
New-Item -ItemType Directory -Force -Path $package | Out-Null
$zipName = "LimitPeeker-$version-win-x64.zip"
$zipPath = Join-Path $package $zipName
Remove-Item -LiteralPath $zipPath, "$zipPath.sha256", (Join-Path $package 'release-notes.md') -Force -ErrorAction SilentlyContinue
$stream = [System.IO.File]::Open($zipPath, [System.IO.FileMode]::CreateNew)
try {
    $zip = New-Object System.IO.Compression.ZipArchive($stream, [System.IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($name in $names) {
            [void][System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
                $zip, (Join-Path $dist $name), "LimitPeeker/$name", [System.IO.Compression.CompressionLevel]::Optimal)
        }
    } finally { $zip.Dispose() }
} finally { $stream.Dispose() }

# What the zip holds, read back.
$zip = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
try { $entries = @($zip.Entries | ForEach-Object { $_.FullName }) } finally { $zip.Dispose() }
$expected = @($names | ForEach-Object { "LimitPeeker/$_" })
if (Compare-Object ($entries | Sort-Object) ($expected | Sort-Object)) {
    throw "The zip holds $($entries -join ', ') instead of $($expected -join ', ')"
}
Write-Output "Zip check passed: $($entries.Count) files in LimitPeeker/"

$hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $zipPath).Hash.ToLowerInvariant()
# One line as sha256sum writes it, without a byte order mark.
[System.IO.File]::WriteAllText("$zipPath.sha256", "$hash  $zipName`n")
$notes = @"
LimitPeeker $version for Windows 11 x64.

1. Download ``$zipName``, open it, and drag the ``LimitPeeker`` folder to a place you will keep, such as ``C:\Tools``.
2. For the settings window, install the .NET Desktop Runtime 10 (x64): ``winget install Microsoft.DotNet.DesktopRuntime.10``.
3. Run ``limitpeeker.exe``. It is not code-signed, so Windows SmartScreen may ask first: choose **More info**, then **Run anyway**.

To update, close LimitPeeker and drag the new ``LimitPeeker`` folder onto the old one's place, replacing the files.

SHA-256 of the zip: ``$hash``
"@
[System.IO.File]::WriteAllText((Join-Path $package 'release-notes.md'), $notes.Replace("`r`n", "`n"))
Write-Output "Packed only, NOT published: $zipPath"
Write-Output "SHA-256: $hash"
