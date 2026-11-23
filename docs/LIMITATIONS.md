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
- **VPN Gate** is integrated against the public Vpngate-Scraper-API feed; servers are
  third-party and may be unreliable or restricted by their operators. Creating the
  TUN adapter and routes needs administrator rights on Windows — if OpenVPN exits
  immediately, VYRO reports that instead of claiming a connection. ToS apply per server.
- No private provider, credentials, or API keys are bundled. The bundled US `.ovpn` is
  a public sample and may be stale; the live feed is preferred.

### Working source, with a local prerequisite

- **VPNBook** is integrated for real: public `.ovpn` configs and the current shared
  password are fetched at connect time (live-tested), and OpenVPN authentication
  succeeds. The remaining local requirement is an installed **TAP-Windows6 adapter**,
  which needs a one-time administrator install of OpenVPN. A plain extracted
  `openvpn.exe` will authenticate and then fail at `open_tun`. VYRO reports that
  failure honestly instead of showing "connected". Shared credentials are not for
  sensitive traffic.

### Providers evaluated and why they are not bundled

- **VPN Gate** (public feed): real OpenVPN configs, but tested servers returned
  `AUTH_FAILED` or never answered the auth stage — no tunnel was established.
- **RapidAPI `free-vpn` MCP**: its only tool returned "API is unreachable / API
  (not working)".
- **apis.io `vpn-server` schema**: a JSON Schema (data shape), not a service.
- **api-evangelist/vpn**: a documentation profile of NordVPN's public API, not a
  tunnel service.
- **Azure VPN Gateway API**: ARM management API requiring an Azure subscription and
  deployed gateway — infrastructure, not a consumer VPN.
- **linkartemy/outline-vpn-api**: a C++ server/key-management library, not a client.
- **VPNly**: proprietary app/extension service with no published API, `.ovpn` or
  WireGuard config. Integrating it would require reverse-engineering a private
  protocol, which VYRO does not do.

A usable provider must issue standard `.ovpn` or WireGuard config files, or expose a
documented client API. With those, VYRO's existing provider layer can connect for real.

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

## 5. Memory

- Each active page is a real WebView2 renderer. Even with hidden tabs discarded,
  WebView2's browser process plus one renderer typically occupies well over 100 MB,
  so a hard "100 MB for 10 pages" target is not achievable with genuine web pages.
- Background tabs are reloaded when revisited (their webview is destroyed on switch),
  and the active tab is discarded after 90 seconds of no input. Page state such as
  scroll position and unsubmitted form data is not preserved across that reload.
- Debug builds are much heavier than release builds; distributed binaries should be
  built with `--release`.

## 6. Browser features missing

- Downloads are transferred by VYRO's own Rust engine (WebView2's download is
  cancelled and replaced), so size and progress come from `Content-Length` and
  streamed bytes. Resume uses HTTP Range and depends on the server supporting it;
  some servers ignore Range and will restart the file.
- Find-on-page (`Ctrl+F` or toolbar), reader mode, and a tab right-click menu with
  grouping are implemented. Zoom UI, fullscreen, swipe navigation, and automatic
  session restoration are still missing.
- Address-bar autocomplete is not wired to the UI (history query API exists).
- Import/export: JSON bookmarks export and Netscape HTML import work; Chrome/Firefox
  full-profile import does not.

## 7. Platform / build

- Built and type-checked on Windows with the GNU Rust toolchain. A Windows build needs
  the WebView2 runtime installed.
- Linux/macOS code paths exist but were not compiled in this environment
  (WebKitGTK / WKWebView system libraries required).
- Android/iOS deliverables are **not produced**. Tauri mobile supports them, but the
  Gradle/Xcode projects and device testing are outstanding.
- Tauri's multi-webview API is behind the `unstable` feature and can change between
  releases; the version is pinned via `Cargo.lock`.

## 8. Stability notes

- Opening very many tabs creates one webview per tab; memory use grows accordingly.
- Dropping a dropdown from the chrome webview over a content webview is constrained by
  webview boundaries; the current UI keeps dashboards in full-panel mode instead.
- The `unstable` `add_child` API should be called from async commands (done here) to
  avoid a WebView2 deadlock on Windows.
