# VYRO Browser — v0.1.0-beta

**Browse Beyond Limits.**

Windows desktop beta. This is a working foundation, not a finished Chrome/Brave replacement.

## What works
- Real webpage rendering (WebView2) with multiple tabs, back/forward, reload, home.
- Address bar with search-engine selection and HTTPS upgrades.
- Ad/tracker blocker with EasyList-style rules, exceptions, allowlisting, live counters.
- Privacy dashboard, site permissions, cookie policy, fingerprinting mitigations.
- Safe Browsing: bundled threat denylist + real Google Safe Browsing API when a key is set.
- Encrypted password vault (Argon2 + AES-256-GCM), reader mode, find-on-page, tab groups.
- Bookmarks (Netscape HTML import / JSON export), history, privacy controls.
- VPN integration with a configured provider CLI, diagnostics, and an app-level kill switch.
- VYRO companion scripts (first-party alternative to unsupported Chrome extensions).

## What does not work / is limited
- **VPN has no bundled provider.** You must configure a real tunneling command. VYRO refuses to simulate connections. Nothing is bundled or faked.
- Chrome Web Store extensions are not supported by the system webview engine.
- DNS leak protection is a settings flag; the kill switch gates VYRO only, not the OS.
- Downloads manager tracks state but does not yet perform the byte transfer.
- Android/iOS builds are not part of this Windows beta.

Full honesty: see `LIMITATIONS.md` in this folder.

## Install / run
1. Install the WebView2 Evergreen runtime if not already present.
2. Extract `vyro-browser.exe` anywhere and run it.
3. Close the last tab to close the browser.

Verify the binary with `SHA256SUMS.txt`.

Source: https://github.com/fanomdev/vyro-browser
License: MPL-2.0
