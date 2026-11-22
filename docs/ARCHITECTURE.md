# Architecture Decision Record — VYRO Browser

## 1. The problem

A browser that genuinely competes with Chrome/Brave needs four hard things:

1. a web platform renderer (HTML/CSS/JS, layout, JS engine),
2. a network stack it can intercept and filter,
3. an extension platform compatible with Chrome Web Store,
4. a cross-platform application shell.

Building (1) and (3) from scratch is a multi-year, large-team effort. The task also
requires Rust as the primary language and a deliverable that actually runs.

## 2. Engines considered

| Option | Real rendering | Chrome extensions | Rust-primary | Verdict |
|---|---|---|---|---|
| Build Blink/Gecko-class engine | ✅ | ❌ (would need extension platform too) | ❌ | Impossible in scope |
| Servo | ⚠️ partial | ❌ | ✅ | Not production-ready for general browsing |
| CEF (Chromium Embedded Framework) + Rust bindings | ✅ | ⚠️ limited, **not** Chrome Web Store compatible | ⚠️ | Heavy; extension promise still unmet |
| Tauri 2 / system webviews | ✅ | ❌ | ✅ | **Chosen** — real browsing, real Rust core, cross-platform |
| Electron | ✅ | ✅ | ❌ | Violates Rust-primary requirement |

## 3. Decision

**Tauri 2 with the multi-webview (`unstable`) API.**

- One OS window hosts a *chrome* webview (the UI) plus one *content* webview per tab.
  Each content webview is a real WebView2 / WKWebView / WebKitGTK instance, so pages
  actually render and run JavaScript.
- All browser logic lives in `vyro-core`, a dependency-free-of-UI Rust crate. It is
  compiled and unit-tested independently of Tauri.
- `src-tauri` is a thin command layer: it owns webview lifecycle, persistence, and the
  limited interception hooks the webview exposes.

### Why not claim extension compatibility?

System webviews do not implement the Chromium `chrome.*` extension APIs or the Chrome
Web Store. CEF's extension model is different and incompatible. Claiming otherwise
would be dishonest. VYRO therefore:

- parses and manages unpacked extensions (`manifest.json`) for real,
- shows an explicit support report in the UI,
- documents the Chromium-fork path needed for true compatibility.

See [`LIMITATIONS.md`](LIMITATIONS.md).

## 4. Data flow

```
User types in address bar
  → JS `invoke('navigate', { id, input })`
  → Rust `engine::resolve` (URL vs search) → engine
  → `privacy::apply_policies` (HTTPS upgrade)
  → `adblock::Blocker::check` (document-level decision)
  → `Webview::navigate(url)` (real load)
  → history visit + persistence + `vyro://state` event → UI refresh
```

Subresource requests are handled by an injected initialization script that rejects
`fetch`, `XMLHttpRequest`, and script/image/iframe `src` assignments to blocked
domains (see `init_script` in `src-tauri/src/lib.rs`). This is the only interception
surface system webviews expose. Full network-level blocking needs a renderer with a
network-delegation API (CEF/Chromium).

## 5. Module ownership

| Module | Responsibility |
|---|---|
| `engine` | URL normalization, search providers, navigation intents, HTTPS upgrade |
| `adblock` | Filter parsing, ABP-style matching, exceptions, allowlist, statistics |
| `tabs` | Tab state, per-tab back/forward stacks, groups, session snapshot |
| `bookmarks` | CRUD, folders, search, Netscape HTML import, JSON export |
| `history` | Visit aggregation, search, selective deletion |
| `downloads` | Download lifecycle/progress state machine |
| `privacy` | Permissions, cookie policy, HTTPS upgrade policy, data-clear categories |
| `vpn` | Provider config, CLI-backed connect/disconnect, diagnostics (no simulation) |
| `extensions` | Unpacked manifest parsing, inventory, honest support report |
| `settings` | User preferences with privacy-first defaults |
| `storage` | Atomic JSON persistence |
| `platform` | OS detection and data directories |

## 6. Security notes

- Settings/bookmarks/history are stored as JSON under the OS application-data folder.
- Private tabs are marked in state but browser data isolation is not yet implemented.
- VPN credentials are never stored: they are read from a named environment variable
  and passed to the provider subprocess as `VYRO_VPN_SECRET`.
- The chrome webview is the only origin granted Tauri IPC (see `capabilities/default.json`);
  remote page webviews are not granted IPC access.
- Initialization-script blocking is a convenience layer, not a security boundary.

## 7. Platform status

- **Windows**: desktop target implemented; requires WebView2 runtime.
- **Linux / macOS**: same Tauri code path; WebKitGTK / WKWebView need platform build deps.
- **Android**: Tauri v2 mobile is supported by the architecture; Gradle project and
  `com.vyro.browser` identifier are specified but not generated/tested here.
- **iOS**: needs macOS + Xcode; not feasible to produce from this environment.
