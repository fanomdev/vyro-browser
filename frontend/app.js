/* VYRO Browser — chrome UI controller. Talks to the Rust core over Tauri IPC. */
const T = window.__TAURI_INTERNALS__ || {};
const EI = window.__TAURI_EVENT_PLUGIN_INTERNALS__ || {};
const invoke = (cmd, args = {}) => T.invoke(cmd, args);

const $ = (s) => document.querySelector(s);
const $$ = (s) => Array.from(document.querySelectorAll(s));

const S = {
  tabs: [], activeId: null, settings: null,
  blocker: null, vpn: null, extensions: null, vpnGate: [],
};

/* --------------------------- boot --------------------------- */
async function boot() {
  bindUI();
  bindTabStripScrolling();
  await loadAll();
  if (!S.tabs.length) await newTab();
  listenForState();
  const addr = $('#address');
  addr.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') { e.preventDefault(); navigate(addr.value); }
    if (e.key === 'Escape') { addr.value = activeTab()?.url || ''; addr.blur(); }
  });
  applyTheme(S.settings);
}

async function loadAll() {
  try {
    S.settings = await invoke('settings_get');
    S.tabs = await invoke('list_tabs');
    S.activeId = S.tabs.find(t => t.active)?.id ?? S.tabs[0]?.id ?? null;
  } catch (e) { console.warn('VYRO state load failed', e); }
  renderTabs();
  renderAddress();
  const chip = { DuckDuckGo: 'DDG', Brave: 'BRV', Google: 'GGL', Bing: 'BNG', Startpage: 'SP' };
  $('#engine-chip').textContent = chip[S.settings?.search_engine] || 'DDG';
}

function listenForState() {
  const onState = (payload) => {
    const data = payload?.payload ?? payload;
    if (data?.tabs) { S.tabs = data.tabs; renderTabs(); renderAddress(); }
    if (data?.blocker) { S.blocker = data.blocker; renderShield(); }
    if (typeof data?.network_gated === 'boolean') {
      $('#gate-banner').classList.toggle('hidden', !data.network_gated);
    }
  };
  const listen = async () => {
    if (!T.transformCallback) throw new Error('no transformCallback');
    const handler = T.transformCallback(onState);
    await invoke('plugin:event|listen', { event: 'vyro://state', target: { kind: 'Any' }, handler });
    const settingsHandler = T.transformCallback(() => openPanel('settings'));
    await invoke('plugin:event|listen', { event: 'vyro://open-download-settings', target: { kind: 'Any' }, handler: settingsHandler }).catch(() => {});
    const dlHandler = T.transformCallback(async (payload) => {
      const data = payload?.payload ?? payload;
      if (!data?.started) return;
      toast('Downloading ' + (data.filename || 'file'));
      const badge = $('#download-badge');
      try {
        const list = await invoke('downloads_list');
        const active = (list.items || []).filter(d => d.state === 'InProgress' || d.state === 'Pending').length;
        badge.textContent = active || '•';
      } catch (_) { badge.textContent = '•'; }
      badge.classList.remove('hidden', 'pulse');
      void badge.offsetWidth;
      badge.classList.add('pulse');
    });
    await invoke('plugin:event|listen', { event: 'vyro://downloads', target: { kind: 'Any' }, handler: dlHandler }).catch(() => {});
  };
  listen().catch(() => setInterval(refreshLight, 1500));
}
async function refreshLight() {
  try {
    S.tabs = await invoke('list_tabs');
    S.blocker = await invoke('blocker_stats');
    renderTabs(); renderShield();
  } catch (_) {}
}

/* --------------------------- tabs & nav --------------------------- */
const activeTab = () => S.tabs.find(t => t.id === S.activeId);

