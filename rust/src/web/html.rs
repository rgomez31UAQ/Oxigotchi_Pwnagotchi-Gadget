//! Dashboard HTML template — embedded single-page web UI.

/// The full dashboard HTML/CSS/JS served at GET /.
pub const DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0, user-scalable=no">
<title>oxigotchi</title>
<style>
*{box-sizing:border-box;margin:0;padding:0}
body{background:#1a1a2e;color:#e0e0e0;font-family:'SF Mono','Fira Code','Cascadia Code',monospace;font-size:14px;padding:12px;max-width:600px;margin:0 auto}
h1{color:#00d4aa;font-size:20px;text-align:center;margin-bottom:4px;letter-spacing:1px}
.section-label{color:#555;font-size:10px;text-transform:uppercase;letter-spacing:2px;margin:16px 0 6px;padding-left:4px}
.card{background:#16213e;border-radius:12px;padding:16px;margin-bottom:12px}
.card-title{color:#00d4aa;font-size:15px;font-weight:bold;margin-bottom:12px;padding-bottom:8px;border-bottom:1px solid #0f3460}
.status-grid{display:grid;grid-template-columns:1fr 1fr;gap:6px 16px}
.status-grid .label{color:#888;font-size:12px}
.status-grid .value{color:#e0e0e0;font-size:13px;font-weight:bold}
.stat-row{display:flex;flex-wrap:wrap;gap:8px}
.stat{text-align:center;flex:1;min-width:60px}
.stat .label{color:#888;font-size:11px}
.stat .value{color:#00d4aa;font-size:18px;font-weight:bold}
.health-row{display:flex;flex-wrap:wrap;gap:10px;margin-bottom:4px}
.health-item{display:flex;align-items:center;gap:6px;font-size:13px}
.dot{width:10px;height:10px;border-radius:50%;display:inline-block}
.dot-green{background:#00d4aa}
.dot-red{background:#e94560}
.dot-gray{background:#555}
.dot-yellow{background:#f0c040}
.toggle-row{display:flex;align-items:center;justify-content:space-between;padding:10px 0;border-bottom:1px solid #0f3460}
.toggle-row:last-child{border-bottom:none}
.toggle-info{flex:1;margin-right:12px}
.toggle-label{font-size:14px;font-weight:bold;color:#e0e0e0}
.toggle-desc{font-size:11px;color:#888;margin-top:2px}
.switch{position:relative;width:50px;height:28px;flex-shrink:0}
.switch input{opacity:0;width:0;height:0}
.slider{position:absolute;cursor:pointer;top:0;left:0;right:0;bottom:0;background:#555;border-radius:28px;transition:.25s}
.slider:before{position:absolute;content:"";height:22px;width:22px;left:3px;bottom:3px;background:#fff;border-radius:50%;transition:.25s}
input:checked+.slider{background:#00d4aa}
input:checked+.slider:before{transform:translateX(22px)}
.rate-btns{display:flex;gap:8px;margin-top:8px}
.rate-btn{flex:1;padding:14px 0;border:2px solid #0f3460;border-radius:10px;background:transparent;color:#e0e0e0;font-size:18px;font-weight:bold;font-family:inherit;cursor:pointer;text-align:center;transition:.2s}
.rate-btn.active{background:#0f3460;color:#00d4aa;border-color:#00d4aa}
.rate-btn.risky{border-color:#e67e22;color:#e67e22}
.rate-btn.risky.active{background:#5a3000;color:#e67e22;border-color:#e67e22}
.rate-btn:active{transform:scale(0.95)}
.mode-btns{display:flex;gap:8px;margin-top:8px}
.mode-btn{flex:1;padding:14px 0;border:2px solid #0f3460;border-radius:10px;background:transparent;color:#e0e0e0;font-size:16px;font-weight:bold;font-family:inherit;cursor:pointer;text-align:center;transition:.2s}
.mode-btn.active{background:#00d4aa;color:#1a1a2e;border-color:#00d4aa}
.mode-btn:active{transform:scale(0.95)}
.action-btns{display:flex;flex-wrap:wrap;gap:8px}
.action-btn{flex:1;min-width:100px;padding:14px 8px;border:none;border-radius:10px;font-family:inherit;font-size:13px;font-weight:bold;cursor:pointer;text-align:center;transition:.2s}
.action-btn:active{transform:scale(0.95)}
.btn-restart{background:#0f3460;color:#00d4aa}
.btn-stop{background:#e94560;color:#fff}
.btn-warn{background:#f0c040;color:#1a1a2e}
.captures-list{max-height:200px;overflow-y:auto;margin-top:8px}
.capture-item{font-size:12px;color:#aaa;padding:4px 0;border-bottom:1px solid #0f346033}
.capture-item:last-child{border-bottom:none}
.toast{position:fixed;bottom:20px;left:50%;transform:translateX(-50%);background:#00d4aa;color:#1a1a2e;padding:10px 20px;border-radius:8px;font-size:13px;font-weight:bold;opacity:0;transition:opacity .3s;pointer-events:none;z-index:999}
.toast.show{opacity:1}
.progress-bar{height:6px;background:#0f3460;border-radius:3px;overflow:hidden;margin-top:4px}
.progress-fill{height:100%;background:#00d4aa;border-radius:3px;transition:width .3s}
.grid-2{display:grid;grid-template-columns:1fr 1fr;gap:8px}
.sub{color:#888;font-size:11px;margin-bottom:8px}
.ap-table{width:100%;border-collapse:collapse;font-size:12px;margin-top:8px}
.ap-table th{color:#888;font-size:11px;text-align:left;padding:4px 6px;border-bottom:1px solid #0f3460}
.ap-table td{padding:4px 6px;border-bottom:1px solid #0f346033;color:#e0e0e0}
.ap-table tr:last-child td{border-bottom:none}
.ap-scroll{max-height:300px;overflow-y:auto;margin-top:4px}
.wl-input{background:#0a1628;color:#e0e0e0;border:1px solid #0f3460;border-radius:6px;padding:8px 10px;font-size:12px;font-family:inherit;width:100%}
.wl-input:focus{outline:none;border-color:#00d4aa}
.wl-select{background:#0a1628;color:#e0e0e0;border:1px solid #0f3460;border-radius:6px;padding:8px 10px;font-size:12px;font-family:inherit}
.wl-btn{padding:8px 16px;border:none;border-radius:6px;font-family:inherit;font-size:12px;font-weight:bold;cursor:pointer;transition:.2s}
.wl-btn:active{transform:scale(0.95)}
.wl-btn-add{background:#00d4aa;color:#1a1a2e}
.wl-btn-rm{background:#e94560;color:#fff;padding:4px 10px;font-size:11px;border:none;border-radius:4px;cursor:pointer}
.wl-btn-rm:active{transform:scale(0.95)}
.ch-input{background:#0a1628;color:#e0e0e0;border:1px solid #0f3460;border-radius:6px;padding:8px 10px;font-size:12px;font-family:inherit;width:100%}
.ch-input:focus{outline:none;border-color:#00d4aa}
.ch-slider{width:100%;accent-color:#00d4aa;margin:8px 0}
.logs-pre{background:#0a1628;color:#aaa;font-size:11px;font-family:'SF Mono','Fira Code',monospace;padding:10px;border-radius:6px;max-height:300px;overflow-y:auto;white-space:pre-wrap;word-break:break-all;margin-top:8px}
.collapse-btn{background:none;border:1px solid #0f3460;color:#888;border-radius:6px;padding:6px 12px;font-size:12px;font-family:inherit;cursor:pointer;transition:.2s}
.collapse-btn:hover{border-color:#00d4aa;color:#00d4aa}
@media(max-width:400px){.grid-2{grid-template-columns:1fr}.stat-row{gap:4px}.stat .value{font-size:15px}}
</style>
</head>
<body>
<h1>Oxigotchi Dashboard</h1>
<div style="text-align:center;color:#888;font-size:11px;margin:-2px 0 10px">Rusty Oxigotchi &mdash; WiFi capture bull</div>

<!-- ═══════ AT-A-GLANCE ═══════ -->
<div class="section-label">At-a-Glance</div>

<!-- 1. Live Display (e-ink preview) -->
<div class="card" id="card-eink" style="text-align:center">
<div class="card-title">Live Display</div>
<div style="padding:8px;background:#fff;display:inline-block;border-radius:4px"><img id="eink-img" src="/api/display.png" alt="e-ink" style="width:250px;height:122px;image-rendering:pixelated"></div>
</div>

<!-- 2. Mode switch -->
<div class="card" id="card-mode">
<div class="card-title">Mode</div>
<div class="sub">RAGE = all attacks max aggression. SAFE = passive scanning only.</div>
<div class="mode-btns">
<button class="mode-btn active" id="mode-rage" onclick="switchMode('RAGE')">RAGE</button>
<button class="mode-btn" id="mode-safe" onclick="switchMode('SAFE')">SAFE</button>
</div>
</div>

<!-- 3. Core stats -->
<div class="card" id="card-stats">
<div class="card-title">Core Stats</div>
<div class="stat-row">
<div class="stat"><div class="label">CH</div><div class="value" id="s-ch">-</div></div>
<div class="stat"><div class="label">APS</div><div class="value" id="s-aps">-</div></div>
<div class="stat"><div class="label">PWND</div><div class="value" id="s-pwnd">-</div></div>
<div class="stat"><div class="label">EPOCH</div><div class="value" id="s-epoch">-</div></div>
<div class="stat"><div class="label">UPTIME</div><div class="value" id="s-uptime">-</div></div>
<div class="stat"><div class="label">RATE</div><div class="value" id="s-rate">-</div></div>
</div>
</div>

<!-- ═══════ HARDWARE HEALTH ═══════ -->
<div class="section-label">Hardware</div>

<div class="grid-2">

<!-- 4. Battery -->
<div class="card" id="card-battery">
<div class="card-title">Battery</div>
<div class="status-grid">
<div class="label">Level</div><div class="value" id="bat-level">-</div>
<div class="label">State</div><div class="value" id="bat-state">-</div>
<div class="label">Voltage</div><div class="value" id="bat-voltage">-</div>
</div>
<div class="progress-bar"><div class="progress-fill" id="bat-bar" style="width:0%"></div></div>
</div>

<!-- 5. System Info -->
<div class="card" id="card-system">
<div class="card-title">System</div>
<div class="status-grid">
<div class="label">CPU Temp</div><div class="value" id="sys-temp">-</div>
<div class="label">CPU</div><div class="value" id="sys-cpu">-</div>
<div class="label">Memory</div><div class="value" id="sys-mem">-</div>
<div class="label">Disk</div><div class="value" id="sys-disk">-</div>
<div class="label">Uptime</div><div class="value" id="sys-uptime">-</div>
<div class="label">GPS</div><div class="value" id="sys-gps">-</div>
</div>
</div>

</div>

<!-- ═══════ HUNTING ═══════ -->
<div class="section-label">Hunting</div>

<!-- 6. WiFi -->
<div class="card" id="card-wifi">
<div class="card-title">WiFi</div>
<div class="sub">Monitor mode status and channel info.</div>
<div class="status-grid">
<div class="label">State</div><div class="value" id="wifi-state">-</div>
<div class="label">Channel</div><div class="value" id="wifi-ch">-</div>
<div class="label">APs Tracked</div><div class="value" id="wifi-aps">-</div>
<div class="label">Channels</div><div class="value" id="wifi-channels">-</div>
<div class="label">Dwell</div><div class="value" id="wifi-dwell">-</div>
</div>
</div>

<!-- 7. Nearby Networks -->
<div class="card" id="card-aps">
<div class="card-title">Nearby Networks</div>
<div class="sub">Access points detected by monitor mode, sorted by signal strength.</div>
<div class="ap-scroll">
<table class="ap-table" id="ap-table">
<thead><tr><th>SSID</th><th>BSSID</th><th>RSSI</th><th>CH</th><th>Cli</th><th>Status</th></tr></thead>
<tbody id="ap-tbody"><tr><td colspan="6" style="color:#555">Loading...</td></tr></tbody>
</table>
</div>
</div>

<!-- 8. Attack controls -->
<div class="card" id="card-attacks">
<div class="card-title">Attack Types</div>
<div style="color:#00d4aa;font-size:11px;margin-bottom:10px;padding:8px;background:#0f346033;border-radius:6px">All 6 ON is the sweet spot &mdash; they complement each other.</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Deauth</div><div class="toggle-desc">Kick clients to capture reconnection handshakes</div></div>
<label class="switch"><input type="checkbox" id="atk-deauth" checked onchange="toggleAttack('deauth',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">PMKID</div><div class="toggle-desc">Grab router password hashes without clients</div></div>
<label class="switch"><input type="checkbox" id="atk-pmkid" checked onchange="toggleAttack('pmkid',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">CSA</div><div class="toggle-desc">Trick clients into switching channels</div></div>
<label class="switch"><input type="checkbox" id="atk-csa" checked onchange="toggleAttack('csa',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Disassociation</div><div class="toggle-desc">Catches clients that resist deauth</div></div>
<label class="switch"><input type="checkbox" id="atk-disassoc" checked onchange="toggleAttack('disassoc',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Anon Reassoc</div><div class="toggle-desc">Capture PMKID from stubborn routers</div></div>
<label class="switch"><input type="checkbox" id="atk-anon_reassoc" checked onchange="toggleAttack('anon_reassoc',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Rogue M2</div><div class="toggle-desc">Fake AP trick for handshakes</div></div>
<label class="switch"><input type="checkbox" id="atk-rogue_m2" checked onchange="toggleAttack('rogue_m2',this.checked)"><span class="slider"></span></label>
</div>

<div style="margin-top:12px;padding-top:10px;border-top:1px solid #0f3460">
<div style="font-size:12px;color:#888;margin-bottom:4px">Attack Rate</div>
<div class="sub">Rate 1 is max safe for BCM43436B0. Higher rates cause firmware crashes.</div>
<div class="rate-btns">
<button class="rate-btn active" id="rate-1" onclick="setRate(1)">1<br><span style="font-size:10px;font-weight:normal;color:#888">Safe</span></button>
<button class="rate-btn risky" id="rate-2" onclick="setRate(2)">2<br><span style="font-size:10px;font-weight:normal">Risky</span></button>
<button class="rate-btn risky" id="rate-3" onclick="setRate(3)">3<br><span style="font-size:10px;font-weight:normal">Danger</span></button>
</div>
</div>
</div>

<!-- 9. Channel Config + Autohunt -->
<div class="card" id="card-channels">
<div class="card-title">Channel Config</div>
<div class="sub">Configure which channels to scan and dwell time per channel.</div>

<div class="toggle-row" style="border-bottom:1px solid #0f3460;padding-bottom:10px;margin-bottom:10px">
<div class="toggle-info"><div class="toggle-label">Autohunt</div><div class="toggle-desc">Let AO automatically pick channels to hunt on</div></div>
<label class="switch"><input type="checkbox" id="autohunt-toggle" checked onchange="toggleAutohunt(this.checked)"><span class="slider"></span></label>
</div>

<div style="margin-bottom:8px">
<div style="font-size:12px;color:#888;margin-bottom:4px">Channels (comma-separated)</div>
<input type="text" id="ch-list" class="ch-input" placeholder="1,6,11" value="">
</div>
<div style="margin-bottom:8px">
<div style="font-size:12px;color:#888;margin-bottom:4px">Dwell Time: <span id="ch-dwell-val">2000</span>ms</div>
<input type="range" id="ch-dwell" class="ch-slider" min="500" max="10000" step="100" value="2000" oninput="document.getElementById('ch-dwell-val').textContent=this.value">
</div>
<div style="color:#e67e22;font-size:11px;padding:6px 8px;background:#5a300033;border-radius:6px;margin-bottom:8px">Warning: Some channels may cause BCM43436B0 firmware crashes. Stick to 1,6,11 for stability.</div>
<button class="wl-btn wl-btn-add" onclick="applyChannels()">Apply</button>
</div>

<!-- ═══════ LOOT ═══════ -->
<div class="section-label">Loot</div>

<!-- 10. Captures (merged: stats + list + download) -->
<div class="card" id="card-captures">
<div class="card-title">Captures</div>
<div class="sub">Validated capture files from AO monitor mode.</div>
<div class="status-grid" style="margin-bottom:8px">
<div class="label">Total Files</div><div class="value" id="cap-total">-</div>
<div class="label">Handshakes</div><div class="value" id="cap-hs">-</div>
<div class="label">Pending Upload</div><div class="value" id="cap-pending">-</div>
<div class="label">Total Size</div><div class="value" id="cap-size">-</div>
</div>
<div class="action-btns" style="margin-bottom:8px">
<a href="/api/download/all" class="action-btn btn-restart" style="text-decoration:none;text-align:center">Download All (ZIP)</a>
</div>
<div class="captures-list" id="cap-list"><div style="color:#555;font-size:12px">Loading...</div></div>
</div>

<!-- 11. Cracked passwords -->
<div class="card" id="card-cracked">
<div class="card-title">Cracked Passwords</div>
<div class="sub">Passwords cracked from captured handshakes.</div>
<div id="cracked-list"><div style="color:#555;font-size:12px">No cracked passwords yet</div></div>
</div>

<!-- ═══════ CONNECTIVITY ═══════ -->
<div class="section-label">Connectivity</div>

<!-- 12. Bluetooth -->
<div class="card" id="card-bt">
<div class="card-title">Bluetooth</div>
<div class="status-grid" style="margin-bottom:10px">
<div class="label">Status</div><div class="value" id="bt-status">-</div>
<div class="label">Device</div><div class="value" id="bt-device">-</div>
<div class="label">IP</div><div class="value" id="bt-ip">-</div>
<div class="label">Internet</div><div class="value" id="bt-internet">-</div>
<div class="label">Retries</div><div class="value" id="bt-retries">-</div>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Discoverable</div><div class="toggle-desc">Make device visible for BT pairing</div></div>
<label class="switch"><input type="checkbox" id="bt-visible" onchange="toggleBtVisible(this.checked)"><span class="slider"></span></label>
</div>
<div style="margin-top:10px;padding-top:10px;border-top:1px solid #0f3460">
<div style="font-size:12px;color:#888;margin-bottom:8px">Phone Pairing</div>
<div class="action-btns" style="margin-bottom:8px">
<button class="action-btn btn-restart" id="bt-scan-btn" onclick="btScan()">Scan for Devices</button>
</div>
<div id="bt-scan-results"></div>
</div>
</div>

<!-- 13. WPA-SEC Upload -->
<div class="card" id="card-wpasec">
<div class="card-title">WPA-SEC Upload</div>
<div class="sub">Upload captured handshakes to wpa-sec.stanev.org for cloud cracking.</div>
<div class="status-grid" style="margin-bottom:8px">
<div class="label">Status</div><div class="value" id="wpasec-status">-</div>
<div class="label">API Key</div><div class="value" id="wpasec-key">-</div>
</div>
<div style="display:flex;gap:6px;align-items:center;flex-wrap:wrap;margin-top:8px">
<input type="text" id="wpasec-input" class="wl-input" placeholder="WPA-SEC API key" style="flex:2;min-width:180px">
<button class="wl-btn wl-btn-add" onclick="saveWpaSec()">Save</button>
</div>
</div>

<!-- 14. Discord Webhook -->
<div class="card" id="card-discord">
<div class="card-title">Discord Notifications</div>
<div class="sub">Send handshake capture notifications to a Discord channel.</div>
<div class="toggle-row" style="border-bottom:none;padding-bottom:0">
<div class="toggle-info"><div class="toggle-label">Enabled</div><div class="toggle-desc">Toggle Discord notifications on/off</div></div>
<label class="switch"><input type="checkbox" id="discord-toggle" onchange="saveDiscord()"><span class="slider"></span></label>
</div>
<div style="display:flex;gap:6px;align-items:center;flex-wrap:wrap;margin-top:8px">
<input type="text" id="discord-url" class="wl-input" placeholder="Discord webhook URL" style="flex:2;min-width:180px">
<button class="wl-btn wl-btn-add" onclick="saveDiscord()">Save</button>
</div>
<div class="status-grid" style="margin-top:8px">
<div class="label">Status</div><div class="value" id="discord-status">Disabled</div>
</div>
</div>

<!-- ═══════ STATUS & PERSONALITY ═══════ -->
<div class="section-label">Status</div>

<!-- 15. Recovery status -->
<div class="card" id="card-recovery">
<div class="card-title">Recovery Status</div>
<div class="sub">WiFi and firmware crash recovery tracking.</div>
<div class="health-row" style="margin-bottom:8px">
<div class="health-item"><span class="dot dot-gray" id="h-wifi"></span>WiFi</div>
<div class="health-item"><span class="dot dot-gray" id="h-ao"></span>AO</div>
<div class="health-item"><span class="dot dot-gray" id="h-recovery"></span>Recovery</div>
<div class="health-item"><span class="dot dot-gray" id="h-gps"></span>GPS</div>
</div>
<div class="status-grid">
<div class="label">State</div><div class="value" id="rec-state">-</div>
<div class="label">Crashes</div><div class="value" id="rec-crashes">-</div>
<div class="label">Recoveries</div><div class="value" id="rec-total">-</div>
<div class="label">Last Recovery</div><div class="value" id="rec-last">-</div>
<div class="label">AO PID</div><div class="value" id="rec-pid">-</div>
<div class="label">AO Uptime</div><div class="value" id="rec-ao-up">-</div>
</div>
</div>

<!-- 16. Personality -->
<div class="card" id="card-personality">
<div class="card-title">Personality</div>
<div class="sub">Mood, experience, and level progression.</div>
<div class="status-grid">
<div class="label">Mood</div><div class="value" id="p-mood">-</div>
<div class="label">Face</div><div class="value" id="p-face">-</div>
<div class="label">XP</div><div class="value" id="p-xp">-</div>
<div class="label">Level</div><div class="value" id="p-level">-</div>
<div class="label">Blind Epochs</div><div class="value" id="p-blind">-</div>
</div>
<div class="progress-bar" style="margin-top:8px"><div class="progress-fill" id="mood-bar" style="width:50%"></div></div>
</div>

<!-- ═══════ MANAGEMENT ═══════ -->
<div class="section-label">Management</div>

<!-- 17. Actions -->
<div class="card" id="card-actions">
<div class="card-title">Actions</div>
<div class="sub">Restart applies config changes. Shutdown powers off the Pi.</div>
<div class="action-btns">
<button class="action-btn btn-restart" onclick="restartAO()">Restart AO</button>
<button class="action-btn btn-stop" onclick="if(confirm('Shut down the Pi?'))doShutdown()">Shutdown Pi</button>
<button class="action-btn btn-warn" onclick="if(confirm('Restart oxigotchi?'))restartPwn()">Restart Oxi</button>
<button class="action-btn btn-restart" onclick="if(confirm('Reboot the Pi?'))restartPi()">Restart Pi</button>
<button class="action-btn btn-restart" onclick="restartSSH()">Restart SSH</button>
</div>
</div>

<!-- 18. Plugins -->
<div class="card" id="card-plugins">
<div class="card-title">Plugins</div>
<div class="sub">Lua plugins control display indicators. Toggle on/off and set x,y positions.</div>
<div id="plugins-list"><div style="color:#555;font-size:12px">Loading...</div></div>
</div>

<!-- 19. Whitelist -->
<div class="card" id="card-whitelist">
<div class="card-title">Whitelist</div>
<div class="sub">Networks and MACs excluded from attacks. Changes apply next epoch.</div>
<div id="wl-list"><div style="color:#555;font-size:12px">Loading...</div></div>
<div style="margin-top:10px;padding-top:10px;border-top:1px solid #0f3460;display:flex;gap:6px;align-items:center;flex-wrap:wrap">
<input type="text" id="wl-value" class="wl-input" placeholder="MAC or SSID" style="flex:2;min-width:120px">
<select id="wl-type" class="wl-select" style="flex:0 0 80px"><option value="MAC">MAC</option><option value="SSID">SSID</option></select>
<button class="wl-btn wl-btn-add" onclick="addWhitelist()">Add</button>
</div>
</div>

<!-- 20. Logs Panel -->
<div class="card" id="card-logs">
<div class="card-title" style="display:flex;justify-content:space-between;align-items:center">
<span>Logs</span>
<button class="collapse-btn" id="logs-toggle" onclick="toggleLogs()">Show</button>
</div>
<div id="logs-panel" style="display:none">
<pre class="logs-pre" id="logs-content">Loading...</pre>
</div>
</div>

<!-- 21. Settings -->
<div class="card" id="card-settings">
<div class="card-title">Settings</div>
<div class="sub">Device configuration. Changes are saved to config.toml.</div>
<div style="margin-bottom:10px">
<div style="font-size:12px;color:#888;margin-bottom:4px">Device Name</div>
<div style="display:flex;gap:6px">
<input type="text" id="setting-name" class="wl-input" placeholder="oxigotchi" style="flex:2">
<button class="wl-btn wl-btn-add" onclick="saveSettings()">Save</button>
</div>
</div>
</div>

<div style="text-align:center;color:#555;font-size:10px;margin-top:8px">Auto-refreshes every 5s &bull; Rusty Oxigotchi</div>

<div class="toast" id="toast"></div>

<script>
function api(method, path, body) {
    var opts = {method: method, headers: {'Content-Type':'application/json'}};
    if (body) opts.body = JSON.stringify(body);
    return fetch(path, opts).then(function(r){return r.json()}).catch(function(e){console.error('API:',path,e)});
}
function toast(msg) {
    var t = document.getElementById('toast');
    t.textContent = msg;
    t.classList.add('show');
    setTimeout(function(){t.classList.remove('show')}, 1500);
}
function fmtUptime(secs) {
    if (!secs && secs !== 0) return '--';
    var h = Math.floor(secs/3600), m = Math.floor((secs%3600)/60), s = secs%60;
    return String(h).padStart(2,'0')+':'+String(m).padStart(2,'0')+':'+String(s).padStart(2,'0');
}
function fmtBytes(b) {
    if (b < 1024) return b + ' B';
    if (b < 1048576) return (b/1024).toFixed(1) + ' KB';
    return (b/1048576).toFixed(1) + ' MB';
}
function esc(s) { var d = document.createElement('div'); d.textContent = s; return d.innerHTML; }

// --- Refresh functions ---

function refreshStatus() {
    api('GET', '/api/status').then(function(d) {
        if (!d) return;
        document.getElementById('s-ch').textContent = d.channel;
        document.getElementById('s-aps').textContent = d.aps_seen;
        document.getElementById('s-pwnd').textContent = d.handshakes;
        document.getElementById('s-epoch').textContent = d.epoch;
        document.getElementById('s-uptime').textContent = d.uptime;
        // Mode buttons
        document.getElementById('mode-rage').classList.toggle('active', d.mode === 'RAGE' || d.mode === 'AO');
        document.getElementById('mode-safe').classList.toggle('active', d.mode === 'SAFE' || d.mode === 'PWN');
        // Settings name field (only if not focused)
        var nameInput = document.getElementById('setting-name');
        if (nameInput && !nameInput.matches(':focus')) nameInput.value = d.name || '';
    });
}

function refreshBattery() {
    api('GET', '/api/battery').then(function(d) {
        if (!d) return;
        if (d.available) {
            document.getElementById('bat-level').textContent = d.level + '%';
            document.getElementById('bat-level').style.color = d.critical ? '#e94560' : (d.low ? '#f0c040' : '#00d4aa');
            document.getElementById('bat-state').textContent = d.charging ? 'Charging' : 'Discharging';
            document.getElementById('bat-voltage').textContent = (d.voltage_mv / 1000).toFixed(2) + 'V';
            document.getElementById('bat-bar').style.width = d.level + '%';
            document.getElementById('bat-bar').style.background = d.critical ? '#e94560' : (d.low ? '#f0c040' : '#00d4aa');
        } else {
            document.getElementById('bat-level').textContent = 'N/A';
            document.getElementById('bat-state').textContent = 'Not detected';
            document.getElementById('bat-voltage').textContent = '-';
        }
    });
}

function refreshBluetooth() {
    api('GET', '/api/bluetooth').then(function(d) {
        if (!d) return;
        document.getElementById('bt-status').textContent = d.connected ? 'Connected' : d.state;
        document.getElementById('bt-status').style.color = d.connected ? '#00d4aa' : '#888';
        document.getElementById('bt-device').textContent = d.device_name || '-';
        document.getElementById('bt-ip').textContent = d.ip || '-';
        document.getElementById('bt-internet').textContent = d.internet_available ? 'Yes' : 'No';
        document.getElementById('bt-internet').style.color = d.internet_available ? '#00d4aa' : '#888';
        document.getElementById('bt-retries').textContent = d.retry_count;
    });
}

function refreshWifi() {
    api('GET', '/api/wifi').then(function(d) {
        if (!d) return;
        document.getElementById('wifi-state').textContent = d.state;
        document.getElementById('wifi-state').style.color = d.state === 'Monitor' ? '#00d4aa' : '#e94560';
        document.getElementById('wifi-ch').textContent = d.channel;
        document.getElementById('wifi-aps').textContent = d.aps_tracked;
        document.getElementById('wifi-channels').textContent = d.channels.join(', ') || '-';
        document.getElementById('wifi-dwell').textContent = d.dwell_ms + 'ms';
        // Populate channel config card with current values
        var chInput = document.getElementById('ch-list');
        if (chInput && !chInput.matches(':focus')) chInput.value = d.channels.join(',');
        var dwInput = document.getElementById('ch-dwell');
        if (dwInput && !dwInput.matches(':active')) { dwInput.value = d.dwell_ms; document.getElementById('ch-dwell-val').textContent = d.dwell_ms; }
    });
}

function refreshAttacks() {
    api('GET', '/api/attacks').then(function(d) {
        if (!d) return;
        document.getElementById('s-rate').textContent = d.attack_rate;
        ['deauth','pmkid','csa','disassoc','anon_reassoc','rogue_m2'].forEach(function(k) {
            var cb = document.getElementById('atk-'+k);
            if (cb) cb.checked = d[k];
        });
        [1,2,3].forEach(function(n) {
            document.getElementById('rate-'+n).classList.toggle('active', n === d.attack_rate);
        });
    });
}

function refreshCaptures() {
    api('GET', '/api/captures').then(function(d) {
        if (!d) return;
        document.getElementById('cap-total').textContent = d.total_files;
        document.getElementById('cap-hs').textContent = d.handshake_files;
        document.getElementById('cap-pending').textContent = d.pending_upload;
        document.getElementById('cap-size').textContent = fmtBytes(d.total_size_bytes);
        var el = document.getElementById('cap-list');
        if (!d.files || !d.files.length) {
            el.innerHTML = '<div style="color:#555;font-size:12px">No captures yet</div>';
            return;
        }
        el.innerHTML = d.files.map(function(f) {
            return '<div class="capture-item"><a href="/api/download/' + encodeURIComponent(f.filename) + '" style="color:#00d4aa;text-decoration:none">' + esc(f.filename) + '</a> <span style="color:#555">(' + fmtBytes(f.size_bytes) + ')</span></div>';
        }).join('');
    });
}

function refreshRecovery() {
    api('GET', '/api/recovery').then(function(d) {
        if (!d) return;
        document.getElementById('rec-state').textContent = d.state;
        document.getElementById('rec-state').style.color = d.state === 'Healthy' ? '#00d4aa' : '#f0c040';
        document.getElementById('rec-total').textContent = d.total_recoveries;
        document.getElementById('rec-last').textContent = d.last_recovery;
    });
    api('GET', '/api/health').then(function(d) {
        if (!d) return;
        document.getElementById('rec-crashes').textContent = d.ao_crash_count;
        document.getElementById('rec-crashes').style.color = d.ao_crash_count > 0 ? '#f0c040' : '#e0e0e0';
        document.getElementById('rec-pid').textContent = d.ao_pid || '-';
        document.getElementById('rec-ao-up').textContent = d.ao_uptime;
        // Health dots
        var wdot = document.getElementById('h-wifi');
        wdot.className = 'dot ' + (d.wifi_state === 'Monitor' ? 'dot-green' : 'dot-red');
        var adot = document.getElementById('h-ao');
        adot.className = 'dot ' + (d.ao_state === 'RUNNING' ? 'dot-green' : 'dot-red');
        var rdot = document.getElementById('h-recovery');
        rdot.className = 'dot ' + (d.ao_crash_count === 0 ? 'dot-green' : 'dot-yellow');
        var gdot = document.getElementById('h-gps');
        gdot.className = 'dot ' + (d.gpsd_available ? 'dot-green' : 'dot-gray');
        var gpsEl = document.getElementById('sys-gps');
        gpsEl.textContent = d.gpsd_available ? 'Connected' : 'N/A';
        gpsEl.style.color = d.gpsd_available ? '#00d4aa' : '#888';
        document.getElementById('sys-uptime').textContent = fmtUptime(d.uptime_secs);
    });
}

function refreshPersonality() {
    api('GET', '/api/personality').then(function(d) {
        if (!d) return;
        document.getElementById('p-mood').textContent = Math.round(d.mood * 100) + '%';
        document.getElementById('p-face').textContent = d.face;
        document.getElementById('p-xp').textContent = d.xp;
        document.getElementById('p-level').textContent = d.level;
        document.getElementById('p-blind').textContent = d.blind_epochs;
        document.getElementById('mood-bar').style.width = Math.round(d.mood * 100) + '%';
        var moodColor = d.mood > 0.7 ? '#00d4aa' : (d.mood > 0.3 ? '#f0c040' : '#e94560');
        document.getElementById('mood-bar').style.background = moodColor;
    });
}

function refreshSystem() {
    api('GET', '/api/system').then(function(d) {
        if (!d) return;
        document.getElementById('sys-temp').textContent = d.cpu_temp_c > 0 ? d.cpu_temp_c.toFixed(1) + '\u00B0C' : '-';
        document.getElementById('sys-temp').style.color = d.cpu_temp_c > 70 ? '#e94560' : (d.cpu_temp_c > 55 ? '#f0c040' : '#00d4aa');
        document.getElementById('sys-cpu').textContent = d.cpu_percent > 0 ? d.cpu_percent.toFixed(0) + '%' : '-';
        document.getElementById('sys-mem').textContent = d.mem_total_mb > 0 ? d.mem_used_mb + '/' + d.mem_total_mb + ' MB' : '-';
        document.getElementById('sys-disk').textContent = d.disk_total_mb > 0 ? d.disk_used_mb + '/' + d.disk_total_mb + ' MB' : '-';
    });
}

function refreshCracked() {
    api('GET', '/api/cracked').then(function(list) {
        var el = document.getElementById('cracked-list');
        if (!list || !list.length) {
            el.innerHTML = '<div style="color:#555;font-size:12px">No cracked passwords yet</div>';
            return;
        }
        el.innerHTML = list.map(function(c) {
            return '<div style="padding:4px 0;border-bottom:1px solid #0f346022">' +
                '<span style="color:#00d4aa;font-weight:bold">' + esc(c.ssid || c.bssid) + '</span>' +
                (c.bssid ? ' <span style="color:#666;font-size:10px">[' + esc(c.bssid) + ']</span>' : '') +
                '<br><span style="color:#f0c040;font-family:monospace;font-size:12px">' + esc(c.password) + '</span></div>';
        }).join('');
    });
}

function refreshAps() {
    api('GET', '/api/aps').then(function(aps) {
        var el = document.getElementById('ap-tbody');
        if (!aps || !aps.length) {
            el.innerHTML = '<tr><td colspan="6" style="color:#555">No APs detected</td></tr>';
            return;
        }
        // Sort by RSSI descending (strongest first)
        aps.sort(function(a,b){ return b.rssi - a.rssi; });
        el.innerHTML = aps.map(function(ap) {
            var rssiColor = ap.rssi > -50 ? '#00d4aa' : (ap.rssi > -70 ? '#f0c040' : '#e94560');
            var hsIcon = ap.has_handshake ? '<span style="color:#00d4aa" title="Handshake captured">&#9734;</span>' : '';
            return '<tr><td>' + esc(ap.ssid || '<hidden>') + '</td>' +
                '<td style="color:#888;font-size:10px">' + esc(ap.bssid) + '</td>' +
                '<td style="color:' + rssiColor + '">' + ap.rssi + '</td>' +
                '<td>' + ap.channel + '</td>' +
                '<td>' + ap.clients + '</td>' +
                '<td>' + hsIcon + '</td></tr>';
        }).join('');
    });
}

function refreshWhitelist() {
    api('GET', '/api/whitelist').then(function(entries) {
        var el = document.getElementById('wl-list');
        if (!entries || !entries.length) {
            el.innerHTML = '<div style="color:#555;font-size:12px">No whitelist entries</div>';
            return;
        }
        var html = '<table class="ap-table"><thead><tr><th>Value</th><th>Type</th><th></th></tr></thead><tbody>';
        entries.forEach(function(e) {
            html += '<tr><td>' + esc(e.value) + '</td><td>' + esc(e.entry_type) + '</td>' +
                '<td><button class="wl-btn-rm" onclick="removeWhitelist(\'' + esc(e.value) + '\')">Remove</button></td></tr>';
        });
        html += '</tbody></table>';
        el.innerHTML = html;
    });
}

function refreshLogs() {
    var panel = document.getElementById('logs-panel');
    if (panel.style.display === 'none') return;
    api('GET', '/api/logs').then(function(d) {
        if (!d) return;
        var el = document.getElementById('logs-content');
        el.textContent = d.lines.join('\n') || 'No logs available';
        el.scrollTop = el.scrollHeight;
    });
}

function refreshPlugins() {
    api('GET', '/api/plugins').then(function(plugins) {
        if (!plugins) return;
        var html = '';
        plugins.forEach(function(p) {
            var tagColor = p.tag === 'default' ? '#00d4aa' : '#f0c040';
            html += '<div class="toggle-row">' +
                '<div class="toggle-info">' +
                '<div class="toggle-label">' + esc(p.name) +
                ' <span style="color:' + tagColor + ';font-size:10px;padding:1px 6px;border:1px solid ' + tagColor + ';border-radius:8px;margin-left:6px">' + esc(p.tag) + '</span>' +
                ' <span style="color:#666;font-size:10px;margin-left:4px">v' + esc(p.version) + '</span></div>' +
                '<div class="toggle-desc" style="margin-top:4px">' +
                'x: <input type="number" min="0" max="249" value="' + p.x + '" style="width:48px;background:#0a1628;color:#e0e0e0;border:1px solid #0f3460;border-radius:4px;padding:2px 4px;font-size:11px" onchange="updatePlugin(\'' + esc(p.name) + '\',this.parentNode)">' +
                ' y: <input type="number" min="0" max="121" value="' + p.y + '" style="width:48px;background:#0a1628;color:#e0e0e0;border:1px solid #0f3460;border-radius:4px;padding:2px 4px;font-size:11px" onchange="updatePlugin(\'' + esc(p.name) + '\',this.parentNode)">' +
                '</div></div>' +
                '<label class="switch"><input type="checkbox" ' + (p.enabled ? 'checked' : '') + ' onchange="togglePlugin(\'' + esc(p.name) + '\',this.checked)"><span class="slider"></span></label>' +
                '</div>';
        });
        document.getElementById('plugins-list').innerHTML = html || '<div style="color:#555;font-size:12px">No plugins loaded</div>';
    });
}

function refreshWpaSec() {
    api('GET', '/api/wpasec').then(function(d) {
        if (!d) return;
        document.getElementById('wpasec-status').textContent = d.enabled ? 'Enabled' : 'Disabled';
        document.getElementById('wpasec-status').style.color = d.enabled ? '#00d4aa' : '#888';
        document.getElementById('wpasec-key').textContent = d.api_key || '(not set)';
    });
}

function refreshDiscord() {
    api('GET', '/api/discord').then(function(d) {
        if (!d) return;
        document.getElementById('discord-status').textContent = d.enabled ? 'Enabled' : 'Disabled';
        document.getElementById('discord-status').style.color = d.enabled ? '#00d4aa' : '#888';
        document.getElementById('discord-toggle').checked = d.enabled;
    });
}

// --- Action functions ---

function addWhitelist() {
    var val = document.getElementById('wl-value').value.trim();
    var typ = document.getElementById('wl-type').value;
    if (!val) { toast('Enter a value'); return; }
    api('POST', '/api/whitelist/add', {value: val, entry_type: typ}).then(function(r) {
        if (r && r.ok) { toast('Added to whitelist'); document.getElementById('wl-value').value = ''; refreshWhitelist(); }
    });
}

function removeWhitelist(val) {
    api('POST', '/api/whitelist/remove', {value: val}).then(function(r) {
        if (r && r.ok) { toast('Removed from whitelist'); refreshWhitelist(); }
    });
}

function applyChannels() {
    var chStr = document.getElementById('ch-list').value.trim();
    var dwell = parseInt(document.getElementById('ch-dwell').value) || 2000;
    var autohunt = document.getElementById('autohunt-toggle').checked;
    var channels = null;
    if (chStr) {
        channels = chStr.split(',').map(function(c){ return parseInt(c.trim()); }).filter(function(c){ return !isNaN(c) && c > 0 && c <= 14; });
        if (!channels.length) { toast('Invalid channel list'); return; }
    }
    api('POST', '/api/channels', {channels: channels, dwell_ms: dwell, autohunt: autohunt}).then(function(r) {
        if (r && r.ok) toast('Channel config applied');
    });
}

function toggleAutohunt(enabled) {
    var chStr = document.getElementById('ch-list').value.trim();
    var dwell = parseInt(document.getElementById('ch-dwell').value) || 2000;
    var channels = null;
    if (chStr) {
        channels = chStr.split(',').map(function(c){ return parseInt(c.trim()); }).filter(function(c){ return !isNaN(c) && c > 0 && c <= 14; });
    }
    api('POST', '/api/channels', {channels: channels, dwell_ms: dwell, autohunt: enabled}).then(function(r) {
        if (r && r.ok) toast('Autohunt ' + (enabled ? 'ON' : 'OFF'));
    });
}

function toggleLogs() {
    var panel = document.getElementById('logs-panel');
    var btn = document.getElementById('logs-toggle');
    if (panel.style.display === 'none') {
        panel.style.display = 'block';
        btn.textContent = 'Hide';
        refreshLogs();
    } else {
        panel.style.display = 'none';
        btn.textContent = 'Show';
    }
}

function toggleAttack(name, val) {
    var data = {};
    data[name] = val;
    api('POST', '/api/attacks', data).then(function() {
        toast('Attack ' + name + (val ? ' ON' : ' OFF'));
    });
}
function setRate(r) {
    api('POST', '/api/rate', {rate: r}).then(function() {
        [1,2,3].forEach(function(n) {
            document.getElementById('rate-'+n).classList.toggle('active', n === r);
        });
        toast('Rate set to ' + r);
    });
}
function switchMode(mode) {
    toast('Switching to ' + mode + '...');
    api('POST', '/api/mode', {mode: mode}).then(function(r) {
        if (r && r.ok) toast(r.message);
    });
}
function restartAO() {
    api('POST', '/api/restart', {}).then(function(r) {
        toast(r && r.message ? r.message : 'Restart queued');
    });
}
function doShutdown() {
    api('POST', '/api/shutdown', {}).then(function(r) {
        toast(r && r.message ? r.message : 'Shutdown queued');
    });
}
function restartPwn() {
    api('POST', '/api/restart-pwn', {}).then(function(r) {
        toast(r && r.message ? r.message : 'Oxigotchi restart queued');
    });
}
function restartPi() {
    api('POST', '/api/restart-pi', {}).then(function(r) {
        toast(r && r.message ? r.message : 'Pi reboot initiated');
    });
}
function restartSSH() {
    api('POST', '/api/restart-ssh', {}).then(function(r) {
        toast(r && r.message ? r.message : 'SSH restart initiated');
    });
}

function toggleBtVisible(visible) {
    api('POST', '/api/bluetooth', {visible: visible}).then(function(r) {
        toast('Bluetooth ' + (visible ? 'discoverable' : 'hidden'));
    });
}

function btScan() {
    var btn = document.getElementById('bt-scan-btn');
    btn.textContent = 'Scanning...';
    btn.disabled = true;
    document.getElementById('bt-scan-results').innerHTML = '<div style="color:#888;font-size:12px">Scanning for nearby devices (~10s)...</div>';
    api('POST', '/api/bluetooth/scan', {}).then(function() {
        // Poll for results every 2s
        var poll = setInterval(function() {
            api('GET', '/api/bluetooth/scan').then(function(devices) {
                if (!devices) return;
                if (devices.length > 0) {
                    clearInterval(poll);
                    btn.textContent = 'Scan for Devices';
                    btn.disabled = false;
                    var html = '<div style="font-size:11px;color:#888;margin-bottom:4px">Found ' + devices.length + ' device(s). Tap to pair:</div>';
                    devices.forEach(function(d) {
                        html += '<div style="display:flex;justify-content:space-between;align-items:center;padding:6px 0;border-bottom:1px solid #0f3460">' +
                            '<div><div style="font-size:13px;font-weight:bold">' + esc(d.name) + '</div>' +
                            '<div style="font-size:10px;color:#888">' + esc(d.mac) + '</div></div>' +
                            '<button class="wl-btn wl-btn-add" style="padding:6px 12px" onclick="btPair(\'' + esc(d.mac) + '\')">Pair</button></div>';
                    });
                    document.getElementById('bt-scan-results').innerHTML = html;
                }
            });
        }, 2000);
        // Stop polling after 20s — scan done or no devices found
        setTimeout(function() {
            clearInterval(poll);
            btn.textContent = 'Scan for Devices';
            btn.disabled = false;
            if (document.getElementById('bt-scan-results').innerHTML.indexOf('Scanning') !== -1) {
                document.getElementById('bt-scan-results').innerHTML = '<div style="color:#888;font-size:12px">No devices found. Make sure your phone\'s Bluetooth is on.</div>';
            }
        }, 20000);
    });
}

function btPair(mac) {
    toast('Pairing with ' + mac + '...');
    api('POST', '/api/bluetooth/pair', {mac: mac}).then(function(r) {
        if (r && r.ok) {
            toast(r.message);
            document.getElementById('bt-scan-results').innerHTML = '<div style="color:#00d4aa;font-size:12px">Pairing in progress...</div>';
        }
    });
}

function saveWpaSec() {
    var key = document.getElementById('wpasec-input').value.trim();
    api('POST', '/api/wpasec', {api_key: key}).then(function(r) {
        if (r && r.ok) { toast('WPA-SEC key saved'); document.getElementById('wpasec-input').value = ''; refreshWpaSec(); }
    });
}

function saveDiscord() {
    var url = document.getElementById('discord-url').value.trim();
    var enabled = document.getElementById('discord-toggle').checked;
    api('POST', '/api/discord', {webhook_url: url, enabled: enabled}).then(function(r) {
        if (r && r.ok) { toast('Discord config saved'); refreshDiscord(); }
    });
}

function togglePlugin(name, enabled) {
    api('POST', '/api/plugins', [{name: name, enabled: enabled}])
        .then(function(r) { toast('Plugin ' + name + (enabled ? ' ON' : ' OFF')); });
}

function updatePlugin(name, container) {
    var inputs = container.querySelectorAll('input[type=number]');
    var x = parseInt(inputs[0].value) || 0;
    var y = parseInt(inputs[1].value) || 0;
    api('POST', '/api/plugins', [{name: name, x: x, y: y}])
        .then(function(r) { toast(name + ' position: ' + x + ',' + y); });
}

function saveSettings() {
    var name = document.getElementById('setting-name').value.trim();
    if (!name) { toast('Enter a name'); return; }
    api('POST', '/api/settings', {name: name}).then(function(r) {
        if (r && r.ok) toast('Settings saved');
    });
}

// --- Initial load & auto-refresh ---
refreshStatus();
setTimeout(refreshBattery, 500);
setTimeout(refreshBluetooth, 1000);
setTimeout(refreshWifi, 1500);
setTimeout(refreshAttacks, 2000);
setTimeout(refreshCaptures, 2500);
setTimeout(refreshRecovery, 3000);
setTimeout(refreshPersonality, 3500);
setTimeout(refreshSystem, 4000);
setTimeout(refreshCracked, 4500);
setTimeout(refreshPlugins, 5000);
setTimeout(refreshAps, 5500);
setTimeout(refreshWhitelist, 6000);
setTimeout(refreshWpaSec, 6500);
setTimeout(refreshDiscord, 7000);

setInterval(refreshStatus, 5000);
setInterval(refreshBattery, 15000);
setInterval(refreshBluetooth, 15000);
setInterval(refreshWifi, 5000);
setInterval(refreshAttacks, 10000);
setInterval(refreshCaptures, 30000);
setInterval(refreshRecovery, 15000);
setInterval(refreshPersonality, 10000);
setInterval(refreshSystem, 15000);
setInterval(refreshCracked, 60000);
setInterval(refreshPlugins, 15000);
setInterval(refreshAps, 10000);
setInterval(refreshWhitelist, 30000);
setInterval(refreshLogs, 10000);
setInterval(refreshWpaSec, 30000);
setInterval(refreshDiscord, 30000);
setInterval(function(){ document.getElementById('eink-img').src='/api/display.png?t='+Date.now(); }, 5000);
</script>
</body>
</html>"##;
