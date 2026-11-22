# VYRO development roadmap

## Stage 0 — Foundation ✅ done
- Architecture research and decision record (`ARCHITECTURE.md`)
- Rust core crate with 12 modules and 43 passing unit tests
- Tauri 2 multi-webview shell with real page rendering
- Original branding and icon set
- Honest status and limitations documentation

## Stage 1 — Core browsing polish (next)
- Wire find-on-page, zoom controls UI, fullscreen, and per-tab context menus
- Address-bar autocomplete backed by `history::search`
- Automatic session restore from the snapshot API
- Real download transfer wired to webview download events
- Keyboard shortcut audit and accessibility pass

## Stage 2 — Protection hardening
- Integrate a maintained Rust filter engine (e.g. `adblock-rust`) for full filter syntax
- Runtime download/update of EasyList, EasyPrivacy, and the malware-domain list
- Cosmetic filtering via injected scripts
- Enforce cookie policy and site permissions at the webview boundary
- Implement real fingerprinting mitigations and Safe Browsing service integration
- Encrypted password vault using the OS keychain

## Stage 3 — VPN
- First-party WireGuard backend (OS tunnel + firewall kill switch + DNS enforcement)
- Provider registry with server/country discovery
- Connection diagnostics and auto-reconnect state machine
- Never bundle credentials; keep env-var/credential-store design

## Stage 4 — Extensions decision point
- Evaluate a Chromium fork with the extension service for genuine Web Store support.
  This is the only honest path to the extension requirement; it changes the engine and
  is a major project of its own.
- Otherwise, define and document a first-party companion-extension format.

## Stage 5 — Platforms
- Android shell with `com.vyro.browser`, bottom navigation, swipe gestures
- iOS shell (requires macOS/Xcode)
- Linux packaging (deb/AppImage/Flatpak) and macOS notarization
- Performance profiling: startup, memory per tab, background throttling

## Stage 6 — Release quality
- End-to-end browser tests (Playwright driving the chrome UI)
- Crash reporting (opt-in), update mechanism, telemetry-free by default
- Security audit and reproducible builds