let renderedTabIds = new Set();
function renderTabs() {
  const strip = $('#tabstrip');
  const savedScroll = strip.scrollLeft;
  const incoming = new Set(S.tabs.map(t => t.id));
  const ghosts = [...renderedTabIds].filter(id => !incoming.has(id)).map(id => {
    const el = strip.querySelector(`[data-tab-id="${id}"]`);
    return el ? el.cloneNode(true) : null;
  }).filter(Boolean);

  strip.innerHTML = '';
  for (const t of S.tabs) {
    const el = document.createElement('div');
    el.dataset.tabId = t.id;
    el.className = 'tab' + (t.active ? ' active' : '') + (t.private ? ' private' : '');
    if (!renderedTabIds.has(t.id)) el.classList.add('tab-enter');
    el.innerHTML = `<span class="fav"></span><span class="label"></span><span class="tab-close" title="Close tab"><svg viewBox="0 0 24 24"><path d="M6 6l12 12M18 6L6 18"/></svg></span>`;
    el.querySelector('.label').textContent = (t.group ? t.group + ' · ' : '') + (t.title || t.url || 'New Tab');
    el.onclick = (e) => { if (e.target.closest('.tab-close')) return; activateTab(t.id); };
    el.querySelector('.tab-close').onclick = (e) => { e.stopPropagation(); closeTab(t.id); };
    el.oncontextmenu = (e) => { e.preventDefault(); showTabMenu(e.clientX, e.clientY, t.id, t.group); };
    strip.appendChild(el);
  }
  ghosts.forEach(g => {
    g.classList.add('tab-exit'); g.removeAttribute('data-tab-id');
    strip.appendChild(g);
    setTimeout(() => g.remove(), 190);
  });
  renderedTabIds = incoming;
  strip.scrollLeft = savedScroll;
  $('#tab-count').textContent = S.tabs.length;
  const updateArrows = () => {
    $('#tabs-left').disabled = strip.scrollLeft <= 0;
    $('#tabs-right').disabled = strip.scrollLeft + strip.clientWidth >= strip.scrollWidth - 1;
  };
  strip.onscroll = updateArrows;
  updateArrows();
  const active = strip.querySelector('.tab.active');
  if (active) {
    const sr = strip.getBoundingClientRect(), ar = active.getBoundingClientRect();
    if (ar.right > sr.right) strip.scrollLeft += ar.right - sr.right;
    else if (ar.left < sr.left) strip.scrollLeft -= sr.left - ar.left;
  }
}

function bindTabStripScrolling() {
  const strip = $('#tabstrip');
  // Wheel only — pointer drags belong to Tauri's window drag region.
  strip.addEventListener('wheel', (e) => {
    if (strip.scrollWidth <= strip.clientWidth) return;
    e.preventDefault();
    const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? strip.clientWidth : 1;
    const delta = Math.abs(e.deltaY) > Math.abs(e.deltaX) ? e.deltaY : e.deltaX;
    strip.scrollLeft += delta * unit;
  }, { passive: false });
}
function renderAddress() {
  const t = activeTab();
  $('#address').value = t?.url || '';
  const back = $('#nav-back'), fwd = $('#nav-forward');
  back.style.opacity = t?.can_back ? '1' : '.4';
  fwd.style.opacity = t?.can_forward ? '1' : '.4';
}

async function newTab(url) {
  try { S.tabs = await invoke('new_tab', { url: url || null, private: false }); }
  catch (e) { return toast('Could not open tab: ' + e); }
  S.activeId = S.tabs.find(t => t.active)?.id ?? null;
  renderTabs(); renderAddress();
  if (openPanelName) { try { await invoke('set_content_visible', { visible: false }); } catch (_) {} }
}
async function activateTab(id) {
  S.activeId = id;
  try { S.tabs = await invoke('activate_tab', { id }); } catch (_) {}
  renderTabs(); renderAddress();
}
async function closeTab(id) {
  try { S.tabs = await invoke('close_tab', { id }); } catch (e) { return toast(e); }
  if (S.activeId === id) S.activeId = S.tabs.find(t => t.active)?.id ?? null;
  if (!S.tabs.length) {
    // Closing the last tab closes the window, like a real browser.
    await invoke('plugin:window|close', { label: 'main' }).catch(() => {});
    return;
  }
  renderTabs(); renderAddress();
}
async function navigate(input) {
  if (S.activeId == null) return newTab(input);
  try {
    const r = await invoke('navigate', { id: S.activeId, input });
    if (r.blocked) toast('Blocked by protection: ' + (r.filter || r.url));
    S.tabs = await invoke('list_tabs');
    renderTabs(); renderAddress();
    await refreshBlocker();
  } catch (e) { toast('Navigation failed: ' + e); }
}
async function goBack() { await invoke('go_back', { id: S.activeId }); refreshLight(); }
async function goForward() { await invoke('go_forward', { id: S.activeId }); refreshLight(); }
async function reload() { await invoke('reload', { id: S.activeId }); }

/* --------------------------- panels --------------------------- */
let openPanelName = null;
async function openPanel(name) {
  closePanels(false);
  openPanelName = name;
  $('#panels').classList.remove('hidden');
  const panel = $(`.panel[data-name="${name}"]`);
  if (panel) panel.classList.add('active');
  try { await invoke('set_content_visible', { visible: false }); } catch (_) {}
  if (name === 'privacy') await renderPrivacy();
  if (name === 'blocker') await refreshBlocker(true);
  if (name === 'vpn') { await renderVpn(); await renderVpnGate(); }
  if (name === 'extensions') await renderExtensions();
  if (name === 'history') await renderHistory();
  if (name === 'bookmarks') await renderBookmarks();
  if (name === 'downloads') await renderDownloads();
  if (name === 'passwords') await renderVault();
  if (name === 'companions') await renderCompanions();
  if (name === 'settings') await renderSettings();
}
async function closePanels(showContent = true) {
  $('#panels').classList.add('hidden');
  $('#sidebar-panel').classList.add('hidden');
  $$('.panel').forEach(p => p.classList.remove('active'));
  openPanelName = null;
  if (showContent) { try { await invoke('set_content_visible', { visible: true }); } catch (_) {} }
}

