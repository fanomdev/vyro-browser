# VYRO Browser

**Browse Beyond Limits.**

VYRO is a privacy-focused browser project with a **real Rust core** (`vyro-core`) and a
**Tauri 2 desktop shell** that renders pages in genuine system webviews (WebView2 on
Windows, WKWebView on macOS, WebKitGTK on Linux). It is not a screenshot mockup:
navigation, tab state, filtering, bookmarks, history, privacy controls and the VPN /
extension integration layers are implemented in Rust and covered by 43+ unit tests.

> **Read this first:** VYRO is a working *foundation / vertical slice*, not a finished
> Chrome/Brave replacement. No single agent can build a production rendering engine in
> one session. Everything that works is marked as such below; everything that does not
> is documented in [`docs/LIMITATIONS.md`](docs/LIMITATIONS.md). Nothing is faked.

---

## Feature status (honest)

| Area | Status | Notes |
|---|---|---|
| Real webpage rendering | ✅ Yes | System webview per tab via Tauri multi-webview (`unstable` API) |
| Multi-tab browsing, back/forward, reload, home | ✅ Yes | Rust `TabManager` + child webviews |
| Address bar: URL / search resolution | ✅ Yes | 5 search engines, autocomplete-ready core |
| Ad & tracker blocker | ✅ Core yes / ⚠️ engine-limited | Real EasyList-style parser + matcher; webview exposes only in-page + navigation-level interception |
| Per-site allowlisting, global toggle, stats | ✅ Yes | Live counters, top domains, recent blocks |
| Bookmarks (folders, search, import/export) | ✅ Yes | Netscape HTML import, JSON export |
| Browsing history | ✅ Yes | Aggregated visits, search, clear |
| Downloads | ✅ Working | Rust transfer engine: exact size, progress, pause/resume, resume after restart, configurable folder, "Show in folder" |
| Privacy dashboard | ✅ Yes | Protection stats, controls, permissions |
| Cookie policy, HTTPS upgrades, permissions | ✅ Yes | Core logic implemented and tested |
| Fingerprinting protection | ✅ In-page mitigations | Canvas/audio/WebGL/screen/navigator entropy reduction via injected script; not a security boundary |
| Safe Browsing | ✅ Local + real API | Bundled threat denylist plus Google Safe Browsing API v5 lookups when `VYRO_SAFE_BROWSING_KEY` is set |
| VPN integration | ✅ Layer / ⚠️ provider needed | Real connect/disconnect via configured provider CLI; **no simulated connections**, no bundled credentials |
| Kill switch | ✅ App-level | Browsing is gated when the VPN drops; **not** an OS firewall rule |
| DNS leak protection | ⚠️ Settings only | Requires provider-level DNS enforcement |
| Extension management | ⚠️ Inventory + companions | Manifest parsing/enable-disable plus VYRO companion scripts; **Chrome Web Store APIs are not supported by webview engines** |
| Dark/light themes, accent, customization | ✅ Yes | Applied live from settings |
| Password manager | ✅ Encrypted vault | Argon2 + AES-256-GCM; master password never stored, only ciphertext |
| Reader mode, find-on-page, tab groups UI | ✅ Implemented | Injected reader/find scripts; right-click a tab to group it |
| Session restore on startup | ⚠️ Partial | Snapshot API exists; auto-restore not wired |
| Android / iOS / Linux builds | ⚠️ Not produced here | Architecture supports Tauri mobile; requires macOS/Android SDK and platform testing |

---

## Features added in this release

