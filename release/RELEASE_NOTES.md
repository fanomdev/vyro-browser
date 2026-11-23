# VYRO Browser — v0.1.0 (Stable)

**Browse Beyond Limits.**

Windows desktop release. This is the first stable build; the earlier
`v0.1.0-beta` release remains available for reference.

Built in release/optimized mode: the EXE is ~26 MB (installer ~5 MB), and only
the active tab keeps a WebView2 renderer alive.

## Download the right file

- **`vyro-browser-setup.exe`** — installer. Choose any drive/folder during setup. Data is stored in a `vyro-data` folder next to the installed EXE, so it works on any drive.
- **`vyro-browser-portable.zip`** — extract the whole ZIP to any drive and run `vyro-browser.exe`. The `portable` marker makes VYRO keep its data in `vyro-data` beside the EXE. Copy the whole folder to another PC and it still works.

**Do not download only `vyro-browser.exe`.** It needs `WebView2Loader.dll` in the same folder. The ZIP and installer both include it.

## Fixed in v0.1.0

- Tab add/close no longer leaves the page area blank or unclickable. The active
  renderer is now restored whenever it is parked offscreen, and only one page
  renderer stays alive at a time.
- Rapid tab operations are serialized, so a late create cannot resurrect a
  renderer for a tab that was already closed.
- Closing the active tab now restores a working renderer for the tab that
  becomes active.
- The window can be dragged by holding the mouse anywhere on the top bar,
  including the tab strip and tabs. Clicking a tab's close button still closes
  it instead of starting a drag.

## What works

- Real webpage rendering (WebView2) with multiple tabs, back/forward, reload, home.
- Address bar with search-engine selection and HTTPS upgrades.
- Ad/tracker blocker with EasyList-style rules, exceptions, allowlisting, live counters.
- Privacy dashboard, site permissions, cookie policy, fingerprinting mitigations.
- Safe Browsing: bundled threat denylist + real Google Safe Browsing API when a key is set.
- Encrypted password vault (Argon2 + AES-256-GCM), reader mode, find-on-page, tab groups.
- Bookmarks, history, VPN provider integration with an app-level kill switch.
- Real downloads: choose any save folder, watch progress in a compact popover, and open the file in Explorer.
- VYRO companion scripts (first-party alternative to unsupported Chrome extensions).

## Honest limitations

- **VPN has no bundled provider.** Configure a real tunneling command; VYRO refuses to simulate connections.
- Chrome Web Store extensions are not supported by the webview engine.
- DNS leak protection is a flag; the kill switch gates VYRO only, not the OS.
- Downloads manager tracks state but does not perform the byte transfer yet.
- Android/iOS are not part of this Windows release.

Full details: `LIMITATIONS.md`.

Requires the WebView2 Evergreen runtime on Windows.
Source: https://github.com/fanomdev/vyro-browser — License: MPL-2.0