/* --------------------------- blocker --------------------------- */
async function refreshBlocker(full) {
  try { S.blocker = await invoke('blocker_stats'); } catch (_) { return; }
  renderShield();
  if (full || openPanelName === 'blocker') {
    const stats = S.blocker.stats || {};
    $('#b-enabled').checked = !!S.blocker.enabled;
    $('#b-count').textContent = S.blocker.filters ?? 0;
    $('#b-blocked').textContent = stats.blocked_total ?? 0;
    $('#b-allowlist').innerHTML = (S.blocker.allowlist || []).map(s => `<span class="chip">${esc(s)}</span>`).join('');
    $('#b-recent').innerHTML = (stats.recent || []).slice().reverse().map(u =>
      `<div class="list-item"><span class="grow">${esc(u)}</span></div>`).join('') || '<div class="muted">No blocks yet.</div>';
  }
}
function renderShield() {
  const n = S.blocker?.stats?.blocked_total ?? 0;
  $('#shield-count').textContent = n > 999 ? '999+' : n;
  $('#p-blocked') && ($('#p-blocked').textContent = n);
  const on = S.blocker?.enabled !== false;
  $('#status-protection').textContent = on ? `◇ Protection active · ${n} blocked` : '◇ Protection paused';
}
async function toggleBlocker(on) {
  await invoke('blocker_toggle', { enabled: on });
  await refreshBlocker(true);
}

/* --------------------------- privacy --------------------------- */
async function renderPrivacy() {
  await refreshBlocker(true);
  const stats = S.blocker?.stats || {};
  $('#p-toggle-blocker').checked = S.blocker?.enabled !== false;
  $('#p-filters').textContent = S.blocker?.filters ?? 0;
  $('#p-allowed').textContent = stats.allowed_total ?? 0;
  const top = Object.entries(stats.blocked_by_domain || {}).sort((a,b)=>b[1]-a[1]).slice(0,8);
  $('#p-top-domains').innerHTML = top.map(([d,n]) =>
    `<div class="list-item"><span class="grow">${esc(d)}</span><span class="chip">${n}</span></div>`).join('') || '<div class="muted">Nothing blocked yet.</div>';
  if (S.settings) {
    $('#p-toggle-https').checked = !!S.settings.https_upgrade;
    $('#p-toggle-cookies').checked = !!S.settings.block_third_party_cookies;
    $('#p-toggle-fp').checked = !!S.settings.fingerprinting_protection;
  }
  try {
    const priv = await invoke('privacy_state');
    $('#perm-list').innerHTML = (priv.permissions || []).map(([k,v]) =>
      `<div class="list-item"><span class="grow">${esc(k)}</span><span class="chip">${esc(v)}</span></div>`).join('') || '<div class="muted">No stored permissions.</div>';
  } catch (_) {}
  try {
    const vpn = await invoke('vpn_state');
    $('#p-vpn').textContent = vpnLabel(vpn.status) === 'Disconnected' ? 'Off' : 'On';
  } catch (_) {}
  try {
    const sb = await invoke('safe_browsing_state');
    $('#sb-enabled').checked = !!sb.enabled;
    $('#sb-status').textContent = sb.remote_available
      ? 'Remote threat service configured and key present.'
      : 'Local denylist active. No API key configured — set VYRO_SAFE_BROWSING_KEY for remote checks.';
    $('#sb-domains').innerHTML = sb.domains.map(d => `<span class="chip">${esc(d)}</span>`).join('');
  } catch (_) {}
}