- **Real multi-tab browsing** — every `+` / `Ctrl+T` opens a distinct tab; closing the last tab closes the window.
- **Privacy dashboard** with live blocked-request counters, allowlisting, custom EasyList-style filters and per-domain statistics.
- **Fingerprinting mitigations** — navigator, screen, canvas, audio, WebGL and timezone entropy reduction (in-page, best-effort).
- **Safe Browsing** — bundled threat denylist plus real Google Safe Browsing API v5 checks when `VYRO_SAFE_BROWSING_KEY` is set. No simulated verdicts.
- **Encrypted password vault** — Argon2 + AES-256-GCM; the master password is never stored.
- **Reader mode and find-on-page** — toolbar buttons plus `Ctrl+F`, with match navigation.
- **Tab groups** via right-click on a tab.
- **VPN Gate provider** — fetches the live public-server feed, decodes `.ovpn` configs, and launches a real OpenVPN process with country/server selection plus a bundled US sample fallback.
- **VPN integration** — connect/disconnect through a configured provider CLI, diagnostics, and an app-level kill switch. No private provider is bundled and no connection is simulated.
- **Tab strip** — horizontal wheel and drag scrolling, a live tab counter, and enter/exit animations when tabs are added or removed.
- **Companion extensions** — first-party scripts injected into matching pages, with an honest note that Chrome Web Store APIs are unavailable on webview engines.
- **Swiss/minimal dark UI** — flat surfaces, no decorative glows, visible focus rings, reduced-motion support, and a light theme.
- **Choose your start-page interface** — VYRO, Google-style, Brave-style or DuckDuckGo-style layouts, independent of the search-engine setting (original layouts, no copied logos).
- **Downloads can be cancelled** from the compact popover, alongside pause/resume.
- **Tab right-click menu** — group tabs, open the tab in your system browser, close the tab, or close VYRO; hovering a tab shows the browser's live RAM usage.
- **Unpacked Chrome extension import** — content scripts are run as VYRO companions; `chrome.*` APIs and background pages remain unsupported.

See `docs/LIMITATIONS.md` for the features that are still honest gaps.

## Architecture at a glance

```
┌──────────────────────────────────────────────────────────────┐
│  frontend/        Browser chrome UI (HTML/CSS/JS, no npm deps)│
│  index.html       tabs, toolbar, dashboards, settings         │
│  newtab.html      new-tab page                                │
└───────────────────────────┬──────────────────────────────────┘
                            │ Tauri IPC (`invoke`)
┌───────────────────────────▼──────────────────────────────────┐
│  src-tauri/       Rust shell — commands, child webviews,      │
│                  in-page blocker bridge, persistence          │
└───────────────────────────┬──────────────────────────────────┘
                            │ direct Rust calls
┌───────────────────────────▼──────────────────────────────────┐
│  vyro-core/       Pure Rust logic (no UI, fully testable)     │
│  adblock · tabs · engine · bookmarks · history · downloads    │
│  privacy · vpn · extensions · settings · storage · platform   │
└──────────────────────────────────────────────────────────────┘
```

Full rationale: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

---

## Memory behaviour

VYRO creates one real WebView2 renderer for the **active tab only**. Switching
tabs discards the previous tab's webview, and a tab left idle for 90 seconds is
discarded too. Any click, key or wheel event recreates it from its saved URL.
Ten open tabs therefore cost roughly one renderer, not ten. The tradeoff is that
background or idle tabs reload when revisited. Release builds are strongly
recommended over debug builds — they are far smaller and lighter at runtime.

## Installing on any drive

Download from the [releases page](https://github.com/fanomdev/vyro-browser/releases):

- **`vyro-browser-setup.exe`** — NSIS installer. Pick any drive/folder during setup. A `portable` marker is installed beside the EXE, so all browser data lives in `vyro-data` next to it — never forced onto C:.
- **`vyro-browser-portable.zip`** — unzip the whole folder to any drive and run `vyro-browser.exe`. Copy that folder to another computer and it works there too.

Both include `WebView2Loader.dll`. A bare EXE alone will not start.

## Quick start (Windows, this machine)

Rust is installed on drive `I:` because `C:` is full. Use:

```powershell
$env:RUSTUP_HOME='I:\rustup'
$env:CARGO_HOME='I:\cargo'
$env:PATH="E:\scoop\apps\mingw\current\bin;$env:PATH"

# Run the core test suite
cargo test --manifest-path E:\vyro-browser\vyro-core\Cargo.toml

# Type-check / build the desktop app
cargo build --manifest-path E:\vyro-browser\src-tauri\Cargo.toml
```

See [`docs/BUILDING.md`](docs/BUILDING.md) for prerequisites, packaging, and Android/iOS notes.

---

## Branding

Original VYRO mark and icon are in [`assets/vyro-logo.svg`](assets/vyro-logo.svg) and
`src-tauri/icons/` (generated by `assets/make_icons.py`). No Brave/Chromium/Chrome
assets are used. Package identifiers: desktop `vyro-browser`, Android `com.vyro.browser`.

## License

MPL-2.0. See [`LICENSE`](LICENSE).
