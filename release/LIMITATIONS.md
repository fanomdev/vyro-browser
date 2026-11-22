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
- The **kill switch is app-level**: when the VPN disconnects, VYRO gates all
  navigation until the user restores it. It is **not** an OS firewall rule and does
  not stop other applications from reaching the network.
- **DNS leak protection** remains a settings flag; enforcing it requires provider-level
  DNS routing — not implemented.
- No provider is bundled; no private keys or API keys are shipped.

## 4. Privacy / security features and their limits

- **Fingerprinting protection** now applies real in-page mitigations (navigator,
  screen, canvas, audio, WebGL, timezone). It is best-effort: a determined script can
  still find entropy, and the injected overrides are not a security boundary.
- **Safe Browsing** uses a bundled starter denylist and, when `VYRO_SAFE_BROWSING_KEY`
  is set, real Google Safe Browsing API v5 lookups. The bundled list is small — this
  is not the full Google threat feed. Without a key, verdicts are honestly `Unverified`.
- **Private browsing** is represented in tab state but does not isolate storage/cookies.
- **Secure password storage** is implemented as an encrypted vault (Argon2 + AES-256-GCM).
  The master password is never written to disk; the key lives only in memory. It does
  not integrate with OS credential stores or offer password sync.
- **Companions** are first-party VYRO scripts, not Chrome extensions.
- **Third-party cookie control** is core policy logic; it is not yet enforced by the
  webview's cookie store.
- **Site permission decisions** are stored and displayed, but camera/mic/location/
  notification requests are not yet intercepted at the webview boundary.

## 5. Browser features missing

- Downloads: lifecycle state exists; actual byte transfer is not wired to the webview
  download handler.
- Find-on-page (`Ctrl+F` or toolbar), reader mode, and a tab right-click menu with
  grouping are implemented. Zoom UI, fullscreen, swipe navigation, and automatic
  session restoration are still missing.
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
