# Building

You need Windows x64, the MSVC C++ Build Tools, the Windows SDK (C++/WinRT) and the Rust MSVC toolchain. The settings
window is built only when the .NET 10 SDK is there. For Rust, `.tools/cargo` in the repository is used when there is one,
otherwise `cargo` on the PATH (installed with rustup).

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build.ps1
```

The script runs `cargo fmt` (which rewrites the sources' layout), `cargo check`, `cargo test`, the release build and the
native DLL build. With `-SkipTests` it skips the first three and only builds. Both Rust and C++ link the CRT statically,
and the VCRUNTIME/MSVCP dependencies are checked. It also checks that the version of the memory the EXE and the DLL share
is the same in both sources. It never starts LimitPeeker or restarts Explorer. The only Rust dependencies are the `windows`
and `windows-numerics` crates.

The version is the one in `Cargo.toml`; `build.rs` puts it in the two Rust programs' file properties, and the script
gives it to the settings window's build. The paths of the PC that
builds it (the checkout and the crates' folder) are left out of the programs.

The bundle is these files in `target/dist`. `cargo build` alone makes neither the DLL nor the settings window, so use the
script above.

- `limitpeeker.exe`
- `limitpeeker_xaml.dll` — needed in the same folder as the EXE
- `uninstall.exe` — runs `limitpeeker.exe --remove`
- `LimitPeeker.Settings.exe` — the settings window, built only when the .NET 10 SDK is there; it runs on the .NET Desktop
  Runtime 10
- `README.md`, `LICENSE`, `THIRD_PARTY_NOTICES.md`, `docs/*.md` and the README's pictures in `docs/images`; the README's
  link to the Releases page works only on GitHub

Explorer holds the DLL, so a build with changes to the DLL's source (`native/`) is made after closing LimitPeeker and
restarting Explorer. When the DLL's source has not changed, the script does not build the DLL again.

## The release zip

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package.ps1
```

After a build, this packs the bundle into `target/package/LimitPeeker-<version>-win-x64.zip`, with all the files above in
one `LimitPeeker` folder, and writes the zip's SHA-256 and a draft of the release notes beside it. Before packing it
checks that every program is of the version in `Cargo.toml` and that no path of this PC (the user's folder, the checkout,
the crates' folder) is inside the files, and after packing, that the zip holds exactly the bundle. It never builds,
runs or publishes anything. A release needs the settings window, so build with the .NET 10 SDK installed first.
`-Forbid <paths>` names more paths that must not be inside the files.

## Repository layout

| Path | Contents |
| --- | --- |
| `src/`, `native/`, `settings/`, `assets/` | The controller and `uninstall.exe` (Rust), the DLL inside Explorer (C++), the settings window (WPF), the program icon |
| `scripts/build.ps1` | Builds the product, into `target/dist` |
| `scripts/package.ps1` | Packs `target/dist` into the release zip, into `target/package` |
| `docs/` | The documentation the README points to |