/* --------------------------- vpn --------------------------- */
function vpnLabel(s) {
  if (!s) return 'Unknown';
  if (typeof s === 'string') return s;
  if (s.Connected) return `Connected · ${s.Connected.server_id}`;
  if (s.Connecting) return 'Connecting…';
  if (s.Reconnecting) return 'Reconnecting…';
  if (s.Error) return 'Error: ' + s.Error;
  return 'Disconnected';
}
async function renderVpnGate(refresh = false) {
  try {
    if (refresh || !S.vpnGate.length) S.vpnGate = await invoke('vpn_gate_servers');
    const countries = [...new Set(S.vpnGate.map(s => s.country_code))].sort();
    const csel = $('#vg-country');
    const prevCountry = csel.value;
    csel.innerHTML = countries.map(c => `<option value="${esc(c)}">${esc(c)}</option>`).join('');
    if (prevCountry && countries.includes(prevCountry)) csel.value = prevCountry;
    const fill = () => {
      const list = S.vpnGate.filter(s => !csel.value || s.country_code === csel.value);
      $('#vg-server').innerHTML = list.map(s =>
        `<option value="${s.index}">${esc(s.hostname)} · ${esc(s.ip)} · ${esc(s.ping)}ms</option>`).join('');
      $('#vg-list').innerHTML = list.slice(0, 12).map(s =>
        `<div class="list-item"><span class="grow"><b>${esc(s.hostname)}</b> <span class="sub">${esc(s.country_code)}</span><br><span class="sub">${esc(s.ip)} · ${esc(s.ping)} ms · ${esc(s.speed)}</span></span></div>`).join('');
    };
    csel.onchange = fill; fill();
  } catch (e) { toast('VPN Gate: ' + e); }
}
async function renderVpn() {
  try { S.vpn = await invoke('vpn_state'); } catch (e) { return toast(e); }
  const v = S.vpn;
  $('#vpn-status').textContent = vpnLabel(v.status);
  $('#vpn-kill').checked = !!v.kill_switch;
  $('#vpn-dns').checked = !!v.dns_leak_protection;
  $('#vpn-auto').checked = !!v.auto_reconnect;
  const psel = $('#vpn-provider');
  psel.innerHTML = v.providers.map(p => `<option value="${esc(p.id)}">${esc(p.name)}</option>`).join('') || '<option value="">No provider configured</option>';
  const fillServers = () => {
    const p = v.providers.find(x => x.id === psel.value);
    $('#vpn-server').innerHTML = (p?.servers || []).map(s =>
      `<option value="${esc(s.id)}">${esc(s.city)}, ${esc(s.country)}</option>`).join('');
  };
  psel.onchange = fillServers; fillServers();
  $('#vpn-diag').innerHTML = v.diagnostics.map(d =>
    `<div class="list-item"><span class="${d.ok ? 'diag-ok' : 'diag-bad'}">${d.ok ? '✓' : '✕'}</span><span class="grow">${esc(d.name)}</span><span class="sub">${esc(d.detail)}</span></div>`).join('');
  $('#status-vpn').textContent = 'VPN: ' + vpnLabel(v.status);
}

/* --------------------------- extensions --------------------------- */
async function renderExtensions() {
  try { S.extensions = await invoke('extensions_list'); } catch (e) { return toast(e); }
  const sup = S.extensions.support || {};
  $('#ext-support').innerHTML = `<b>Engine:</b> ${esc(sup.engine || '')}<br>` +
    `<b>Chrome Web Store extensions:</b> ${sup.chrome_web_store_supported ? 'supported' : 'NOT supported by this engine'}<br>` +
    `<span class="muted">${esc(sup.note || '')}</span>`;
  $('#ext-list').innerHTML = (S.extensions.items || []).map(e =>
    `<div class="list-item"><span class="grow"><b>${esc(e.name)}</b> <span class="sub">v${esc(e.version)}</span><br><span class="sub">${esc(e.permissions.join(', '))}</span></span>
     <label class="row"><span>Enabled</span><input type="checkbox" class="switch ext-toggle" data-id="${esc(e.id)}" ${e.enabled ? 'checked' : ''}></label>
     <button class="btn ghost ext-remove" data-id="${esc(e.id)}">Remove</button></div>`).join('') || '<div class="muted">No extensions installed.</div>';
  $$('.ext-toggle').forEach(el => el.onchange = async () => { await invoke('extensions_toggle', { id: el.dataset.id, enabled: el.checked }); });
  $$('.ext-remove').forEach(el => el.onclick = async () => { await invoke('extensions_remove', { id: el.dataset.id }); renderExtensions(); });
}

/* --------------------------- history / bookmarks / downloads --------------------------- */
async function renderHistory(query = '') {
  try {
    const items = await invoke('history_list', { query: query || null });
    $('#hist-list').innerHTML = items.slice().reverse().map(h =>
      `<div class="list-item"><span class="grow">${esc(h.title || h.url)}<br><span class="sub">${esc(h.url)}</span></span>
       <button class="btn ghost hist-open" data-url="${esc(h.url)}">Open</button></div>`).join('') || '<div class="muted">No history yet.</div>';
    $$('.hist-open').forEach(b => b.onclick = () => { closePanels(); newTab(b.dataset.url); });
  } catch (e) { toast(e); }
}
async function renderBookmarks() {
  try {
    const data = await invoke('bookmarks_list');
    $('#bm-list').innerHTML = data.items.map(b =>
      `<div class="list-item"><span class="grow">${esc(b.title)}<br><span class="sub">${esc(b.url)} · ${esc(b.folder)}</span></span>
       <button class="btn ghost bm-open" data-url="${esc(b.url)}">Open</button>
       <button class="btn ghost bm-del" data-id="${b.id}">✕</button></div>`).join('') || '<div class="muted">No bookmarks yet.</div>';
    $$('.bm-open').forEach(b => b.onclick = () => { closePanels(); newTab(b.dataset.url); });
    $$('.bm-del').forEach(b => b.onclick = async () => { await invoke('bookmarks_remove', { id: Number(b.dataset.id) }); renderBookmarks(); });
  } catch (e) { toast(e); }
}
async function renderDownloads() {
  try {
    const items = await invoke('downloads_list');
    $('#dl-list').innerHTML = items.map(d =>
      `<div class="list-item"><span class="grow">${esc(d.filename)}<br><span class="sub">${esc(d.state)}</span></span></div>`).join('') || '<div class="muted">No downloads yet.</div>';
  } catch (e) { toast(e); }
}

