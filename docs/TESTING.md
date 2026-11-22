# Testing

## Core unit tests (executed)

The pure-Rust core is compiled and tested independently of the webview shell:

```
cargo test --manifest-path vyro-core/Cargo.toml
```

Result on rustc 1.100.0 (windows-gnu): **43 passed, 0 failed**.

Coverage by module:

| Module | Tests | What is verified |
|---|---|---|
| `adblock` | 10 | comment/header parsing, domain anchors, subdomain matching, lookalike rejection, `@@` exceptions, `$script` resource filter, `$third-party`, site allowlist, global toggle, per-domain stats, wildcard/`^` matcher |
| `tabs` | 3 | navigate + back/forward stacks, active-tab fallback on close, tab groups |
| `bookmarks` | 4 | CRUD/search/folders, update/remove, Netscape HTML import, JSON roundtrip |
| `history` | 3 | visit aggregation, search/most-visited, selective clear |
| `downloads` | 2 | lifecycle + progress fraction, cancel/clear |
| `engine` | 5 | bare domain → https, query → search URL, explicit scheme preserved, HTTPS upgrade, numeric input treated as search |
| `vpn` | 5 | refuses to simulate without provider, refuses missing command, server/country selection, real connect via configured command, diagnostics flag gaps |
| `extensions` | 3 | manifest parsing, install/toggle/remove lifecycle, honest support report |
| `privacy` | 3 | permission decision lifecycle, reset origin, HTTPS + cookie policy |
| `settings` | 2 | privacy-first defaults, serde roundtrip |
| `storage` | 2 | atomic JSON roundtrip, missing file |
| `platform` | 1 | data directory resolution |

## Shell compile checks

```
cargo check --manifest-path src-tauri/Cargo.toml
cargo build --manifest-path src-tauri/Cargo.toml
```

`cargo check` passes with no warnings. The full build links the WebView2-backed shell.

## Not yet tested

- End-to-end browser interaction (requires a GUI/WebView2 environment).
- Linux/macOS/Android/iOS builds (platform SDKs unavailable here).
- Real VPN provider connections (requires user-supplied provider configuration).
