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
use vyro_core::companions::CompanionManager;
use vyro_core::safe_browsing::{SafeBrowsing, Verdict};
use vyro_core::downloads::DownloadManager;
use vyro_core::engine::{resolve, SearchEngine};
use vyro_core::extensions::{ExtensionManager, SupportReport};
use vyro_core::history::History;
use vyro_core::platform::data_dir;
use vyro_core::privacy::{Decision, Permission, PrivacyController};
use vyro_core::settings::Settings;
use vyro_core::tabs::TabManager;
use vyro_core::vault::Vault;
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
    safe_browsing: Mutex<SafeBrowsing>,
    vault: Mutex<Vault>,
    companions: Mutex<CompanionManager>,
    /// App-level kill switch: blocks navigation when the VPN drops.
    network_gate: Mutex<bool>,
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

/// In-page bridge: ad/tracker interception, fingerprinting mitigations,
/// find-on-page, reader mode, and first-party companion code. System webviews
/// expose no general network interception, so this is a genuine but bounded
/// layer — see docs/LIMITATIONS.md.
fn init_script(blocker: &Blocker, fp: bool, companions: &str) -> String {
    let domains = serde_json::to_string(&blocker.blocked_domains()).unwrap_or_else(|_| "[]".into());
    let mut script = r#"
(function(){
  var BLOCKED = new Set(__VYRO_DOMAINS__);
  function blocked(u) {
    try { var h = new URL(u, location.href).hostname.toLowerCase();
      for (var d of BLOCKED) if (h === d || h.endsWith('.' + d)) return true;
    } catch (e) {}
    return false;
  }
  function note(u) {
    window.__vyroBlocked = (window.__vyroBlocked || 0) + 1;
    try { window.dispatchEvent(new CustomEvent('vyro-blocked', { detail: u })); } catch (e) {}
  }
  var of = window.fetch;
  window.fetch = function(input) {
    var u = typeof input === 'string' ? input : (input && input.url) || '';
    if (blocked(u)) { note(u); return Promise.reject(new Error('VYRO: blocked')); }
    return of.apply(this, arguments);
  };
  var oo = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function(m, u) {
    if (blocked(String(u))) { note(String(u)); throw new Error('VYRO: blocked'); }
    return oo.apply(this, arguments);
  };
  [HTMLScriptElement, HTMLImageElement, HTMLIFrameElement].forEach(function(Ctor) {
    var d = Object.getOwnPropertyDescriptor(Ctor.prototype, 'src');
    if (!d) return;
    Object.defineProperty(Ctor.prototype, 'src', {
      get: function() { return d.get.call(this); },
      set: function(v) { if (blocked(String(v))) { note(String(v)); return; } d.set.call(this, v); }
    });
  });

  /* ---- fingerprinting mitigation (opt-in) ---- */
  if (__VYRO_FP__ === '1') {
    try { Object.defineProperty(navigator, 'webdriver', { get: function(){ return undefined; } }); } catch(e){}
    try { Object.defineProperty(navigator, 'plugins', { get: function(){ return []; } }); } catch(e){}
    try { Object.defineProperty(navigator, 'mimeTypes', { get: function(){ return []; } }); } catch(e){}
    try { Object.defineProperty(navigator, 'hardwareConcurrency', { get: function(){ return 4; } }); } catch(e){}
    try { Object.defineProperty(navigator, 'deviceMemory', { get: function(){ return 4; } }); } catch(e){}
    try { Object.defineProperty(navigator, 'languages', { get: function(){ return ['en-US','en']; } }); } catch(e){}
    var screenVals = { width:1920, height:1080, availWidth:1920, availHeight:1040, colorDepth:24, pixelDepth:24 };
    Object.keys(screenVals).forEach(function(k){ try { Object.defineProperty(screen, k, { get: (function(v){ return function(){ return v; }; })(screenVals[k]) }); } catch(e){} });
    try { Object.defineProperty(window, 'devicePixelRatio', { get: function(){ return 1; } }); } catch(e){}
    try {
      var oldTz = Intl.DateTimeFormat.prototype.resolvedOptions;
      Intl.DateTimeFormat.prototype.resolvedOptions = function(){ var o = oldTz.call(this); o.timeZone = 'UTC'; return o; };
    } catch(e){}
    try {
      var oldGet = CanvasRenderingContext2D.prototype.getImageData;
      CanvasRenderingContext2D.prototype.getImageData = function(){
        var d = oldGet.apply(this, arguments);
        if (d.data.length) { d.data[0] = (d.data[0] + 1) % 256; }
        return d;
      };
      var oldData = HTMLCanvasElement.prototype.toDataURL;
      HTMLCanvasElement.prototype.toDataURL = function(){
        try {
          var tmp = document.createElement('canvas'); tmp.width = this.width; tmp.height = this.height;
          var c = tmp.getContext('2d'); c.drawImage(this, 0, 0);
          var d = c.getImageData(0, 0, 1, 1); d.data[0] = (d.data[0] + 1) % 256; c.putImageData(d, 0, 0);
          return oldData.apply(tmp, arguments);
        } catch(e) { return oldData.apply(this, arguments); }
      };
    } catch(e){}
    try {
      var oldFreq = AnalyserNode.prototype.getFloatFrequencyData;
      AnalyserNode.prototype.getFloatFrequencyData = function(a){
        oldFreq.call(this, a);
        for (var i = 0; i < a.length; i++) a[i] += (i % 2 ? 0.00001 : -0.00001);
      };
      var oldParam = WebGLRenderingContext.prototype.getParameter;
      WebGLRenderingContext.prototype.getParameter = function(p){
        if (p === 37445) return 'Google Inc. (VYRO)';
        if (p === 37446) return 'ANGLE (VYRO, Software)';
        return oldParam.call(this, p);
      };
    } catch(e){}
  }

  /* ---- find on page ---- */
  var findStyle = document.createElement('style');
  findStyle.textContent = '#vyro-findbar{position:fixed;top:12px;right:16px;z-index:2147483647;display:flex;' +
    'align-items:center;gap:6px;padding:8px 10px;border-radius:10px;background:#151524;color:#f2f3f9;' +
    'border:1px solid rgba(255,255,255,.18);box-shadow:0 12px 30px rgba(0,0,0,.45);font:13px system-ui}' +
    '#vyro-findbar input{width:220px;padding:7px 10px;border-radius:7px;border:1px solid #3a3a55;' +
    'background:#0d0d17;color:#fff;outline:none}#vyro-findbar input:focus{border-color:#7c5cff}' +
    '#vyro-findbar span{min-width:56px;text-align:center;color:#9aa0b5;font-size:12px}' +
    '#vyro-findbar button{width:26px;height:26px;border:0;border-radius:6px;background:transparent;color:#c9ccdb;cursor:pointer}' +
    '#vyro-findbar button:hover{background:rgba(255,255,255,.12)}' +
    'mark.vyro-find{background:rgba(124,92,255,.35);color:inherit;border-radius:2px}' +
    'mark.vyro-find.active{background:#7c5cff;color:#fff}';
  document.head.appendChild(findStyle);
  window.__vyroFindState = { marks: [], index: 0, bar: null };
  window.__vyroFindClose = function(){
    var st = window.__vyroFindState;
    st.marks.forEach(function(m){ try { m.replaceWith(document.createTextNode(m.textContent)); } catch(e){} });
    st.marks = []; st.index = 0;
    if (st.bar) { st.bar.remove(); st.bar = null; }
  };
  window.__vyroFindHighlight = function(){
    var st = window.__vyroFindState;
    st.marks.forEach(function(m, i){ m.classList.toggle('active', i === st.index); });
    if (st.marks[st.index]) st.marks[st.index].scrollIntoView({ block: 'center' });
  };
  window.__vyroFindUpdate = function(){
    var st = window.__vyroFindState, el = document.getElementById('vyro-findcount');
    if (el) el.textContent = st.marks.length ? (st.index + 1) + ' / ' + st.marks.length : 'No results';
  };
  window.__vyroFind = function(q){
    window.__vyroFindClose();
    var st = window.__vyroFindState;
    if (!st.bar) {
      var bar = document.createElement('div'); bar.id = 'vyro-findbar';
      bar.innerHTML = '<input id="vyro-findinput" placeholder="Find on page" spellcheck="false">' +
        '<span id="vyro-findcount"></span>' +
        '<button id="vyro-findprev" title="Previous">‹</button>' +
        '<button id="vyro-findnext" title="Next">›</button>' +
        '<button id="vyro-findclose" title="Close">×</button>';
      document.documentElement.appendChild(bar);
      document.getElementById('vyro-findinput').addEventListener('input', function(){ window.__vyroFind(this.value); });
      document.getElementById('vyro-findprev').onclick = function(){ window.__vyroFindStep(-1); };
      document.getElementById('vyro-findnext').onclick = function(){ window.__vyroFindStep(1); };
      document.getElementById('vyro-findclose').onclick = window.__vyroFindClose;
      st.bar = bar;
    }
    var input = document.getElementById('vyro-findinput');
    if (input && q !== undefined && input.value !== q) input.value = q;
    if (!q) { window.__vyroFindUpdate(); return 0; }
    var needle = String(q).toLowerCase();
    var walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, {
      acceptNode: function(n){
        if (!n.nodeValue || !n.nodeValue.trim()) return NodeFilter.FILTER_REJECT;
        var p = n.parentElement;
        if (!p || /SCRIPT|STYLE|NOSCRIPT|MARK|INPUT|TEXTAREA/.test(p.tagName)) return NodeFilter.FILTER_REJECT;
        return NodeFilter.FILTER_ACCEPT;
      }
    });
    var nodes = [], n;
    while ((n = walker.nextNode())) nodes.push(n);
    nodes.forEach(function(node){
      var text = node.nodeValue, lower = text.toLowerCase(), ranges = [], from = 0, i;
      while ((i = lower.indexOf(needle, from)) !== -1) { ranges.push([i, i + needle.length]); from = i + needle.length; }
      if (!ranges.length) return;
      var frag = document.createDocumentFragment(), pos = 0;
      ranges.forEach(function(r){
        if (r[0] > pos) frag.appendChild(document.createTextNode(text.slice(pos, r[0])));
        var m = document.createElement('mark'); m.className = 'vyro-find'; m.textContent = text.slice(r[0], r[1]);
        frag.appendChild(m); st.marks.push(m); pos = r[1];
      });
      if (pos < text.length) frag.appendChild(document.createTextNode(text.slice(pos)));
      node.parentNode.replaceChild(frag, node);
    });
    st.index = 0; window.__vyroFindHighlight(); window.__vyroFindUpdate();
    return st.marks.length;
  };
  window.__vyroFindStep = function(dir){
    var st = window.__vyroFindState;
    if (!st.marks.length) return;
    st.index = (st.index + dir + st.marks.length) % st.marks.length;
    window.__vyroFindHighlight(); window.__vyroFindUpdate();
  };
  document.addEventListener('keydown', function(e){
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
      e.preventDefault(); window.__vyroFind('');
      var inp = document.getElementById('vyro-findinput'); if (inp) inp.focus();
    } else if (e.key === 'Enter' && document.activeElement && document.activeElement.id === 'vyro-findinput') {
      e.preventDefault(); window.__vyroFindStep(e.shiftKey ? -1 : 1);
    } else if (e.key === 'Escape') {
      window.__vyroFindClose();
    }
  });

  /* ---- reader mode ---- */
  window.__vyroReader = function(){
    var old = document.getElementById('vyro-reader-style');
    if (old) { old.remove(); return false; }
    var s = document.createElement('style'); s.id = 'vyro-reader-style';
    s.textContent = 'body{background:#faf8f2!important;color:#202124!important;max-width:760px!important;' +
      'margin:40px auto!important;padding:0 24px!important;float:none!important;width:auto!important;' +
      'font:19px/1.75 Georgia,serif!important}header,nav,footer,aside,form,iframe,.ad,#sidebar{display:none!important}' +
      'img{max-width:100%!important;height:auto!important}a{color:#5b3fd6!important}';
    document.head.appendChild(s);
    return true;
  };
})();
__VYRO_COMPANIONS__
"#.to_string();
    script = script.replace("__VYRO_DOMAINS__", &domains);
    script = script.replace("__VYRO_FP__", if fp { "1" } else { "0" });
    script = script.replace("__VYRO_COMPANIONS__", companions);
    script
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
    drop(h);
    if let Ok(v) = state.vault.lock().unwrap().to_json() {
        let _ = fs::write(dir.join("vault.json"), v);
    }
    if let Ok(v) = serde_json::to_string_pretty(state.companions.lock().unwrap().list()) {
        let _ = fs::write(dir.join("companions.json"), v);
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
    if let Ok(s) = fs::read(dir.join("history.json")) {
        if let Ok(entries) = serde_json::from_slice::<Vec<vyro_core::history::HistoryEntry>>(&s) {
            state.history.lock().unwrap().import(entries);
        }
    }
    if let Ok(s) = fs::read(dir.join("vault.json")) {
        if let Ok(v) = Vault::from_json(&String::from_utf8_lossy(&s)) {
            *state.vault.lock().unwrap() = v;
        }
    }
    if let Ok(s) = fs::read(dir.join("companions.json")) {
        if let Ok(list) = serde_json::from_slice::<Vec<vyro_core::companions::Companion>>(&s) {
            let mut m = state.companions.lock().unwrap();
            for c in list {
                let _ = m.install(c.id, c.name, c.matches, c.code);
            }
        }
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
        },
        "network_gated": *state.network_gate.lock().unwrap()
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
    let fp = state.settings.lock().unwrap().fingerprinting_protection;
    let companion_code = state.companions.lock().unwrap().script_for("*");
    let script = init_script(&state.blocker.lock().unwrap(), fp, &companion_code);

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

    // App-level kill switch: engages when the VPN drops and stays on until the
    // user explicitly restores connectivity. It is NOT an OS firewall rule.
    if *state.network_gate.lock().unwrap() {
        return Ok(json!({ "blocked": true, "url": url, "reason": "Network gate active — VPN kill switch" }));
    }

    // Safe Browsing verdict: local denylist + optional real API lookup.
    let verdict = state.safe_browsing.lock().unwrap().check(&url);
    if let Verdict::Malicious(reason) = verdict {
        emit_state(&app, &state);
        return Ok(json!({ "blocked": true, "url": url, "reason": format!("Safe Browsing: {}", reason) }));
    }

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
    *state.network_gate.lock().unwrap() = false;
    emit_state(&app, &state);
    Ok(())
}