/* --------------------------- vault & companions --------------------------- */
async function renderVault() {
  try {
    const st = await invoke('vault_state');
    $('#vault-status').textContent = !st.initialized
      ? 'No vault yet — create a master password.'
      : st.unlocked ? `Unlocked · ${st.count} credential(s)` : 'Locked';
    const items = st.unlocked ? await invoke('vault_list') : [];
    $('#vault-list').innerHTML = items.map(([id, service, user]) =>
      `<div class="list-item"><span class="grow"><b>${esc(service)}</b><br><span class="sub">${esc(user)}</span></span>
       <button class="btn ghost vault-show" data-id="${id}">Reveal</button>
       <button class="btn ghost vault-del" data-id="${id}">✕</button></div>`).join('') || '<div class="muted">No saved credentials.</div>';
    $$('.vault-show').forEach(b => b.onclick = async () => {
      try { const pw = await invoke('vault_reveal', { id: Number(b.dataset.id) }); toast(`Password: ${pw}`); }
      catch (e) { toast(e); }
    });
    $$('.vault-del').forEach(b => b.onclick = async () => {
      await invoke('vault_remove', { id: Number(b.dataset.id) }); renderVault();
    });
  } catch (e) { toast(e); }
}
async function renderCompanions() {
  try {
    const data = await invoke('companion_list');
    $('#companion-note').innerHTML = `<b>VYRO companion format.</b> ${esc(data.note || '')}`;
    $('#comp-list').innerHTML = data.items.map(c =>
      `<div class="list-item"><span class="grow"><b>${esc(c.name)}</b> <span class="sub">${esc(c.id)}</span><br><span class="sub">${esc(c.matches.join(', '))}</span></span>
       <label class="row"><span>Enabled</span><input type="checkbox" class="switch comp-toggle" data-id="${esc(c.id)}" ${c.enabled ? 'checked' : ''}></label>
       <button class="btn ghost comp-del" data-id="${esc(c.id)}">Remove</button></div>`).join('') || '<div class="muted">No companions installed.</div>';
    $$('.comp-toggle').forEach(el => el.onchange = async () => { await invoke('companion_toggle', { id: el.dataset.id, enabled: el.checked }); });
    $$('.comp-del').forEach(el => el.onclick = async () => { await invoke('companion_remove', { id: el.dataset.id }); renderCompanions(); });
  } catch (e) { toast(e); }
}

/* --------------------------- settings --------------------------- */
function setSegment(id, value) {
  $$(`#${id} button`).forEach(b => b.classList.toggle('active', b.dataset.value === value));
}
function clearSaveBar() {
  $('#save-bar').classList.remove('show');
  $('#save-status').textContent = 'All changes saved';
}
function markSettingsDirty() {
  $('#save-bar').classList.add('show');
  $('#save-status').textContent = 'Unsaved changes';
}
function renderSettings() {
  const s = S.settings; if (!s) return;
  setSegment('s-theme', cap(s.theme));
  setSegment('s-engine', s.search_engine || 'DuckDuckGo');
  $('#s-accent').value = s.accent_color || '#7c5cff';
  $$('#s-swatches .swatch:not(.custom)').forEach(b =>
    b.classList.toggle('active', b.dataset.color.toLowerCase() === (s.accent_color || '').toLowerCase()));
  $('#s-home').value = s.homepage || '';
  $('#s-restore').checked = !!s.restore_session;
  $('#s-bookmarksbar').checked = !!s.show_bookmarks_bar;
  $('#s-tracking').checked = !!s.tracking_protection;
  $('#s-safe').checked = !!s.safe_browsing;
  $('#s-clear').checked = !!s.clear_on_exit;
  $('#s-downloaddir').value = s.download_dir || '';
  clearSaveBar();
}
const cap = (s) => s ? s[0].toUpperCase() + s.slice(1) : 'Dark';
function applyTheme(s) {
  if (!s) return;
  document.body.classList.toggle('light', String(s.theme).toLowerCase() === 'light');
  document.documentElement.style.setProperty('--accent', s.accent_color || '#7c5cff');
}
async function saveSettings() {
  const s = { ...S.settings };
  s.theme = $('#s-theme button.active')?.dataset.value || 'Dark';
  s.accent_color = $('#s-accent').value;
  s.homepage = $('#s-home').value;
  s.search_engine = $('#s-engine button.active')?.dataset.value || 'DuckDuckGo';
  s.restore_session = $('#s-restore').checked;
  s.show_bookmarks_bar = $('#s-bookmarksbar').checked;
  s.tracking_protection = $('#s-tracking').checked;
  s.safe_browsing = $('#s-safe').checked;
  s.clear_on_exit = $('#s-clear').checked;
  s.download_dir = $('#s-downloaddir').value.trim() || s.download_dir;
  try {
    await invoke('settings_update', { value: s });
    await invoke('downloads_set_dir', { path: s.download_dir });
    S.settings = s; applyTheme(s); clearSaveBar(); toast('Settings saved');
  } catch (e) { toast('Save failed: ' + e); }
}

