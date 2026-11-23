# Building VYRO

## Prerequisites

### All platforms
- Rust (stable, 2021 edition). This project is tested with rustc 1.100.0.
- No npm/bundler is required — the frontend is plain HTML/CSS/JS with Tauri IPC.

### Windows
- WebView2 Runtime (preinstalled on Windows 11; otherwise install Evergreen runtime).
- A linker: MSVC Build Tools **or** MinGW-w64. This machine had no MSVC linker and no
  space on `C:`, so the GNU toolchain was installed on `I:`:

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu --profile minimal
rustup default stable-x86_64-pc-windows-gnu
rustup target add x86_64-pc-windows-gnu
```

### Linux
- `webkit2gtk-4.1`, `libayatana-appindicator3`, GTK3 dev packages, plus a C linker.

### macOS
- Xcode command line tools; WKWebView is provided by the OS.

## This machine (environment-specific)

`C:` has no free space, so Rust's home directories live on `I:` and MinGW is on `E:`.
Prefix every Cargo command with:

```powershell
$env:RUSTUP_HOME='I:\rustup'
$env:CARGO_HOME='I:\cargo'
$env:PATH="E:\scoop\apps\mingw\current\bin;$env:PATH"
```

## Commands

```powershell
# 1. Core logic + tests (fast, no webview deps)
cargo test --manifest-path E:\vyro-browser\vyro-core\Cargo.toml

# 2. Type-check the desktop shell
cargo check --manifest-path E:\vyro-browser\src-tauri\Cargo.toml

# 3. Build the desktop binary (release for distribution — much smaller/faster)
cargo build --release --manifest-path E:\vyro-browser\src-tauri\Cargo.toml
# Debug only, for development:
cargo build --manifest-path E:\vyro-browser\src-tauri\Cargo.toml

# 4. Install the Tauri CLI for running/bundling
cargo install tauri-cli --version '^2'

# 5. Run in development (from src-tauri)
cd E:\vyro-browser\src-tauri
cargo tauri dev

# 6. Produce installers
cargo tauri build
```

Output binaries land in `src-tauri/target/debug/` (dev) or
`src-tauri/target/release/bundle/` (installers: MSI/NSIS on Windows, .deb/AppImage on
Linux, .dmg on macOS).

## Packaging the Windows installer and portable ZIP (this machine)

The Tauri CLI is optional here — the release uses NSIS and 7-Zip directly.
`WebView2Loader.dll` must be packaged next to the EXE or the app will not start.

```powershell
$src = 'E:\vyro-browser\release\_installer_src'
Copy-Item E:\vyro-browser\src-tauri\target\debug\vyro-browser.exe "$src\vyro-browser.exe" -Force
Copy-Item E:\vyro-browser\src-tauri\target\debug\WebView2Loader.dll "$src\WebView2Loader.dll" -Force
& 'E:\scoop\shims\makensis.exe' "$src\installer.nsi"      # vyro-browser-setup.exe
& 'E:\scoop\shims\7z.exe' a -tzip 'E:\vyro-browser\release\vyro-browser-portable.zip' `
    "$src\vyro-browser.exe" "$src\WebView2Loader.dll" "$src\portable" "$src\README.txt"
```

The `portable` marker makes `platform::data_dir` use `<exe dir>\vyro-data` instead
of `%LOCALAPPDATA%`, so both formats run from any drive. The NSIS installer shows a
directory-selection page and writes the marker into the chosen folder.

## Regenerating icons

The icon set in `src-tauri/icons/` is generated from original geometry:

```powershell
python E:\vyro-browser\assets\make_icons.py
```

Requires Pillow. The source logo is `assets/vyro-logo.svg`.

## Android / iOS

The architecture targets Tauri v2, which supports both platforms, but the platform
projects are not included in this deliverable.

```bash
# Once Android SDK + NDK are configured:
cargo tauri android init
cargo tauri android build
# On macOS with Xcode:
cargo tauri ios init
cargo tauri ios build
```

 identifiers to use: `com.vyro.browser` (Android), `vyro-browser` (desktop).

## Troubleshooting

- **`link.exe` not found** → you are on the MSVC toolchain without Build Tools. Use the
  GNU toolchain commands above.
- **icon.ico not found** → run `make_icons.py`.
- **WebView2 errors at runtime** → install the WebView2 Evergreen runtime.
- **`add_child` missing** → the `unstable` feature must be enabled on `tauri`.