#[tauri::command]
async fn vpn_disconnect(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let kill_switch = state.vpn.lock().unwrap().kill_switch;
    state.vpn.lock().unwrap().disconnect().map_err(|e| e.to_string())?;
    if kill_switch {
        *state.network_gate.lock().unwrap() = true;
    }
    emit_state(&app, &state);
    Ok(())
}

#[tauri::command]
async fn set_network_gate(enabled: bool, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    *state.network_gate.lock().unwrap() = enabled;
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

// ---------------------------------------------------------------------------
// Protection, vault, companions, reader/find, groups
// ---------------------------------------------------------------------------

#[tauri::command]
async fn safe_browsing_state(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    let sb = state.safe_browsing.lock().unwrap();
    Ok(json!({
        "enabled": sb.config.enabled,
        "remote_available": sb.remote_available(),
        "api_key_env": sb.config.api_key_env,
        "domains": sb.local_domains()
    }))
}

#[tauri::command]
async fn safe_browsing_toggle(enabled: bool, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.safe_browsing.lock().unwrap().set_enabled(enabled);
    Ok(())
}

#[tauri::command]
async fn safe_browsing_add_domain(domain: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.safe_browsing.lock().unwrap().add_local_domain(&domain);
    persist(&state);
    Ok(())
}

#[tauri::command]
async fn find_in_page(id: u64, query: String, step: Option<i32>, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let js = if let Some(direction) = step {
        format!("window.__vyroFindStep({});", direction)
    } else {
        format!("window.__vyroFind({});", serde_json::to_string(&query).unwrap_or_else(|_| "\"\"".into()))
    };
    if let Some(wv) = state.webviews.lock().unwrap().get(&id) {
        wv.eval(&js).map_err(|e| e.to_string())?;
    }
    emit_state(&app, &state);
    Ok(())
}

#[tauri::command]
async fn reader_mode(id: u64, state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let code = r#"(function(){ return !!window.__vyroReader && window.__vyroReader(); })()"#;
    if let Some(wv) = state.webviews.lock().unwrap().get(&id) {
        // eval cannot return a value; the toggle also works by calling it again.
        wv.eval(code).map_err(|e| e.to_string())?;
    }
    Ok(true)
}

#[tauri::command]
async fn tab_set_group(id: u64, group: Option<String>, app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<Vec<TabView>, String> {
    state.tabs.lock().unwrap().set_group(id, group);
    emit_state(&app, &state);
    Ok(tab_views(&state))
}

#[tauri::command]
async fn vault_state(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    let v = state.vault.lock().unwrap();
    Ok(json!({
        "initialized": v.is_initialized(),
        "unlocked": v.is_unlocked(),
        "count": v.count()
    }))
}

#[tauri::command]
async fn vault_init(master: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.vault.lock().unwrap().initialize(&master).map_err(|e| e.to_string())?;
    persist(&state);
    Ok(())
}

#[tauri::command]
async fn vault_unlock(master: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.vault.lock().unwrap().unlock(&master).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn vault_lock(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.vault.lock().unwrap().lock();
    Ok(())
}

#[tauri::command]
async fn vault_add(service: String, username: String, password: String, state: tauri::State<'_, AppState>) -> Result<u64, String> {
    let id = state.vault.lock().unwrap().add(&service, &username, &password).map_err(|e| e.to_string())?;
    persist(&state);
    Ok(id)
}

#[tauri::command]
async fn vault_list(state: tauri::State<'_, AppState>) -> Result<Vec<(u64, String, String)>, String> {
    Ok(state.vault.lock().unwrap().list())
}

#[tauri::command]
async fn vault_reveal(id: u64, state: tauri::State<'_, AppState>) -> Result<String, String> {
    state.vault.lock().unwrap().reveal(id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn vault_remove(id: u64, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.vault.lock().unwrap().remove(id).map_err(|e| e.to_string())?;
    persist(&state);
    Ok(())
}

#[tauri::command]
async fn companion_list(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    Ok(json!({
        "items": state.companions.lock().unwrap().list(),
        "note": "VYRO companion format — first-party JS injected into matching pages. Not Chrome Web Store extensions."
    }))
}

#[tauri::command]
async fn companion_install(id: String, name: String, matches: Vec<String>, code: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.companions.lock().unwrap().install(id, name, matches, code)?;
    persist(&state);
    Ok(())
}

#[tauri::command]
async fn companion_remove(id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    if !state.companions.lock().unwrap().remove(&id) {
        return Err(format!("Companion '{}' not found", id));
    }
    persist(&state);
    Ok(())
}

#[tauri::command]
async fn companion_toggle(id: String, enabled: bool, state: tauri::State<'_, AppState>) -> Result<(), String> {
    if !state.companions.lock().unwrap().toggle(&id, enabled) {
        return Err(format!("Companion '{}' not found", id));
    }
    persist(&state);
    Ok(())
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
            engine_engines,
            set_network_gate,
            safe_browsing_state,
            safe_browsing_toggle,
            safe_browsing_add_domain,
            find_in_page,
            reader_mode,
            tab_set_group,
            vault_state,
            vault_init,
            vault_unlock,
            vault_lock,
            vault_add,
            vault_list,
            vault_reveal,
            vault_remove,
            companion_list,
            companion_install,
            companion_remove,
            companion_toggle
        ])
        .run(tauri::generate_context!())
        .expect("VYRO Browser failed to start");
}

// Keep the compiler from dropping the unused enum variant import in builds
// where SearchEngine is only referenced through Settings.
#[allow(dead_code)]
fn _engine_marker(_: SearchEngine) {}