/* --------------------------- helpers --------------------------- */
function showTabListMenu(anchor) {
  $$('.tab-menu').forEach(m => m.remove());
  const menu = document.createElement('div');
  menu.className = 'tab-menu';
  const r = anchor.getBoundingClientRect();
  menu.style.left = Math.min(r.left, window.innerWidth - 260) + 'px';
  menu.style.top = (r.bottom + 6) + 'px';
  S.tabs.forEach((t, i) => {
    const b = document.createElement('button');
    b.textContent = `${i + 1}. ${t.group ? t.group + ' · ' : ''}${t.title || t.url || 'New Tab'}`;
    b.onclick = () => { menu.remove(); activateTab(t.id); };
    menu.appendChild(b);
  });
  document.body.appendChild(menu);
  setTimeout(() => {
    const close = e => { if (!menu.contains(e.target) && e.target !== anchor) { menu.remove(); document.removeEventListener('mousedown', close); } };
    document.addEventListener('mousedown', close);
  }, 0);
}
function showTabMenu(x, y, id, group) {
  $$('.tab-menu').forEach(m => m.remove());
  const menu = document.createElement('div');
  menu.className = 'tab-menu';
  menu.style.left = Math.min(x, window.innerWidth - 210) + 'px';
  menu.style.top = Math.min(y, window.innerHeight - 170) + 'px';
  const groups = [...new Set(S.tabs.map(t => t.group).filter(Boolean))];
  const items = [
    ['New group…', async () => {
      const name = prompt('Group name'); if (name) { await invoke('tab_set_group', { id, group: name }); refreshLight(); }
    }],
    ...groups.map(g => [g, async () => { await invoke('tab_set_group', { id, group: g }); refreshLight(); }]),
    ['Remove from group', async () => { await invoke('tab_set_group', { id, group: null }); refreshLight(); }],
    ['Close tab', () => closeTab(id)],
  ];
  items.forEach(([label, fn]) => {
    const b = document.createElement('button');
    b.textContent = label;
    b.onclick = async () => { menu.remove(); await fn(); };
    menu.appendChild(b);
  });
  document.body.appendChild(menu);
  setTimeout(() => {
    const close = e => { if (!menu.contains(e.target)) { menu.remove(); document.removeEventListener('mousedown', close); } };
    document.addEventListener('mousedown', close);
  }, 0);
}
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, c => ({ '&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;' })[c]);
async function patch(key, value) {
  S.settings[key] = value;
  try { S.settings = await invoke('settings_patch', { value: { [key]: value } }); applyTheme(S.settings); }
  catch (e) { toast('Could not save: ' + e); }
}
let toastTimer;
function toast(msg) {
  let el = $('#toast');
  if (!el) { el = document.createElement('div'); el.id = 'toast'; document.body.appendChild(el); }
  el.textContent = msg; el.className = 'show';
  clearTimeout(toastTimer); toastTimer = setTimeout(() => el.className = '', 3200);
}

