//! VYRO Browser — Tauri 2 shell.
//!
//! The Rust core (vyro-core) owns all logic. This crate wires it to a real
//! multi-webview browser window: a chrome webview for the UI plus one child
//! webview per tab for genuine page rendering.

use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Emitter, LogicalPosition, LogicalSize, Manager, Webview, WebviewUrl, Wry};
use vyro_core::adblock::{Blocker, ResourceType};
use vyro_core::bookmarks::BookmarkStore;
use vyro_core::downloads::DownloadManager;
use vyro_core::engine::{resolve, SearchEngine};
use vyro_core::extensions::{ExtensionManager, SupportReport};
use vyro_core::history::History;
use vyro_core::platform::data_dir;
use vyro_core::privacy::{Decision, Permission, PrivacyController};
use vyro_core::settings::Settings;
use vyro_core::tabs::TabManager;
use vyro_core::vpn::{VpnManager, VpnProviderConfig};
use vyro_core::APP_NAME;

/// Height of the browser chrome (tab strip + toolbar) in logical pixels.
const CHROME_H: f64 = 96.0;
type TabWebview = Webview<Wry>;

#[derive(Default)]
pub struct AppState {
    tabs: Mutex<TabManager>,
    blocker: Mutex<Blocker>,
    bookmarks: Mutex<BookmarkStore>,
    history: Mutex<History>,
    downloads: Mutex<DownloadManager>,
    extensions: Mutex<ExtensionManager>,
    privacy: Mutex<PrivacyController>,
    vpn: Mutex<VpnManager>,
    settings: Mutex<Settings>,
    webviews: Mutex<HashMap<u64, TabWebview>>,
    data_dir: Mutex<PathBuf>,
}

#[derive(Serialize)]
struct TabView {
    id: u64,
    title: String,
    url: String,
    active: bool,
    private: bool,
    group: Option<String>,
    loading: bool,
    can_back: bool,
    can_forward: bool,
}

fn tab_views(state: &AppState) -> Vec<TabView> {
    let tabs = state.tabs.lock().unwrap();
    let active = tabs.active_id();
    tabs.list()
        .iter()
        .map(|t| TabView {
            id: t.id,
            title: t.title.clone(),
            url: t.url.clone(),
            active: Some(t.id) == active,
            private: t.private,
            group: t.group.clone(),
            loading: t.loading,
            can_back: t.can_go_back(),
            can_forward: t.can_go_forward(),
        })
        .collect()
}

fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split(['/', '?', '#', ':'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// In-page interception bridge. System webviews do not expose subresource
/// request blocking, so this injects a script that rejects fetch/XHR/script/
/// image loads to known tracker domains. Navigation-level blocking is done in
/// Rust in `navigate`. See docs/LIMITATIONS.md.
fn init_script(blocker: &Blocker) -> String {
    let domains = serde_json::to_string(&blocker.blocked_domains()).unwrap_or_else(|_| "[]".into());
    format!(
        r#"
(function(){{
  var BLOCKED = new Set({domains});
  function blocked(u) {{
    try {{ var h = new URL(u, location.href).hostname.toLowerCase();
      for (var d of BLOCKED) if (h === d || h.endsWith('.' + d)) return true;
    }} catch (e) {{}}
    return false;
  }}
  function note(u) {{
    window.__vyroBlocked = (window.__vyroBlocked || 0) + 1;
    try {{ window.dispatchEvent(new CustomEvent('vyro-blocked', {{ detail: u }})); }} catch (e) {{}}
  }}
  var of = window.fetch;
  window.fetch = function(input) {{
    var u = typeof input === 'string' ? input : (input && input.url) || '';
    if (blocked(u)) {{ note(u); return Promise.reject(new Error('VYRO: blocked')); }}
    return of.apply(this, arguments);
  }};
  var oo = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function(m, u) {{
    if (blocked(String(u))) {{ note(String(u)); throw new Error('VYRO: blocked'); }}
    return oo.apply(this, arguments);
  }};
  ['src'].forEach(function(prop) {{
    [HTMLScriptElement, HTMLImageElement, HTMLIFrameElement].forEach(function(Ctor) {{
      var d = Object.getOwnPropertyDescriptor(Ctor.prototype, prop);
      if (!d) return;
      Object.defineProperty(Ctor.prototype, prop, {{
        get: function() {{ return d.get.call(this); }},
        set: function(v) {{ if (blocked(String(v))) {{ note(String(v)); return; }} d.set.call(this, v); }}
      }});
    }});
  }});
}})();
"#,
        domains = domains
    )
}

fn persist(state: &AppState) {
    let dir = state.data_dir.lock().unwrap();
    if fs::create_dir_all(&*dir).is_err() {
        return;
    }
    if let Ok(v) = serde_json::to_string_pretty(&*state.settings.lock().unwrap()) {
        let _ = fs::write(dir.join("settings.json"), v);
    }
    let b = state.bookmarks.lock().unwrap();
    let _ = fs::write(dir.join("bookmarks.json"), b.export_json());
    drop(b);
    let h = state.history.lock().unwrap();
    if let Ok(v) = serde_json::to_string_pretty(h.list()) {
        let _ = fs::write(dir.join("history.json"), v);
    }
}

fn load_initial(state: &AppState) {
    let dir = state.data_dir.lock().unwrap();
    if let Ok(s) = fs::read(dir.join("settings.json")) {
        if let Ok(v) = serde_json::from_slice::<Settings>(&s) {
            *state.settings.lock().unwrap() = v;
        }
    }
    if let Ok(s) = fs::read(dir.join("bookmarks.json")) {
        let mut b = state.bookmarks.lock().unwrap();
        let _ = b.import_json(&String::from_utf8_lossy(&s));
    }
}

fn content_size(window: &tauri::Window) -> (f64, f64) {
    let size = window.inner_size().unwrap_or(tauri::PhysicalSize::new(1400, 900));
    let scale = window.scale_factor().unwrap_or(1.0);
    (
        size.width as f64 / scale,
        size.height as f64 / scale - CHROME_H,
    )
}

// Real webview visibility — the old offscreen-bounds trick was unreliable and
// let page webviews paint over dashboard panels.
fn hide_webview(wv: &TabWebview) {
    let _ = wv.hide();
}

fn show_webview(wv: &TabWebview) {
    let _ = wv.show();
}

fn emit_state(app: &tauri::AppHandle, state: &AppState) {
    let blocker = state.blocker.lock().unwrap();
    let payload = json!({
        "tabs": tab_views(state),
        "blocker": {
            "enabled": blocker.is_enabled(),
            "filters": blocker.filter_count(),
            "stats": blocker.stats_snapshot(),
            "allowlist": blocker.allowlisted_sites()
        }
    });
    let _ = app.emit("vyro://state", payload);
}

// ---------------------------------------------------------------------------
// Tab & navigation commands
// ---------------------------------------------------------------------------

#[tauri::command]
async fn list_tabs(state: tauri::State<'_, AppState>) -> Result<Vec<TabView>, String> {
    Ok(tab_views(&state))
}

#[tauri::command]
async fn new_tab(
    url: Option<String>,
    private: Option<bool>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<TabView>, String> {
    let private = private.unwrap_or(false);
    let target = url.unwrap_or_default();
    let id = state.tabs.lock().unwrap().open(target.clone(), private);
    let script = init_script(&state.blocker.lock().unwrap());

    let window = app.get_window("main").ok_or("main window missing")?;
    let (w, h) = content_size(&window);
    let webview_url = if target.is_empty() {
        WebviewUrl::App("newtab.html".into())
    } else {
        WebviewUrl::External(
            url::Url::parse(&target)
                .map_err(|e| e.to_string())?,
        )
    };
    let wv = window
        .add_child(
            tauri::webview::WebviewBuilder::new(format!("tab-{}", id), webview_url)
                .initialization_script(&script)
                .auto_resize(),
            LogicalPosition::new(0.0, CHROME_H),
            LogicalSize::new(w, h),
        )
        .map_err(|e| e.to_string())?;

    for (other_id, other) in state.webviews.lock().unwrap().iter() {
        if *other_id != id {
            hide_webview(other);
        }
    }
    state.webviews.lock().unwrap().insert(id, wv);
    emit_state(&app, &state);
    Ok(tab_views(&state))
}

#[tauri::command]
async fn close_tab(id: u64, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<Vec<TabView>, String> {
    state.tabs.lock().unwrap().close(id);
    if let Some(wv) = state.webviews.lock().unwrap().remove(&id) {
        let _ = wv.close();
    }
    if let Some(active) = state.tabs.lock().unwrap().active_id() {
        if let Some(wv) = state.webviews.lock().unwrap().get(&active) {
            show_webview(wv);
        }
    }
    emit_state(&app, &state);
    Ok(tab_views(&state))
}

#[tauri::command]
async fn activate_tab(id: u64, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<Vec<TabView>, String> {
    state.tabs.lock().unwrap().set_active(id);
    for (tid, wv) in state.webviews.lock().unwrap().iter() {
        if *tid == id {
            show_webview(wv);
        } else {
            hide_webview(wv);
        }
    }
    emit_state(&app, &state);
    Ok(tab_views(&state))
}

#[tauri::command]
async fn navigate(
    id: u64,
    input: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Value, String> {
    let engine = state.settings.lock().unwrap().search_engine;
    let intent = resolve(&input, engine);
    let raw = match intent {
        vyro_core::engine::NavigationIntent::Url(u) => u,
        vyro_core::engine::NavigationIntent::Search(u) => u,
    };
    let url = state.privacy.lock().unwrap().apply_policies(&raw);

    let source = host_of(&url);
    let decision = state.blocker.lock().unwrap().check(
        &url,
        &source,
        ResourceType::Document,
        false,
    );
    if decision.blocked {
        emit_state(&app, &state);
        return Ok(json!({ "blocked": true, "url": url, "filter": decision.filter }));
    }

    state.tabs.lock().unwrap().navigate(id, &url);
    state.history.lock().unwrap().visit(&url, &url);
    if let Some(wv) = state.webviews.lock().unwrap().get(&id) {
        let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;
        wv.navigate(parsed).map_err(|e| e.to_string())?;
    }
    persist(&state);
    emit_state(&app, &state);
    Ok(json!({ "blocked": false, "url": url }))
}

#[tauri::command]
async fn go_back(id: u64, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<Value, String> {
    let url = state.tabs.lock().unwrap().back(id);
    if let Some(ref u) = url {
        if let Some(wv) = state.webviews.lock().unwrap().get(&id) {
            let parsed = url::Url::parse(u).map_err(|e| e.to_string())?;
            wv.navigate(parsed).map_err(|e| e.to_string())?;
        }
    }
    emit_state(&app, &state);
    Ok(json!({ "url": url }))
}

#[tauri::command]
async fn go_forward(id: u64, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<Value, String> {
    let url = state.tabs.lock().unwrap().forward(id);
    if let Some(ref u) = url {
        if let Some(wv) = state.webviews.lock().unwrap().get(&id) {
            let parsed = url::Url::parse(u).map_err(|e| e.to_string())?;
            wv.navigate(parsed).map_err(|e| e.to_string())?;
        }
    }
    emit_state(&app, &state);
    Ok(json!({ "url": url }))
}

#[tauri::command]
async fn reload(id: u64, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let url = state.tabs.lock().unwrap().get(id).map(|t| t.url.clone());
    if let Some(u) = url.filter(|u| !u.is_empty()) {
        if let Some(wv) = state.webviews.lock().unwrap().get(&id) {
            wv.navigate(url::Url::parse(&u).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        }
    }
    emit_state(&app, &state);
    Ok(())
}

/// Dashboards live in the chrome webview, so page webviews are hidden while a
/// panel is open (otherwise they paint over the panel and swallow its clicks).
#[tauri::command]
async fn set_content_visible(visible: bool, _app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let active = state.tabs.lock().unwrap().active_id();
    for (id, wv) in state.webviews.lock().unwrap().iter() {
        if visible && Some(*id) == active {
            show_webview(wv);
        } else {
            hide_webview(wv);
        }
    }
    Ok(())
}

#[tauri::command]
async fn set_zoom(id: u64, factor: f64, state: tauri::State<'_, AppState>) -> Result<(), String> {
    if let Some(wv) = state.webviews.lock().unwrap().get(&id) {
        let _ = wv.set_zoom(factor);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Bookmarks / history / downloads
// ---------------------------------------------------------------------------

#[tauri::command]
async fn bookmarks_list(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    let b = state.bookmarks.lock().unwrap();
    Ok(json!({ "items": b.list(), "folders": b.folders() }))
}

#[tauri::command]
async fn bookmarks_add(title: String, url: String, folder: String, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<Value, String> {
    let id = state.bookmarks.lock().unwrap().add(title, url, folder);
    persist(&state);
    emit_state(&app, &state);
    Ok(json!({ "id": id }))
}

#[tauri::command]
async fn bookmarks_remove(id: u64, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.bookmarks.lock().unwrap().remove(id);
    persist(&state);
    emit_state(&app, &state);
    Ok(())
}

#[tauri::command]
async fn bookmarks_import(html: String, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<usize, String> {
    let n = state.bookmarks.lock().unwrap().import_netscape_html(&html);
    persist(&state);
    emit_state(&app, &state);
    Ok(n)
}

#[tauri::command]
async fn bookmarks_export(state: tauri::State<'_, AppState>) -> Result<String, String> {
    Ok(state.bookmarks.lock().unwrap().export_json())
}

#[tauri::command]
async fn history_list(query: Option<String>, _app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<Value, String> {
    let h = state.history.lock().unwrap();
    Ok(json!(h.search(query.as_deref().unwrap_or(""), 500)))
}

#[tauri::command]
async fn history_clear(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.history.lock().unwrap().clear();
    persist(&state);
    emit_state(&app, &state);
    Ok(())
}

#[tauri::command]
async fn downloads_list(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    Ok(json!(state.downloads.lock().unwrap().list()))
}

// ---------------------------------------------------------------------------
// Blocker / privacy
// ---------------------------------------------------------------------------

#[tauri::command]
async fn blocker_stats(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    let b = state.blocker.lock().unwrap();
    Ok(json!({
        "enabled": b.is_enabled(),
        "filters": b.filter_count(),
        "stats": b.stats_snapshot(),
        "allowlist": b.allowlisted_sites()
    }))
}

#[tauri::command]
async fn blocker_toggle(enabled: bool, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.blocker.lock().unwrap().set_enabled(enabled);
    emit_state(&app, &state);
    Ok(())
}

#[tauri::command]
async fn blocker_allow_site(site: String, allowed: bool, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut b = state.blocker.lock().unwrap();
    if allowed { b.allow_site(&site) } else { b.unallow_site(&site) }
    emit_state(&app, &state);
    Ok(())
}

#[tauri::command]
async fn blocker_add_filters(text: String, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<usize, String> {
    let n = state.blocker.lock().unwrap().add_filters(&text, "user");
    emit_state(&app, &state);
    Ok(n)
}

#[tauri::command]
async fn privacy_state(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    let p = state.privacy.lock().unwrap();
    Ok(json!({
        "config": p.config,
        "permissions": p.permissions.all()
    }))
}

#[tauri::command]
async fn privacy_set_permission(origin: String, permission: String, decision: String, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let perm = match permission.as_str() {
        "camera" => Permission::Camera,
        "microphone" => Permission::Microphone,
        "location" => Permission::Location,
        "notifications" => Permission::Notifications,
        _ => return Err("unknown permission".into()),
    };
    let dec = match decision.as_str() {
        "allow" => Decision::Allow,
        "block" => Decision::Block,
        _ => Decision::Ask,
    };
    state.privacy.lock().unwrap().permissions.set(&origin, perm, dec);
    emit_state(&app, &state);
    Ok(())
}

// ---------------------------------------------------------------------------
// VPN / extensions / settings (honest integration layers)
// ---------------------------------------------------------------------------

#[tauri::command]
async fn vpn_state(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    let v = state.vpn.lock().unwrap();
    Ok(json!({
        "status": v.status,
        "countries": v.countries(),
        "providers": v.providers,
        "diagnostics": v.diagnostics(),
        "kill_switch": v.kill_switch,
        "dns_leak_protection": v.dns_leak_protection,
        "auto_reconnect": v.auto_reconnect
    }))
}

#[tauri::command]
async fn vpn_add_provider(json_text: String, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let provider: VpnProviderConfig = serde_json::from_str(&json_text).map_err(|e| e.to_string())?;
    state.vpn.lock().unwrap().add_provider(provider);
    emit_state(&app, &state);
    Ok(())
}

#[tauri::command]
async fn vpn_connect(provider: String, server: String, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.vpn.lock().unwrap().connect(&provider, &server).map_err(|e| e.to_string())?;
    emit_state(&app, &state);
    Ok(())
}

#[tauri::command]
async fn vpn_disconnect(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.vpn.lock().unwrap().disconnect().map_err(|e| e.to_string())?;
    emit_state(&app, &state);
    Ok(())
}

#[tauri::command]
async fn extensions_list(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    Ok(json!({
        "items": state.extensions.lock().unwrap().list(),
        "support": ExtensionManager::support_report()
    }))
}

#[tauri::command]
async fn extensions_install(manifest: String, path: String, id: String, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.extensions.lock().unwrap().install_from_manifest(&manifest, &path, &id).map_err(|e| e.to_string())?;
    emit_state(&app, &state);
    Ok(())
}

#[tauri::command]
async fn extensions_toggle(id: String, enabled: bool, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.extensions.lock().unwrap().set_enabled(&id, enabled).map_err(|e| e.to_string())?;
    emit_state(&app, &state);
    Ok(())
}

#[tauri::command]
async fn extensions_remove(id: String, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.extensions.lock().unwrap().remove(&id).map_err(|e| e.to_string())?;
    emit_state(&app, &state);
    Ok(())
}

#[tauri::command]
async fn extension_support() -> Result<SupportReport, String> {
    Ok(ExtensionManager::support_report())
}

#[tauri::command]
async fn settings_get(state: tauri::State<'_, AppState>) -> Result<Settings, String> {
    Ok(state.settings.lock().unwrap().clone())
}

/// Merge a partial settings object into the current settings. Toggles use this
/// so a missing field can never make a click silently fail.
#[tauri::command]
async fn settings_patch(value: Value, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<Settings, String> {
    let current = state.settings.lock().unwrap().clone();
    let mut merged = serde_json::to_value(&current).map_err(|e| e.to_string())?;
    if let (Some(obj), Some(patch)) = (merged.as_object_mut(), value.as_object()) {
        for (key, val) in patch {
            obj.insert(key.clone(), val.clone());
        }
    }
    let updated: Settings = serde_json::from_value(merged).map_err(|e| e.to_string())?;
    *state.settings.lock().unwrap() = updated.clone();
    persist(&state);
    emit_state(&app, &state);
    Ok(updated)
}

#[tauri::command]
async fn settings_update(value: Value, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let updated: Settings = serde_json::from_value(value).map_err(|e| e.to_string())?;
    *state.settings.lock().unwrap() = updated;
    persist(&state);
    emit_state(&app, &state);
    Ok(())
}

#[tauri::command]
async fn engine_engines() -> Result<Value, String> {
    Ok(json!([
        { "id": "duckduckgo", "label": "DuckDuckGo" },
        { "id": "brave", "label": "Brave" },
        { "id": "google", "label": "Google" },
        { "id": "bing", "label": "Bing" },
        { "id": "startpage", "label": "Startpage" }
    ]))
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            data_dir: Mutex::new(data_dir(APP_NAME)),
            ..Default::default()
        })
        .setup(|app| {
            load_initial(&app.state::<AppState>());
            let window = tauri::window::WindowBuilder::new(app, "main")
                .title("VYRO Browser — Browse Beyond Limits.")
                .inner_size(1440.0, 900.0)
                .min_inner_size(820.0, 600.0)
                .decorations(false)
                .build()?;
            let (w, h) = content_size(&window);
            window.add_child(
                tauri::webview::WebviewBuilder::new("ui", WebviewUrl::App("index.html".into()))
                    .auto_resize(),
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(w, h + CHROME_H),
            )?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let _ = handle; // tabs are created by the UI via `new_tab`
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_tabs,
            new_tab,
            close_tab,
            activate_tab,
            navigate,
            go_back,
            go_forward,
            reload,
            set_content_visible,
            set_zoom,
            bookmarks_list,
            bookmarks_add,
            bookmarks_remove,
            bookmarks_import,
            bookmarks_export,
            history_list,
            history_clear,
            downloads_list,
            blocker_stats,
            blocker_toggle,
            blocker_allow_site,
            blocker_add_filters,
            privacy_state,
            privacy_set_permission,
            vpn_state,
            vpn_add_provider,
            vpn_connect,
            vpn_disconnect,
            extensions_list,
            extensions_install,
            extensions_toggle,
            extensions_remove,
            extension_support,
            settings_get,
            settings_patch,
            settings_update,
            engine_engines
        ])
        .run(tauri::generate_context!())
        .expect("VYRO Browser failed to start");
}

// Keep the compiler from dropping the unused enum variant import in builds
// where SearchEngine is only referenced through Settings.
#[allow(dead_code)]
fn _engine_marker(_: SearchEngine) {}
