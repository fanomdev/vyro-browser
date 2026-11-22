# Known limitations — read before trusting VYRO

This document exists so nobody mistakes the current build for a finished browser.
If a feature is not listed as working in `README.md`, assume it is not.

## 1. Extension compatibility (the big one)

- The system webview engines (WebView2, WKWebView, WebKitGTK, Android WebView) **do not
  implement Chrome's extension platform**.
- VYRO can parse `manifest.json`, list/enable/disable unpacked extensions, and show
  their permissions. It **cannot** load `chrome.*` APIs, background service workers,
  or content scripts, and it **cannot** install extensions from the Chrome Web Store.
- The UI states this explicitly. No fake extension listings are shown.
- **Path to real support:** fork Chromium with the extensions service enabled and build
  a Rust bridge to it (materially a separate multi-year project).

## 2. Ad/tracker blocking

What is real:
- EasyList/EasyPrivacy-style parser (`||domain^`, `@@` exceptions, `$third-party`,
  `$script`, wildcards), 20+ built-in rules, per-site allowlist, live statistics —
  all in `vyro-core` with unit tests.
- Navigation-level blocking in Rust before a page loads.
- An injected script blocks `fetch`/`XHR`/script/image/iframe loads to known domains.

What is not:
- System webviews do not expose general subresource request interception. Stylesheets,
  fonts, media, WebSockets, and statically declared subresources may not be blocked.
- No response-body filtering / cosmetic element hiding.
- Full EasyList/EasyPrivacy lists are not bundled (size/licensing); users add them via
  the Blocker panel.
- The injected bridge is not a security boundary; a page can delete/override it.

## 3. VPN

- There is **no VPN tunnel implemented inside VYRO** and no simulated connections.
- Connecting requires a provider configured with an external CLI command
  (`wg-quick`, `openvpn`, vendor CLI). With no provider or no command, `connect()`
  returns an error by design (unit-tested).
- Credentials come from a named environment variable; they are never persisted.
- **Kill switch** and **DNS leak protection** are settings/flags only. Enforcing them
  requires OS firewall and DNS changes by the provider backend — not implemented.
- No provider is bundled; no private keys or API keys are shipped.

## 4. Privacy / security features not yet real

- **Fingerprinting protection** is a settings flag only; no mitigations are applied.
- **Safe Browsing** is a settings flag only; no Google Safe Browsing API integration.
- **Private browsing** is represented in tab state but does not isolate storage/cookies.
- **Secure password storage** is not implemented.
- **Third-party cookie control** is core policy logic; it is not yet enforced by the
  webview's cookie store.
- **Site permission decisions** are stored and displayed, but camera/mic/location/
  notification requests are not yet intercepted at the webview boundary.

## 5. Browser features missing

- Downloads: lifecycle state exists; actual byte transfer is not wired to the webview
  download handler.
- Find-on-page, reader mode, zoom UI, fullscreen, context menus, swipe navigation,
  tab-group UI, and automatic session restoration are not implemented.
- Address-bar autocomplete is not wired to the UI (history query API exists).
- Import/export: JSON bookmarks export and Netscape HTML import work; Chrome/Firefox
  full-profile import does not.

## 6. Platform / build

- Built and type-checked on Windows with the GNU Rust toolchain. A Windows build needs
  the WebView2 runtime installed.
- Linux/macOS code paths exist but were not compiled in this environment
  (WebKitGTK / WKWebView system libraries required).
- Android/iOS deliverables are **not produced**. Tauri mobile supports them, but the
  Gradle/Xcode projects and device testing are outstanding.
- Tauri's multi-webview API is behind the `unstable` feature and can change between
  releases; the version is pinned via `Cargo.lock`.

## 7. Stability notes

- Opening very many tabs creates one webview per tab; memory use grows accordingly.
- Dropping a dropdown from the chrome webview over a content webview is constrained by
  webview boundaries; the current UI keeps dashboards in full-panel mode instead.
- The `unstable` `add_child` API should be called from async commands (done here) to
  avoid a WebView2 deadlock on Windows.