/* --------------------------- bindings --------------------------- */
function bindUI() {
  $('#new-tab').onclick = () => newTab();
  $('#tab-count').onclick = e => showTabListMenu(e.currentTarget);
  $('#tabs-left').onclick = () => { $('#tabstrip').scrollLeft -= 260; };
  $('#tabs-right').onclick = () => { $('#tabstrip').scrollLeft += 260; };
  let popoverOpen = false;
  $('#downloads-btn').onclick = async () => {
    popoverOpen = !popoverOpen;
    try { await invoke('download_popover', { show: popoverOpen }); }
    catch (e) { popoverOpen = false; toast(e); }
  };
  $('#nav-back').onclick = goBack;
  $('#nav-forward').onclick = goForward;
  $('#nav-reload').onclick = reload;
  $('#nav-home').onclick = () => navigate(S.settings?.homepage || 'https://duckduckgo.com');
  $('#shield').onclick = () => openPanel('privacy');
  $('#bookmark-current').onclick = async () => {
    const t = activeTab(); if (!t || !t.url) return;
    await invoke('bookmarks_add', { title: t.title || t.url, url: t.url, folder: 'Bookmarks bar' });
    toast('Bookmarked');
  };
  $$('[data-panel]').forEach(b => { if (b.classList.contains('side-item')) return; b.onclick = () => openPanel(b.dataset.panel); });
  $$('.side-item').forEach(b => b.onclick = () => openPanel(b.dataset.panel));
  $$('[data-close]').forEach(b => b.onclick = () => closePanels());

  $('#win-min').onclick = () => invoke('plugin:window|minimize', { label: 'main' }).catch(() => {});
  $('#win-max').onclick = () => invoke('plugin:window|toggle_maximize', { label: 'main' }).catch(() => {});
  $('#win-close').onclick = () => invoke('plugin:window|close', { label: 'main' }).catch(() => {});

  $('#p-toggle-blocker').onchange = e => toggleBlocker(e.target.checked);
  $('#p-toggle-https').onchange = e => patch('https_upgrade', e.target.checked);
  $('#p-toggle-cookies').onchange = e => patch('block_third_party_cookies', e.target.checked);
  $('#p-toggle-fp').onchange = e => patch('fingerprinting_protection', e.target.checked);
  $('#perm-save').onclick = async () => {
    await invoke('privacy_set_permission', { origin: $('#perm-origin').value, permission: $('#perm-kind').value, decision: $('#perm-decision').value });
    renderPrivacy(); toast('Permission saved');
  };

  $('#b-enabled').onchange = e => toggleBlocker(e.target.checked);
  $('#b-allow').onclick = async () => { await invoke('blocker_allow_site', { site: $('#b-site').value, allowed: true }); refreshBlocker(true); };
  $('#b-unallow').onclick = async () => { await invoke('blocker_allow_site', { site: $('#b-site').value, allowed: false }); refreshBlocker(true); };
  $('#b-add-filters').onclick = async () => {
    const n = await invoke('blocker_add_filters', { text: $('#b-filters').value });
    toast(`${n} rules added`); $('#b-filters').value = ''; refreshBlocker(true);
  };

  $('#vpn-connect').onclick = async () => {
    try { await invoke('vpn_connect', { provider: $('#vpn-provider').value, server: $('#vpn-server').value }); toast('Connected'); }
    catch (e) { toast(String(e)); }
    renderVpn();
  };
  $('#vpn-disconnect').onclick = async () => { try { await invoke('vpn_disconnect'); } catch (e) { toast(e); } renderVpn(); };
  $('#vpn-kill').onchange = e => patch('vpn_kill_switch', e.target.checked);
  $('#vpn-dns').onchange = e => patch('vpn_dns_leak_protection', e.target.checked);
  $('#vpn-auto').onchange = e => patch('vpn_auto_reconnect', e.target.checked);
  $('#vg-refresh').onclick = () => renderVpnGate(true);
  $('#vg-connect').onclick = async () => {
    try {
      await invoke('vpn_gate_connect', { index: Number($('#vg-server').value), openvpnPath: $('#vg-openvpn').value || null });
      toast('VPN Gate connected'); renderVpn();
    } catch (e) { toast(String(e)); }
  };
  $('#vg-bundled').onclick = async () => {
    try {
      await invoke('vpn_gate_bundled_connect', { openvpnPath: $('#vg-openvpn').value || null });
      toast('Bundled sample connected'); renderVpn();
    } catch (e) { toast(String(e)); }
  };
  $('#vpn-add-provider').onclick = async () => {
    try { await invoke('vpn_add_provider', { jsonText: $('#vpn-provider-json').value }); toast('Provider added'); renderVpn(); }
    catch (e) { toast('Invalid provider JSON: ' + e); }
  };

  $('#ext-install').onclick = async () => {
    try { await invoke('extensions_install', { manifest: $('#ext-manifest').value, path: 'unpacked', id: $('#ext-id').value || ('ext-' + Date.now()) }); renderExtensions(); }
    catch (e) { toast(e); }
  };

  $('#hist-search').onclick = () => renderHistory($('#hist-query').value);
  $('#hist-query').onkeydown = e => { if (e.key === 'Enter') renderHistory(e.target.value); };
  $('#hist-clear').onclick = async () => { await invoke('history_clear'); renderHistory(); };
  $('#bm-add').onclick = async () => {
    await invoke('bookmarks_add', { title: $('#bm-title').value, url: $('#bm-url').value, folder: $('#bm-folder').value || 'Bookmarks bar' });
    $('#bm-title').value = $('#bm-url').value = ''; renderBookmarks();
  };
  $('#bm-export').onclick = async () => {
    const json = await invoke('bookmarks_export');
    const a = document.createElement('a');
    a.href = URL.createObjectURL(new Blob([json], { type: 'application/json' }));
    a.download = 'vyro-bookmarks.json'; a.click();
  };
  $('#bm-file').onchange = async e => {
    const f = e.target.files[0]; if (!f) return;
    const text = await f.text();
    const n = await invoke('bookmarks_import', { html: text });
    toast(`Imported ${n} bookmarks`); renderBookmarks();
  };

  $('#s-save').onclick = saveSettings;
  $('#s-discard').onclick = () => { renderSettings(); toast('Changes discarded'); };
  ['s-theme', 's-engine'].forEach(id => {
    $$(`#${id} button`).forEach(b => b.onclick = () => {
      setSegment(id, b.dataset.value);
      markSettingsDirty();
      if (id === 's-theme') applyTheme({ ...S.settings, theme: b.dataset.value });
    });
  });
  $$('#s-swatches .swatch:not(.custom)').forEach(b => b.onclick = () => {
    $$('#s-swatches .swatch').forEach(x => x.classList.remove('active'));
    b.classList.add('active');
    $('#s-accent').value = b.dataset.color;
    applyTheme({ ...S.settings, accent_color: b.dataset.color });
    markSettingsDirty();
  });
  $('#s-accent').oninput = e => {
    $$('#s-swatches .swatch').forEach(x => x.classList.remove('active'));
    applyTheme({ ...S.settings, accent_color: e.target.value });
    markSettingsDirty();
  };
  $$('.panel[data-name="settings"] input, .panel[data-name="settings"] textarea').forEach(el => {
    if (el.id === 's-accent') return;
    el.addEventListener('change', markSettingsDirty);
    el.addEventListener('input', markSettingsDirty);
  });

  $('#tool-find').onclick = async () => {
    try { await invoke('find_in_page', { id: S.activeId, query: '' }); } catch (e) { toast(e); }
  };
  $('#tool-reader').onclick = async () => {
    try { await invoke('reader_mode', { id: S.activeId }); toast('Reader mode toggled'); } catch (e) { toast(e); }
  };
  $('#gate-restore').onclick = async () => {
    await invoke('set_network_gate', { enabled: false });
    $('#gate-banner').classList.add('hidden');
    toast('Browsing restored');
  };

  $('#sb-enabled').onchange = async e => {
    await invoke('safe_browsing_toggle', { enabled: e.target.checked });
    renderPrivacy();
  };
  $('#sb-add').onclick = async () => {
    await invoke('safe_browsing_add_domain', { domain: $('#sb-domain').value });
    $('#sb-domain').value = ''; renderPrivacy();
  };

  $('#vault-init').onclick = async () => {
    try { await invoke('vault_init', { master: $('#vault-master').value }); $('#vault-master').value = ''; renderVault(); toast('Vault created'); }
    catch (e) { toast(e); }
  };
  $('#vault-unlock').onclick = async () => {
    try { await invoke('vault_unlock', { master: $('#vault-master').value }); $('#vault-master').value = ''; renderVault(); toast('Vault unlocked'); }
    catch (e) { toast(e); }
  };
  $('#vault-lock').onclick = async () => { await invoke('vault_lock'); renderVault(); };
  $('#vault-add').onclick = async () => {
    try {
      await invoke('vault_add', { service: $('#vault-service').value, username: $('#vault-user').value, password: $('#vault-pass').value });
      $('#vault-service').value = $('#vault-user').value = $('#vault-pass').value = ''; renderVault();
    } catch (e) { toast(e); }
  };
  $('#comp-install').onclick = async () => {
    try {
      await invoke('companion_install', {
        id: $('#comp-id').value || ('comp-' + Date.now()), name: $('#comp-name').value,
        matches: $('#comp-match').value.split(',').map(s => s.trim()).filter(Boolean),
        code: $('#comp-code').value
      });
      $('#comp-id').value = $('#comp-name').value = $('#comp-match').value = $('#comp-code').value = '';
      renderCompanions(); toast('Companion installed; open a new tab to apply it');
    } catch (e) { toast(e); }
  };

  document.addEventListener('keydown', (e) => {
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === 't') { e.preventDefault(); newTab(); }
    else if (mod && e.key.toLowerCase() === 'w') { e.preventDefault(); if (S.activeId != null) closeTab(S.activeId); }
    else if (mod && e.key.toLowerCase() === 'l') { e.preventDefault(); $('#address').focus(); $('#address').select(); }
    else if (mod && e.key.toLowerCase() === 'r') { e.preventDefault(); reload(); }
    else if (mod && e.key.toLowerCase() === 'd') { e.preventDefault(); $('#bookmark-current').click(); }
    else if (mod && e.key.toLowerCase() === 'h') { e.preventDefault(); openPanel('history'); }
    else if (mod && e.key === ',') { e.preventDefault(); openPanel('settings'); }
    else if (e.key === 'Escape') { closePanels(); }
  });
}

boot();
