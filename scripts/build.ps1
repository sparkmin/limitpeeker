# Builds the product into target/dist: the controller, the TAP DLL and, with
# the .NET 10 SDK, the settings window, with the documents beside them.
# cargo fmt, check and test run first; -SkipTests builds only. A zip is made
# from it by package.ps1.
param([switch]$SkipTests)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
# A toolchain inside the checkout (.tools, never committed) comes first, then
# the cargo on PATH, as rustup installs it.
$tools = Join-Path $repo '.tools'
if (Test-Path -LiteralPath (Join-Path $tools 'cargo/bin/cargo.exe')) {
    $env:CARGO_HOME = Join-Path $tools 'cargo'
    $env:RUSTUP_HOME = Join-Path $tools 'rustup'
    $cargo = Join-Path $env:CARGO_HOME 'bin/cargo.exe'
} else {
    $cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
    if (-not $cargo) { throw 'Rust not found: install it with rustup (https://rustup.rs, MSVC toolchain), or put one in .tools' }
}
Write-Output "Rust: $cargo"
$manifest = Join-Path $repo 'Cargo.toml'
# The one version of the product, and the copyright line of its license, for
# the settings window's file.
$version = (Select-String -Path $manifest -Pattern '^version = "([^"]+)"' | Select-Object -First 1).Matches.Groups[1].Value
if (-not $version) { throw 'The version was not found in Cargo.toml' }
$copyright = (Select-String -Path (Join-Path $repo 'LICENSE') -Pattern '^Copyright' | Select-Object -First 1).Line
# Where the crates' sources are, to be named apart from this PC's path in the
# EXE's panic messages.
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$steps = New-Object System.Collections.Generic.List[object]
if (-not $SkipTests) {
    foreach ($step in 'fmt', 'check', 'test') { $steps.Add(@($step, '--manifest-path', $manifest)) }
}
$steps.Add(@('build', '--release', '--manifest-path', $manifest))
$previousFlags = $env:CARGO_ENCODED_RUSTFLAGS
try {
    # Scope static CRT to the product, including its test/build dependencies.
    # The paths of this checkout and of the crates become short names, so no
    # path of the PC that built it is left in the EXE; the last that matches
    # is used, and the crates may be inside the checkout.
    $env:CARGO_ENCODED_RUSTFLAGS = (@(
        '-C', 'target-feature=+crt-static',
        "--remap-path-prefix=$repo=.",
        "--remap-path-prefix=$cargoHome=cargo"
    ) -join [char]31)
    foreach ($arguments in $steps) {
        & $cargo @arguments
        if ($LASTEXITCODE -ne 0) { throw "cargo failed: $arguments" }
    }
} finally { $env:CARGO_ENCODED_RUSTFLAGS = $previousFlags }
# The shared-layout version lives in both sources. They once disagreed silently
# and broke session reuse, which no test covered because every run restarted
# Explorer. Fail the build instead of shipping that again.
$rustVersion = (Select-String -Path (Join-Path $repo 'src/xaml/channel.rs') -Pattern '^(?:pub\(super\) )?const VERSION: u32 = (\d+);').Matches.Groups[1].Value
$nativeVersion = (Select-String -Path (Join-Path $repo 'native/taskbar_tap.cpp') -Pattern '^constexpr LONG Version = (\d+);').Matches.Groups[1].Value
if (-not $rustVersion -or -not $nativeVersion) { throw 'Shared-layout version constant not found in one of the sources' }
if ($rustVersion -ne $nativeVersion) { throw "Shared-layout version mismatch: xaml/channel.rs=$rustVersion taskbar_tap.cpp=$nativeVersion" }
Write-Output "Shared-layout version check passed: $rustVersion"
$vswhere = 'C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe'
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw 'MSVC x64 Build Tools required' }
Import-Module (Join-Path $vs 'Common7/Tools/Microsoft.VisualStudio.DevShell.dll')
Enter-VsDevShell -VsInstallPath $vs -SkipAutomaticLocation -DevCmdArguments '-arch=x64 -host_arch=x64' | Out-Null
$out = Join-Path $repo 'target/dist'
New-Item -ItemType Directory -Force -Path $out | Out-Null
# Explorer pins the DLL until it exits, so relinking it fails while it is
# loaded. Labels and the look come from the EXE; rebuild the DLL only when
# its own sources changed, so a new EXE never needs an Explorer restart.
# The DLL is compiled in target/native, and only the DLL goes to the bundle;
# the object, export and import library files stay there.
$native = Join-Path $repo 'target/native'
New-Item -ItemType Directory -Force -Path $native | Out-Null
Remove-Item -Path (Join-Path $out 'taskbar_tap.obj'),(Join-Path $out 'limitpeeker_xaml.exp'),(Join-Path $out 'limitpeeker_xaml.lib'),(Join-Path $out 'limitpeeker_xaml.dll.source') -Force -ErrorAction SilentlyContinue
$tapSources = @('native/taskbar_tap.cpp', 'native/taskbar_tap.def') | ForEach-Object { Join-Path $repo $_ }
$tapHash = ($tapSources | ForEach-Object { (Get-FileHash -Algorithm SHA256 -LiteralPath $_).Hash }) -join ''
$tapDll = Join-Path $out 'limitpeeker_xaml.dll'
$tapStamp = Join-Path $native 'limitpeeker_xaml.dll.source'
if ((Test-Path -LiteralPath $tapDll) -and (Test-Path -LiteralPath $tapStamp) -and ((Get-Content -LiteralPath $tapStamp -Raw).Trim() -eq $tapHash)) {
    Write-Output 'TAP sources unchanged; keeping the built DLL'
} else {
    Push-Location $native
    try {
        # C++/WinRT would keep the source file's full path for its errors.
        # The flag is not in the hash above: a change of it alone would
        # rebuild the DLL Explorer holds.
        & cl.exe /nologo /std:c++20 /EHsc /W4 /O2 /MT /LD /utf-8 /DUNICODE /D_UNICODE /DNOMINMAX /DWINRT_NO_SOURCE_LOCATION (Join-Path $repo 'native/taskbar_tap.cpp') /Fe:limitpeeker_xaml.dll /link ("/DEF:" + (Join-Path $repo 'native/taskbar_tap.def')) ole32.lib oleaut32.lib runtimeobject.lib user32.lib advapi32.lib
        if ($LASTEXITCODE -ne 0) { throw 'TAP build failed' }
    } finally { Pop-Location }
    try {
        Copy-Item -LiteralPath (Join-Path $native 'limitpeeker_xaml.dll') -Destination $out -ErrorAction Stop
    } catch {
        throw 'Copying the TAP DLL failed. Explorer still has the old DLL loaded: stop the product and restart Explorer.'
    }
    # Written after the copy, so a failed copy is tried again next time.
    Set-Content -LiteralPath $tapStamp -Value $tapHash -Encoding ascii
}
# uninstall.exe runs limitpeeker.exe --remove beside it.
Copy-Item -LiteralPath (Join-Path $repo 'target/release/limitpeeker.exe'),(Join-Path $repo 'target/release/uninstall.exe') -Destination $out
foreach ($binary in @('limitpeeker.exe', 'uninstall.exe', 'limitpeeker_xaml.dll')) {
    $imports = & dumpbin.exe /dependents (Join-Path $out $binary)
    if ($LASTEXITCODE -ne 0) { throw "Dependency inspection failed: $binary" }
    if ($imports -match '(?i)\b(VCRUNTIME|MSVCP|CONCRT)\w*\.dll') {
        throw "Unexpected redistributable runtime dependency: $binary"
    }
    Write-Output "Static CRT dependency check passed: $binary"
}
# The settings window is .NET 10 WPF, built only when the .NET 10 SDK is
# installed; the widget runs without it. No telemetry and no first-run
# steps, and the project references no packages, so nothing is downloaded.
$env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
$env:DOTNET_NOLOGO = '1'
$env:DOTNET_SKIP_FIRST_TIME_EXPERIENCE = '1'
$env:DOTNET_CLI_WORKLOAD_UPDATE_NOTIFY_DISABLE = '1'
if ((Get-Command dotnet -ErrorAction SilentlyContinue) -and ((& dotnet --list-sdks) -match '^10\.')) {
    # The window is one file now; files of an earlier build are taken away
    # so none is left beside it.
    Remove-Item -Path (Join-Path $out 'LimitPeeker.Settings.*') -Force -ErrorAction SilentlyContinue
    # The product's version without the commit, and the sources' paths in
    # the debug information inside it as /_/.
    & dotnet publish (Join-Path $repo 'settings/LimitPeeker.Settings.csproj') -c Release -o $out --nologo `
        "-p:Version=$version" "-p:Copyright=$copyright" '-p:IncludeSourceRevisionInInformationalVersion=false' `
        "-p:PathMap=$repo=/_/"
    if ($LASTEXITCODE -ne 0) { throw 'Settings window build failed' }
    Write-Output "Settings window built: $(Join-Path $out 'LimitPeeker.Settings.exe')"
} else {
    Write-Output 'Settings window NOT built: the .NET 10 SDK is not installed'
}
# The license goes with every copy, as MIT asks.
Copy-Item -LiteralPath (Join-Path $repo 'README.md'),(Join-Path $repo 'LICENSE'),(Join-Path $repo 'THIRD_PARTY_NOTICES.md') -Destination $out
# The documents the README links to, with the pictures the README shows, and
# the note in icons/ on the logos, which are not included. Only that file is
# copied there: the logos someone put in icons/ stay.
$docs = Join-Path $out 'docs'
$images = Join-Path $docs 'images'
New-Item -ItemType Directory -Force -Path $images | Out-Null
Remove-Item -Path (Join-Path $docs '*.md'),(Join-Path $images '*.png') -Force -ErrorAction SilentlyContinue
Copy-Item -Path (Join-Path $repo 'docs/*.md') -Destination $docs
Copy-Item -Path (Join-Path $repo 'docs/images/*.png') -Destination $images
$icons = Join-Path $out 'icons'
New-Item -ItemType Directory -Force -Path $icons | Out-Null
Copy-Item -LiteralPath (Join-Path $repo 'assets/icons/README.txt') -Destination $icons
Write-Output "Built only, NOT executed: $out"
