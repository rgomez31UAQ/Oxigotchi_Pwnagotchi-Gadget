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
.rage-card{margin-bottom:12px}
.rage-row{display:flex;align-items:center;gap:12px;margin-top:8px}
.rage-slider{flex:1;-webkit-appearance:none;appearance:none;height:8px;border-radius:4px;background:linear-gradient(90deg,#0f3460,#e67e22,#e74c3c);outline:none;opacity:0.9}
.rage-slider::-webkit-slider-thumb{-webkit-appearance:none;appearance:none;width:24px;height:24px;border-radius:50%;background:#00d4aa;cursor:pointer;border:2px solid #0a1628}
.rage-slider:disabled{opacity:0.3;cursor:not-allowed}
.rage-slider:disabled::-webkit-slider-thumb{background:#555}
.rage-slider::-moz-range-thumb{width:24px;height:24px;border-radius:50%;background:#00d4aa;cursor:pointer;border:2px solid #0a1628}
.rage-slider:disabled::-moz-range-thumb{background:#555}
.rage-slider::-moz-range-track{height:8px;border-radius:4px;background:linear-gradient(90deg,#0f3460,#e67e22,#e74c3c)}
.rage-level{font-size:20px;font-weight:bold;color:#00d4aa;min-width:80px;text-align:center}
.rage-level.yolo{color:#e74c3c}
.rage-disclaimer{color:#e67e22;font-size:11px;padding:6px 8px;background:#5a300033;border-radius:6px;margin-top:8px;display:none}
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
.ap-scroll{max-height:300px;overflow:auto;margin-top:4px}
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
.bt-rage-btn{flex:1;padding:12px 0;border:2px solid #0f3460;border-radius:10px;background:transparent;color:#e0e0e0;font-size:14px;font-weight:bold;font-family:inherit;cursor:pointer;text-align:center;transition:.2s}
.bt-rage-btn.active{background:#0f3460;color:#00d4aa;border-color:#00d4aa}
.bt-rage-btn:active{transform:scale(0.95)}
.bt-action-btn{background:#1a1a2e;color:#00d4aa;border:1px solid #00d4aa;padding:2px 8px;border-radius:4px;cursor:pointer;font-size:11px;margin:1px}
.bt-action-btn:hover:not(:disabled){background:#00d4aa;color:#0f0f23}
.bt-action-btn:disabled{opacity:0.3;cursor:not-allowed}
.bt-badge{font-size:9px;padding:1px 5px;border-radius:8px;margin-left:6px;vertical-align:middle}
.bt-badge-pr{color:#f0c040;border:1px solid #f0c040}
.bt-badge-auto{color:#00d4aa;border:1px solid #00d4aa}
.bt-row-disabled{opacity:0.4;pointer-events:none}
.bt-row-warning{border-left:2px solid #f0c040}
.bt-dev-list{display:flex;flex-direction:column;gap:6px;max-height:400px;overflow-y:auto;padding:2px}
.bt-dev{display:flex;align-items:center;gap:10px;padding:10px 12px;background:#0a1628;border-radius:10px;border:1px solid #0f346044;transition:border-color .2s}
.bt-dev:hover{border-color:#0f3460}
.bt-dev-signal{display:flex;align-items:flex-end;gap:2px;min-width:20px;height:18px}
.bt-dev-signal .bar{width:3px;border-radius:1px;background:#333;transition:background .3s}
.bt-dev-signal.sig-strong .bar{background:#00d4aa}
.bt-dev-signal.sig-medium .bar:nth-child(-n+3){background:#f0c040}
.bt-dev-signal.sig-weak .bar:nth-child(-n+2){background:#e94560}
.bt-dev-signal.sig-dead .bar:first-child{background:#e94560}
.bt-dev-info{flex:1;min-width:0}
.bt-dev-name{font-size:13px;font-weight:600;color:#e0e0e0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.bt-dev-name.unnamed{color:#555;font-weight:400}
.bt-dev-meta{font-size:10px;color:#666;margin-top:2px;display:flex;gap:6px;align-items:center;flex-wrap:wrap}
.bt-dev-vendor{color:#888}
.bt-dev-addr{font-family:'SF Mono','Fira Code',monospace;letter-spacing:0.3px}
.bt-dev-transport{font-size:9px;padding:1px 5px;border-radius:4px;font-weight:600;letter-spacing:0.5px}
.bt-dev-transport.ble{color:#00d4aa;background:#00d4aa15;border:1px solid #00d4aa33}
.bt-dev-transport.classic{color:#5dade2;background:#5dade215;border:1px solid #5dade233}
.bt-dev-atk-detail{color:#e9456099;font-size:9px;font-style:italic}
.bt-dev-rssi{font-size:10px;color:#555;min-width:42px;text-align:right}
.bt-dev-state{font-size:10px;padding:2px 8px;border-radius:10px;font-weight:600;white-space:nowrap}
.bt-dev-state.st-untouched{color:#555;background:#55555515}
.bt-dev-state.st-attacking{color:#e67e22;background:#e67e2220;border:1px solid #e67e2233;animation:pulse-attack 1.5s infinite}
.bt-dev-state.st-targeted{color:#f0c040;background:#f0c04015;border:1px solid #f0c04033}
.bt-dev-state.st-captured{color:#00d4aa;background:#00d4aa15;border:1px solid #00d4aa33}
.bt-dev-state.st-failed{color:#e94560;background:#e9456015;border:1px solid #e9456033}
@keyframes pulse-attack{0%,100%{opacity:1}50%{opacity:0.6}}
.bt-dev-actions{display:flex;gap:4px;flex-shrink:0}
.bt-dev-empty{text-align:center;padding:24px 12px;color:#444;font-size:13px}
.bt-dev-count{font-size:11px;color:#555;text-align:right;padding:4px 0}
.interact-btn{padding:8px 16px;border:1px solid #0f3460;border-radius:8px;background:#0a1628;color:#e0e0e0;font-family:inherit;font-size:12px;font-weight:600;cursor:pointer;transition:all .2s;min-width:72px}
.interact-btn:hover:not(:disabled){background:#0f3460;border-color:#00d4aa;color:#00d4aa}
.interact-btn:active:not(:disabled){transform:scale(0.95)}
.interact-btn:disabled{opacity:0.35;cursor:not-allowed;color:#555}
.interact-btn.on-cooldown{border-color:#0f3460;color:#555;font-size:11px}
.interact-response{margin-top:8px;font-size:12px;color:#00d4aa;text-align:center;min-height:18px;transition:opacity .3s}
.util-btn{padding:6px 14px;border:1px solid #0f3460;border-radius:8px;background:#0a1628;color:#e0e0e0;font-family:inherit;font-size:12px;font-weight:600;cursor:pointer;transition:all .2s}
.util-btn:hover:not(:disabled){background:#0f3460;border-color:#00d4aa;color:#00d4aa}
.util-btn:active:not(:disabled){transform:scale(0.95)}
.util-btn:disabled{opacity:0.35;cursor:not-allowed}
.util-btn-danger{border-color:#5c1a1a;color:#e94560;background:#1a1a2e}
.util-btn-danger:hover:not(:disabled){background:#5c1a1a;border-color:#e94560}
.util-btn-confirm{border-color:#1a5c1a;color:#00d4aa}
.util-btn-confirm:hover:not(:disabled){background:#1a5c1a;border-color:#00d4aa}
.util-btn-reject{border-color:#5c1a1a;color:#e94560}
.util-btn-reject:hover:not(:disabled){background:#5c1a1a;border-color:#e94560}
@media(max-width:400px){.grid-2{grid-template-columns:1fr}.stat-row{gap:4px}.stat .value{font-size:15px}.bt-dev{padding:8px 10px;gap:8px}.bt-dev-rssi{display:none}}
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
<div id="interact-btns" style="margin-top:10px;display:flex;gap:8px;justify-content:center">
<button class="interact-btn" id="btn-pet" onclick="interact('pet')">Pet</button>
<button class="interact-btn" id="btn-treat" onclick="interact('treat')">Treat</button>
<button class="interact-btn" id="btn-praise" onclick="interact('praise')">Praise</button>
</div>
<div class="interact-response" id="interact-response"></div>
</div>

<!-- 2. Mode switch -->
<div class="card" id="card-mode">
<div class="card-title">Mode</div>
<div class="sub">RAGE = all attacks max aggression. BT = Bluetooth offensive. SAFE = passive scanning only.</div>
<div class="mode-btns">
<button class="mode-btn active" id="mode-rage" onclick="switchMode('RAGE')">RAGE</button>
<button class="mode-btn" id="mode-bt" onclick="switchMode('BT')">BT</button>
<button class="mode-btn" id="mode-safe" onclick="switchMode('SAFE')">SAFE</button>
</div>
<div class="rage-disclaimer" id="bt-tether-warn">&#9888; BT mode disconnects phone tethering. You will lose remote access until switching back to RAGE or SAFE.</div>
</div>

<!-- 3. Core stats -->
<div class="card" id="card-stats">
<div class="card-title">Overview</div>
<div class="stat-row">
<div class="stat"><div class="label" id="s-label-1">CH</div><div class="value" id="s-val-1">-</div></div>
<div class="stat"><div class="label" id="s-label-2">COWS</div><div class="value" id="s-val-2">-</div></div>
<div class="stat"><div class="label" id="s-label-3">PWND</div><div class="value" id="s-val-3">-</div></div>
<div class="stat"><div class="label" id="s-label-4">CHARGES</div><div class="value" id="s-val-4">-</div></div>
<div class="stat"><div class="label" id="s-label-5">UPTIME</div><div class="value" id="s-val-5">-</div></div>
<div class="stat"><div class="label" id="s-label-6">RATE</div><div class="value" id="s-val-6">-</div></div>
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
<div class="section-label" data-modes="rage bt">Hunting</div>

<!-- 6. WiFi -->
<div class="card" id="card-wifi" data-modes="rage">
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
<div class="card" id="card-aps" data-modes="rage">
<div class="card-title">Nearby Networks</div>
<div class="sub">Access points detected by monitor mode, sorted by signal strength.</div>
<div class="ap-scroll">
<table class="ap-table" id="ap-table">
<thead><tr>
<th style="cursor:help" title="Network name. (hidden) = SSID broadcast off. (AO) = seen by AngryOxide only, no beacon captured.">SSID</th>
<th style="cursor:help" title="Hardware MAC address of the access point.">BSSID</th>
<th style="cursor:help" title="Signal strength in dBm. Green &gt; -50 (strong), yellow &gt; -70 (ok), red ≤ -70 (weak). -100 = unknown.">RSSI</th>
<th style="cursor:help" title="WiFi channel number (1–13 = 2.4 GHz).">CH</th>
<th style="cursor:help" title="Associated client count. For AO-only APs this shows attack event count instead.">Cli</th>
<th style="cursor:help" title="★ = handshake or PMKID captured — hash is saved and ready to crack.">Status</th>
</tr></thead>
<tbody id="ap-tbody"><tr><td colspan="6" style="color:#555">Loading...</td></tr></tbody>
</table>
</div>
</div>

<!-- RAGE Slider -->
<div class="card rage-card" id="card-rage" data-modes="rage">
<div class="card-title">RAGE Slider</div>
<div class="sub">Aggression preset &mdash; controls rate, dwell, and channels in one slider.</div>
<div class="toggle-row" style="margin-top:8px">
<div class="toggle-info"><div class="toggle-label">RAGE Mode</div><div class="toggle-desc" id="rage-desc">Custom &mdash; individual controls active</div></div>
<label class="switch"><input type="checkbox" id="rage-toggle" onchange="toggleRage(this.checked)"><span class="slider"></span></label>
</div>
<div class="rage-row">
<input type="range" class="rage-slider" id="rage-slider" min="1" max="7" value="4" oninput="slideRage(parseInt(this.value))" disabled>
<div class="rage-level" id="rage-label">&mdash;</div>
</div>
<div class="rage-disclaimer" id="rage-yolo">&#9888; YOLO: Only combo that crashed in stress tests. AO may die &mdash; daemon auto-recovers.</div>

<div style="margin-top:12px;padding-top:10px;border-top:1px solid #0f3460">
<div style="font-size:12px;color:#888;margin-bottom:4px">Attack Rate</div>
<div class="sub">All rates stable with v6 firmware patch. Rate 3 + 500ms + all channels is the only crash combo.</div>
<div class="rate-btns">
<button class="rate-btn active" id="rate-1" onclick="setRate(1)">1<br><span style="font-size:10px;font-weight:normal;color:#888">Quiet</span></button>
<button class="rate-btn" id="rate-2" onclick="setRate(2)">2<br><span style="font-size:10px;font-weight:normal">Normal</span></button>
<button class="rate-btn risky" id="rate-3" onclick="setRate(3)">3<br><span style="font-size:10px;font-weight:normal">Aggressive</span></button>
</div>
</div>

<div style="border-top:1px solid #0f3460;padding-top:10px;margin-top:10px">
<div style="margin-bottom:8px">
<div style="font-size:12px;color:#888;margin-bottom:4px">Channels</div>
<input type="hidden" id="ch-list" value="1,6,11">
<div id="ch-btns" style="display:flex;flex-wrap:wrap;gap:4px"></div>
</div>
<div style="margin-bottom:8px">
<div style="font-size:12px;color:#888;margin-bottom:4px">Dwell Time: <span id="ch-dwell-val">2000</span>ms</div>
<input type="range" id="ch-dwell" class="ch-slider" min="500" max="10000" step="100" value="2000" oninput="document.getElementById('ch-dwell-val').textContent=this.value">
</div>
<div style="color:#27ae60;font-size:11px;padding:6px 8px;background:#1a472a33;border-radius:6px;margin-bottom:8px">All channel/dwell combos stable with v6 firmware patch. Only known crash: rate 3 + 500ms + all 13ch.</div>
<button class="wl-btn wl-btn-add" onclick="applyChannels()">Apply custom mode</button>
</div>

<div class="toggle-row" style="border-top:1px solid #0f3460;padding-top:10px;margin-top:10px">
<div class="toggle-info"><div class="toggle-label">Autohunt</div><div class="toggle-desc">Let AO automatically pick channels to hunt on</div></div>
<label class="switch"><input type="checkbox" id="autohunt-toggle" checked onchange="toggleAutohunt(this.checked)"><span class="slider"></span></label>
</div>

<div class="toggle-row" style="border-top:1px solid #0f3460;padding-top:10px;margin-top:10px">
<div class="toggle-info"><div class="toggle-label">Smart Skip</div><div class="toggle-desc">Skip APs that already have captured handshakes</div></div>
<label class="switch"><input type="checkbox" id="skip-captured-toggle" checked onchange="toggleSkipCaptured(this.checked)"><span class="slider"></span></label>
</div>
</div>

<!-- 8. Attack controls -->
<div class="card" id="card-attacks" data-modes="rage">
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
</div>

<!-- RF Classification -->
<div class="card" id="card-rf" data-modes="rage">
<div class="card-title">RF Environment</div>
<div class="sub">Real-time 802.11 frame classification — 26&times; faster than bettercap.</div>
<div class="stat-row" style="margin-bottom:10px">
<div class="stat"><div class="value" id="rf-speed">-</div><div class="label">frames/ms</div></div>
<div class="stat"><div class="value" id="rf-total">-</div><div class="label">classified</div></div>
<div class="stat"><div class="value" id="rf-bssids">-</div><div class="label">BSSIDs</div></div>
<div class="stat"><div class="value" id="rf-dominant">-</div><div class="label">dominant</div></div>
</div>
<div class="status-grid">
<div class="label">Beacons/s</div><div class="value" id="rf-beacon">-</div>
<div class="label">Probes/s</div><div class="value" id="rf-probe">-</div>
<div class="label">Deauths/s</div><div class="value" id="rf-deauth">-</div>
<div class="label">Data/s</div><div class="value" id="rf-data">-</div>
<div class="label">Batches</div><div class="value" id="rf-batches">-</div>
<div class="label">Overflows</div><div class="value" id="rf-overflow">-</div>
</div>
</div>

<!-- BT Operations -->
<div class="card" id="card-bt-ops" data-modes="bt">
<div class="card-title">BT Operations</div>
<div class="sub">Attack engine status and patchram state.</div>
<div class="status-grid">
<div class="label">Engine</div><div class="value" id="bt-ops-engine">-</div>
<div class="label">Rage Level</div><div class="value" id="bt-ops-rage">-</div>
<div class="label">Devices Seen</div><div class="value" id="bt-ops-devices">-</div>
<div class="label">Active Attacks</div><div class="value" id="bt-ops-active">-</div>
<div class="label">Total Attacks</div><div class="value" id="bt-ops-total">-</div>
<div class="label">Patchram</div><div class="value" id="bt-ops-patchram">-</div>
</div>
</div>

<!-- BT Rage Level -->
<div class="card" id="card-bt-rage" data-modes="bt">
<div class="card-title">BT Rage Level</div>
<div class="sub">Controls which attack categories are permitted.</div>
<div class="bt-rage-actions" style="display:flex;gap:8px;margin:12px 0">
<button class="bt-rage-btn" id="bt-rage-low" onclick="setBtRage('Low')">Low<br><span style="font-size:10px;font-weight:normal;color:#888">Diagnostics</span></button>
<button class="bt-rage-btn active" id="bt-rage-medium" onclick="setBtRage('Medium')">Medium<br><span style="font-size:10px;font-weight:normal">Active</span></button>
<button class="bt-rage-btn" id="bt-rage-high" onclick="setBtRage('High')">High<br><span style="font-size:10px;font-weight:normal">Aggressive</span></button>
</div>
<div id="bt-rage-desc" style="font-size:11px;color:#888;padding:8px;background:#0f346033;border-radius:6px">Medium: Active attacks targeting external devices</div>
</div>

<!-- BT Nearby Devices -->
<div class="card" id="card-bt-devices" data-modes="bt">
<div class="card-title">Nearby Devices</div>
<div class="sub">Bluetooth devices detected via HCI scanning.</div>
<div class="bt-dev-count" id="bt-dev-count"></div>
<div class="bt-dev-list" id="bt-dev-list">
<div class="bt-dev-empty">Scanning...</div>
</div>
</div>

<!-- BT Attacks -->
<div class="card" id="card-bt-attacks" data-modes="bt">
<div class="card-title">BT Attacks</div>
<div style="margin-bottom:12px">
<div style="font-size:12px;color:#888;margin-bottom:6px">Scan Mode</div>
<div style="display:flex;gap:8px">
<button class="bt-rage-btn" id="scan-mode-ble" onclick="setBtScanMode('ble')">BLE</button>
<button class="bt-rage-btn" id="scan-mode-classic" onclick="setBtScanMode('classic')">Classic</button>
<button class="bt-rage-btn active" id="scan-mode-both" onclick="setBtScanMode('both')">Both</button>
</div>
</div>
<div class="toggle-row bt-scan-ble" id="bt-row-smp_downgrade">
<div class="toggle-info"><div class="toggle-label">SMP Downgrade<span class="bt-badge bt-badge-auto">auto</span></div><div class="toggle-desc">Trick devices into weak pairing to steal encryption keys</div></div>
<label class="switch"><input type="checkbox" id="bt-atk-smp_downgrade" onchange="toggleBtAttack('smp_downgrade',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row bt-scan-classic" id="bt-row-knob">
<div class="toggle-info"><div class="toggle-label">KNOB<span class="bt-badge bt-badge-auto">auto</span></div><div class="toggle-desc">Shrink the encryption key so it's easy to crack</div></div>
<label class="switch"><input type="checkbox" id="bt-atk-knob" onchange="toggleBtAttack('knob',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row bt-scan-classic" id="bt-row-l2cap_fuzz">
<div class="toggle-info"><div class="toggle-label">L2CAP Fuzz<span class="bt-badge bt-badge-auto">auto</span></div><div class="toggle-desc">Send garbage data to crash Bluetooth connections</div></div>
<label class="switch"><input type="checkbox" id="bt-atk-l2cap_fuzz" onchange="toggleBtAttack('l2cap_fuzz',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row bt-scan-classic" id="bt-row-l2cap_conn_flood">
<div class="toggle-info"><div class="toggle-label">L2CAP Flood<span class="bt-badge bt-badge-auto">auto</span></div><div class="toggle-desc">Rapid connect/disconnect cycle to overwhelm target</div></div>
<label class="switch"><input type="checkbox" id="bt-atk-l2cap_conn_flood" onchange="toggleBtAttack('l2cap_conn_flood',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row bt-scan-ble" id="bt-row-att_gatt_fuzz">
<div class="toggle-info"><div class="toggle-label">ATT/GATT Fuzz<span class="bt-badge bt-badge-auto">auto</span></div><div class="toggle-desc">Send garbage BLE commands to crash devices</div></div>
<label class="switch"><input type="checkbox" id="bt-atk-att_gatt_fuzz" onchange="toggleBtAttack('att_gatt_fuzz',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row" id="bt-row-vendor_diag">
<div class="toggle-info"><div class="toggle-label">Controller Diagnostics<span class="bt-badge bt-badge-pr">PR</span></div><div class="toggle-desc">Read what's happening inside our Bluetooth chip</div></div>
<button class="bt-action-btn" onclick="launchVendorDiagnostics()" id="btn-vendor-diag">Run</button>
</div>
<div style="margin-top:10px;padding-top:10px;border-top:1px solid #0f3460">
<div style="font-size:12px;color:#888;margin-bottom:4px">BT Rage Level: <span id="bt-rage-label">Medium</span></div>
<div style="font-size:11px;color:#888">Controls which auto-attacks run. Medium+ enables KNOB and Clone.</div>
</div>
</div>

<!-- ═══════ LOOT ═══════ -->
<div class="section-label" data-modes="rage bt">Loot</div>

<!-- Whitelist -->
<div class="card" id="card-whitelist" data-modes="rage">
<div class="card-title">Whitelist</div>
<div class="sub">Networks and MACs excluded from attacks. Changes apply next epoch.</div>
<div id="wl-list"><div style="color:#555;font-size:12px">Loading...</div></div>
<div style="margin-top:10px;padding-top:10px;border-top:1px solid #0f3460;display:flex;gap:6px;align-items:center;flex-wrap:wrap">
<input type="text" id="wl-value" class="wl-input" placeholder="MAC or SSID" style="flex:2;min-width:120px">
<select id="wl-type" class="wl-select" style="flex:0 0 80px"><option value="MAC">MAC</option><option value="SSID">SSID</option></select>
<button class="wl-btn wl-btn-add" onclick="addWhitelist()">Add</button>
</div>
</div>

<!-- 10. Captures (merged: stats + list + download) -->
<div class="card" id="card-captures" data-modes="rage">
<div class="card-title">Trophies</div>
<div class="status-grid" style="margin-bottom:8px">
<div class="label">Total Files</div><div class="value" id="cap-total">-</div>
<div class="label">Crackable</div><div class="value" id="cap-hs">-</div>
<div class="label">Pending Upload</div><div class="value" id="cap-pending">-</div>
<div class="label">Total Size</div><div class="value" id="cap-size">-</div>
</div>

<div style="border-top:1px solid #0f3460;padding-top:10px;margin-top:2px;margin-bottom:10px">
<div style="font-size:12px;color:#888;margin-bottom:6px">Capture Mode</div>
<div style="font-size:12px;color:#aaa;margin-bottom:8px">Default: AO captures go to RAM first. Only verified handshakes (with crackable hash) get written to SD. Protects the SD card from wear.</div>
<div class="toggle-row" style="margin-bottom:6px">
<div class="toggle-info"><div class="toggle-label">Collect All</div><div class="toggle-desc">Keep every frame AO sees — partial handshakes, probes, mgmt frames. Writes directly to SD.</div></div>
<label class="switch"><input type="checkbox" id="capture-all-toggle" onchange="setCaptureAll(this.checked)"><span class="slider"></span></label>
</div>
<div id="capture-all-warning" style="display:none;color:#e67e22;font-size:11px;padding:6px 8px;background:#5a300033;border-radius:6px;margin-top:4px">
Warning: Collect All bypasses RAM buffering and writes everything directly to SD — probe requests, partial handshakes, management frames. Valuable for deeper analysis but causes significant SD wear. Use a high-endurance card and expect it to fill up faster.
</div>
</div>

<div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:6px">
<div style="display:flex;gap:6px">
<button class="rate-btn active" id="cap-filter-crack" onclick="setCapFilter('crackable')" style="font-size:11px;padding:4px 8px">Crackable</button>
<button class="rate-btn" id="cap-filter-all" onclick="setCapFilter('all')" style="font-size:11px;padding:4px 8px">All</button>
</div>
<a href="/api/download/all" class="action-btn btn-restart" style="text-decoration:none;text-align:center;font-size:11px;padding:4px 10px">Download ZIP</a>
</div>
<div class="captures-list" id="cap-list"><div style="color:#555;font-size:12px">Loading...</div></div>
</div>

<!-- 11. WPA-SEC Upload -->
<div class="card" id="card-wpasec" data-modes="rage">
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

<!-- 12. Cracked passwords -->
<div class="card" id="card-cracked" data-modes="rage">
<div class="card-title">Milk</div>
<div class="sub">Passwords milked from pwned cows.</div>
<div id="cracked-list"><div style="color:#555;font-size:12px">No cracked passwords yet</div></div>
</div>

<!-- BT Captures -->
<div class="card" id="card-bt-captures" data-modes="bt">
<div class="card-title">BT Captures</div>
<div class="sub">Artifacts captured during BT attacks.</div>
<div class="status-grid">
<div class="label">Keys</div><div class="value" id="bt-cap-keys">-</div>
<div class="label">Transcripts</div><div class="value" id="bt-cap-transcripts">-</div>
<div class="label">Crashes</div><div class="value" id="bt-cap-crashes">-</div>
<div class="label">Vendor</div><div class="value" id="bt-cap-vendor">-</div>
<div class="label">Total</div><div class="value" id="bt-cap-total" style="color:#00d4aa">-</div>
</div>
</div>

<!-- ═══════ CONNECTIVITY ═══════ -->
<div class="section-label">Connectivity</div>

<!-- 13. Bluetooth -->
<div class="card" id="card-bt">
<div class="card-title">Bluetooth</div>
<div class="status-grid" style="margin-bottom:10px">
<div class="label">Status</div><div class="value" id="bt-status">-</div>
<div class="label">Device</div><div class="value" id="bt-device">-</div>
<div class="label">IP</div><div class="value" id="bt-ip">-</div>
<div class="label">Internet</div><div class="value" id="bt-internet">-</div>
<div class="label">Retries</div><div class="value" id="bt-retries">-</div>
<div class="label">Feature Mode</div><div class="value" id="bt-feature-mode">-</div>
<div class="label">Nearby</div><div class="value" id="bt-nearby">-</div>
<div class="label">Contention</div><div class="value" id="bt-contention">-</div>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Discoverable</div><div class="toggle-desc">Make device visible for BT pairing</div></div>
<label class="switch"><input type="checkbox" id="bt-visible" onchange="toggleBtVisible(this.checked)"><span class="slider"></span></label>
</div>
<div class="card-section" style="border-top:1px solid #0f3460;padding-top:10px;margin-top:10px">
<div style="font-size:13px;font-weight:600;color:#e0e0e0;margin-bottom:8px">Phone Tethering</div>
<div style="display:flex;gap:8px;align-items:center">
<button class="util-btn" onclick="btScan()" id="bt-scan-btn">Scan for Devices</button>
<button class="util-btn util-btn-danger" id="bt-disconnect-btn" onclick="btDisconnect()" style="display:none">Disconnect</button>
</div>
<div id="bt-device-list" style="margin-top:8px"></div>
<div id="bt-passkey-area" style="display:none;margin-top:10px;padding:10px;background:#0a1628;border-radius:8px">
<div style="color:#e0e0e0;font-size:12px">Confirm passkey matches your phone:</div>
<div id="bt-passkey-code" style="font-size:24px;font-weight:bold;color:#00d4ff;text-align:center;padding:10px">------</div>
<div style="display:flex;gap:8px;justify-content:center">
<button class="util-btn util-btn-confirm" onclick="btConfirmPasskey(true)">Confirm</button>
<button class="util-btn util-btn-reject" onclick="btConfirmPasskey(false)">Reject</button>
</div>
</div>
</div>
</div>

<!-- 15. Discord Webhook -->
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
<div class="section-label" data-modes="rage">Status</div>

<!-- 16. Recovery status -->
<div class="card" id="card-recovery" data-modes="rage">
<div class="card-title">Recovery Status</div>
<div class="sub">WiFi and firmware crash recovery tracking.</div>
<div class="health-row" style="margin-bottom:8px">
<div class="health-item"><span class="dot dot-gray" id="h-wifi"></span>WiFi</div>
<div class="health-item"><span class="dot dot-gray" id="h-ao"></span>AO</div>
<div class="health-item"><span class="dot dot-gray" id="h-recovery"></span>Recovery</div>
<div class="health-item"><span class="dot dot-gray" id="h-firmware"></span>Firmware</div>
<div class="health-item"><span class="dot dot-gray" id="h-gps"></span>GPS</div>
</div>
<div class="status-grid">
<div class="label">State</div><div class="value" id="rec-state">-</div>
<div class="label">Crashes</div><div class="value" id="rec-crashes">-</div>
<div class="label">Recoveries</div><div class="value" id="rec-total">-</div>
<div class="label">Last Recovery</div><div class="value" id="rec-last">-</div>
<div class="label">AO PID</div><div class="value" id="rec-pid">-</div>
<div class="label">AO Uptime</div><div class="value" id="rec-ao-up">-</div>
<div class="label">Firmware</div><div class="value" id="fw-health">-</div>
<div class="label">Crash Suppress</div><div class="value" id="fw-crash">-</div>
<div class="label">HardFault</div><div class="value" id="fw-fault">-</div>
</div>
</div>

<!-- 17. Personality -->
<div class="card" id="card-personality">
<div class="card-title">Personality</div>
<div class="sub">Mood, experience, and level progression.</div>
<div class="status-grid">
<div class="label">Mooooood</div><div class="value" id="p-mood">-</div>
<div class="label">Face</div><div class="value" id="p-face">-</div>
<div class="label">XP</div><div class="value" id="p-xp">-</div>
<div class="label">Level</div><div class="value" id="p-level">-</div>
<div class="label">Blind Epochs</div><div class="value" id="p-blind">-</div>
</div>
<div class="progress-bar" style="margin-top:8px"><div class="progress-fill" id="mood-bar" style="width:50%"></div></div>
</div>

<!-- ═══════ MANAGEMENT ═══════ -->
<div class="section-label">Management</div>

<!-- 18. Actions -->
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

<!-- 19. Plugins -->
<div class="card" id="card-plugins">
<div class="card-title">Plugins</div>
<div class="sub">Lua plugins control display indicators. Toggle on/off and set x,y positions.</div>
<div id="plugins-list"><div style="color:#555;font-size:12px">Loading...</div></div>
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
<div class="sub">Device configuration. Changes are persisted across restarts.</div>
<div style="margin-bottom:10px">
<div style="font-size:12px;color:#888;margin-bottom:4px">Device Name</div>
<div style="display:flex;gap:6px">
<input type="text" id="setting-name" class="wl-input" placeholder="oxigotchi" style="flex:2">
</div>
</div>

<div style="font-size:13px;color:#00d4aa;font-weight:bold;margin:12px 0 6px">Display</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Invert Display</div><div class="toggle-desc">White on black (recommended for e-ink)</div></div>
<label class="switch"><input type="checkbox" id="setting-invert" checked><span class="slider"></span></label>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Rotation</div></div>
<select id="setting-rotation" style="background:#1a1a2e;border:1px solid #0f3460;border-radius:6px;padding:6px 10px;color:#e0e0e0;font-family:inherit;font-size:13px">
<option value="0">0&deg;</option><option value="180" selected>180&deg;</option>
</select>
</div>

<div data-modes="rage safe">
<div style="font-size:13px;color:#00d4aa;font-weight:bold;margin:12px 0 6px">WiFi Tuning</div>
<div style="margin-bottom:8px">
<div style="font-size:12px;color:#888;margin-bottom:4px">Min RSSI: <span id="setting-rssi-val">-100</span> dBm</div>
<input type="range" id="setting-rssi" class="ch-slider" min="-100" max="-30" step="1" value="-100" oninput="document.getElementById('setting-rssi-val').textContent=this.value">
<div style="display:flex;justify-content:space-between;font-size:10px;color:#555"><span>-100 (all)</span><span>-30 (strong only)</span></div>
</div>
<div style="margin-bottom:8px">
<div style="font-size:12px;color:#888;margin-bottom:4px">AP TTL: <span id="setting-ttl-val">120</span>s</div>
<input type="range" id="setting-ttl" class="ch-slider" min="30" max="600" step="10" value="120" oninput="document.getElementById('setting-ttl-val').textContent=this.value">
<div style="display:flex;justify-content:space-between;font-size:10px;color:#555"><span>30s (forget fast)</span><span>600s (remember long)</span></div>
</div>
</div>

<div style="font-size:13px;color:#00d4aa;font-weight:bold;margin:12px 0 6px">Display</div>
<div style="margin-bottom:8px">
<div style="font-size:12px;color:#888;margin-bottom:4px">Full refresh every: <span id="setting-refresh-val">10</span> partials</div>
<input type="range" id="setting-refresh" class="ch-slider" min="10" max="500" step="1" value="10" oninput="document.getElementById('setting-refresh-val').textContent=this.value">
<div style="display:flex;justify-content:space-between;font-size:10px;color:#555"><span>10 (less ghosting)</span><span>500 (less flicker)</span></div>
</div>

<div style="margin-top:12px">
<button class="wl-btn wl-btn-add" onclick="saveSettings()" style="width:100%">Save Settings</button>
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
function esc(s) { var d = document.createElement('div'); d.textContent = s; return d.innerHTML.replace(/\\/g, '&#92;').replace(/'/g, '&#39;'); }

var _currentMode = 'rage';
var _lastHydratedMode = null;
var _overviewState = {mode: 'RAGE'};
var _btPatchramState = '';
function normalizeMode(raw) {
    var m = (raw || '').toUpperCase();
    if (m === 'AO' || m === 'RAGE') return 'rage';
    if (m === 'BT') return 'bt';
    if (m === 'PWN' || m === 'SAFE') return 'safe';
    return 'rage';
}

function mergeOverviewState(patch) {
    if (!patch) return;
    Object.keys(patch).forEach(function(key) {
        _overviewState[key] = patch[key];
    });
}

function updateSectionLabelVisibility() {
    document.querySelectorAll('.section-label').forEach(function(label) {
        var node = label.nextElementSibling;
        var show = false;
        while (node && !node.classList.contains('section-label')) {
            var cards = [];
            if (node.classList.contains('card')) {
                cards = [node];
            } else {
                cards = Array.prototype.slice.call(node.querySelectorAll('.card'));
            }
            if (cards.some(function(card) { return card.style.display !== 'none'; })) {
                show = true;
                break;
            }
            node = node.nextElementSibling;
        }
        label.style.display = show ? '' : 'none';
    });
}

function applyModeVisibility(rawMode) {
    var mode = normalizeMode(rawMode);
    _currentMode = mode;
    document.querySelectorAll('[data-modes]').forEach(function(el) {
        var modes = el.getAttribute('data-modes').split(' ');
        el.style.display = modes.indexOf(mode) >= 0 ? 'block' : 'none';
    });
    updateSectionLabelVisibility();
}

function setSlot(n, label, value) {
    var l = document.getElementById('s-label-' + n);
    var v = document.getElementById('s-val-' + n);
    if (l) l.textContent = label;
    if (v) v.textContent = value != null ? value : '-';
}

function updateOverview(state) {
    var mode = normalizeMode(state.mode);
    if (mode === 'rage') {
        setSlot(1, 'CH', state.channel);
        setSlot(2, 'COWS', state.aps_seen);
        setSlot(3, 'PWND', state.handshakes);
        setSlot(4, 'CHARGES', state.epoch);
        setSlot(5, 'UPTIME', state.uptime);
        setSlot(6, 'RATE', state.attacks ? state.attacks.attack_rate : '-');
    } else if (mode === 'bt') {
        var bta = state.bt_attacks ? state.bt_attacks.stats : {};
        setSlot(1, 'DEVICES', bta.devices_seen != null ? bta.devices_seen : '-');
        setSlot(2, 'ACTIVE', bta.active_attacks != null ? bta.active_attacks : '-');
        setSlot(3, 'CAPTURES', bta.total_captures != null ? bta.total_captures : '-');
        setSlot(4, 'ATTACKS', bta.total_attacks != null ? bta.total_attacks : '-');
        setSlot(5, 'UPTIME', state.uptime);
        setSlot(6, 'RAGE', state.bt_attacks ? state.bt_attacks.rage_level : '-');
    } else {
        var bt = state.bluetooth || {};
        setSlot(1, 'BT', bt.connected ? 'Connected' : (bt.state || '-'));
        setSlot(2, 'DEVICE', bt.device_name || '-');
        setSlot(3, 'NET', bt.internet_available ? 'Yes' : 'No');
        setSlot(4, 'NEARBY', bt.nearby_devices != null ? bt.nearby_devices : '-');
        setSlot(5, 'UPTIME', state.uptime);
        setSlot(6, 'MODE', 'SAFE');
    }
}

function syncModeUi(rawMode) {
    var newMode = normalizeMode(rawMode);
    if (newMode !== _currentMode) {
        if (newMode !== 'bt') delete _overviewState.bt_attacks;
        if (newMode !== 'safe') delete _overviewState.bluetooth;
        if (newMode !== 'rage') delete _overviewState.attacks;
    }
    document.getElementById('mode-rage').classList.toggle('active', rawMode === 'RAGE' || rawMode === 'AO');
    document.getElementById('mode-bt').classList.toggle('active', rawMode === 'BT');
    document.getElementById('bt-tether-warn').style.display = rawMode === 'BT' ? 'block' : 'none';
    document.getElementById('mode-safe').classList.toggle('active', rawMode === 'SAFE' || rawMode === 'PWN');
    applyModeVisibility(rawMode);
}

function refreshModeScopedData(rawMode) {
    var mode = normalizeMode(rawMode);
    if (mode === _lastHydratedMode) return;
    _lastHydratedMode = mode;
    if (mode === 'bt') refreshBtAttacks();
    if (mode === 'bt' || mode === 'safe') refreshBluetooth();
}

// --- Refresh functions ---

function refreshStatus() {
    api('GET', '/api/status').then(function(d) {
        if (!d) return;
        mergeOverviewState(d);
        syncModeUi(d.mode);
        updateOverview(_overviewState);
        var nameInput = document.getElementById('setting-name');
        if (nameInput && !nameInput.matches(':focus')) nameInput.value = d.name || '';
        syncSettingsFromData(d);
        refreshModeScopedData(d.mode);
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
        mergeOverviewState({bluetooth: d});
        updateOverview(_overviewState);
        document.getElementById('bt-status').textContent = d.connected ? 'Connected' : d.state;
        document.getElementById('bt-status').style.color = d.connected ? '#00d4aa' : '#888';
        document.getElementById('bt-device').textContent = d.device_name || '-';
        document.getElementById('bt-ip').textContent = d.ip || '-';
        document.getElementById('bt-internet').textContent = d.internet_available ? 'Yes' : 'No';
        document.getElementById('bt-internet').style.color = d.internet_available ? '#00d4aa' : '#888';
        document.getElementById('bt-retries').textContent = d.retry_count;
        document.getElementById('bt-feature-mode').textContent = d.feature_mode || '-';
        document.getElementById('bt-nearby').textContent = d.nearby_devices != null ? d.nearby_devices : '-';
        document.getElementById('bt-contention').textContent = d.contention_score != null ? d.contention_score : '-';
    });
}


var _chConfigCooldown = 0;
function refreshWifi() {
    api('GET', '/api/wifi').then(function(d) {
        if (!d) return;
        document.getElementById('wifi-state').textContent = d.state;
        document.getElementById('wifi-state').style.color = d.state === 'Monitor' ? '#00d4aa' : '#e94560';
        document.getElementById('wifi-ch').textContent = d.channel;
        document.getElementById('wifi-aps').textContent = d.aps_tracked;
        document.getElementById('wifi-channels').textContent = d.channels.join(', ') || '-';
        document.getElementById('wifi-dwell').textContent = d.dwell_ms + 'ms';
        // Populate channel config card — skip if user recently applied changes (cooldown)
        if (Date.now() < _chConfigCooldown) return;
        if (!d.autohunt_enabled) {
            document.getElementById('ch-list').value = d.channels.join(',');
            _savedChannels = d.channels.slice();
        }
        renderChannelButtons(d.autohunt_enabled ? [] : d.channels);
        var dwInput = document.getElementById('ch-dwell');
        if (dwInput && !dwInput.matches(':active')) { dwInput.value = d.dwell_ms; document.getElementById('ch-dwell-val').textContent = d.dwell_ms; }
        var ahToggle = document.getElementById('autohunt-toggle');
        if (ahToggle) ahToggle.checked = d.autohunt_enabled;
        var scToggle = document.getElementById('skip-captured-toggle');
        if (scToggle) scToggle.checked = d.skip_captured;
        if (d.rage_level != null) {
            updateRageLabel(d.rage_level, true);
        } else {
            updateRageLabel(0, false);
        }
    });
}

function refreshAttacks() {
    api('GET', '/api/attacks').then(function(d) {
        if (!d) return;
        mergeOverviewState({attacks: d});
        updateOverview(_overviewState);
        ['deauth','pmkid','csa','disassoc','anon_reassoc','rogue_m2'].forEach(function(k) {
            var cb = document.getElementById('atk-'+k);
            if (cb) cb.checked = d[k];
        });
        [1,2,3].forEach(function(n) {
            document.getElementById('rate-'+n).classList.toggle('active', n === d.attack_rate);
        });
    });
}

function updateRfFromWs(d) {
    var rfCard = document.getElementById('card-rf');
    if (rfCard) rfCard.style.display = (d.enabled && d.available) ? 'block' : 'none';
    if (!d.enabled || !d.available) return;
    var speed = (d.last_batch_size > 0 && d.last_batch_duration_us > 0)
        ? (d.last_batch_size / (d.last_batch_duration_us / 1000)).toFixed(0)
        : '-';
    document.getElementById('rf-speed').textContent = speed;
    document.getElementById('rf-total').textContent = d.frames_classified || 0;
    document.getElementById('rf-bssids').textContent = d.unique_bssids || 0;
    document.getElementById('rf-dominant').textContent = d.dominant_class || '-';
    document.getElementById('rf-beacon').textContent = (d.beacon_rate || 0).toFixed(1);
    document.getElementById('rf-probe').textContent = (d.probe_rate || 0).toFixed(1);
    var deRate = d.deauth_rate || 0;
    var deEl = document.getElementById('rf-deauth');
    deEl.textContent = deRate.toFixed(1);
    deEl.style.color = deRate > 5 ? '#e94560' : '#e0e0e0';
    document.getElementById('rf-data').textContent = (d.data_rate || 0).toFixed(1);
    document.getElementById('rf-batches').textContent = d.batches_processed || 0;
    var ovCount = d.overflow_count || 0;
    var ovEl = document.getElementById('rf-overflow');
    ovEl.textContent = ovCount;
    ovEl.style.color = ovCount > 0 ? '#f0c040' : '#e0e0e0';
}

function refreshRf() {
    api('GET', '/api/qpu').then(function(d) {
        if (!d) return;
        updateRfFromWs(d);
    });
}

var _capFiles = [];
var _capFilter = 'crackable';

function setCapFilter(mode) {
    _capFilter = mode;
    document.getElementById('cap-filter-crack').classList.toggle('active', mode === 'crackable');
    document.getElementById('cap-filter-all').classList.toggle('active', mode === 'all');
    renderCapList();
}

function syncCaptureModeUi(enabled) {
    var tog = document.getElementById('capture-all-toggle');
    if (tog) tog.checked = !!enabled;
    document.getElementById('capture-all-warning').style.display = enabled ? 'block' : 'none';
}

function setCaptureAll(enabled) {
    syncCaptureModeUi(enabled);
    api('POST', '/api/capture-all', {enabled: enabled}).then(function(r) {
        if (r && r.ok) toast(enabled ? 'Collect All enabled — AO will restart' : 'Verified Only enabled — AO will restart');
    });
}

function capDisplayName(f) {
    var ssid = f.ssid && f.ssid.length ? f.ssid : '';
    var mac = f.bssid_mac && f.bssid_mac !== '00:00:00:00:00:00' ? f.bssid_mac : '';
    var date = f.captured_date || '';
    if (ssid && mac) return esc(ssid) + ' \u00b7 ' + esc(mac) + (date ? ' \u00b7 ' + esc(date) : '');
    if (ssid) return esc(ssid) + ' \u00b7 ' + esc(f.filename) + (date ? ' \u00b7 ' + esc(date) : '');
    return esc(f.filename);
}

function renderCapList() {
    var el = document.getElementById('cap-list');
    var files = _capFilter === 'crackable'
        ? _capFiles.filter(function(f) { return f.has_handshake; })
        : _capFiles;
    if (!files.length) {
        el.innerHTML = '<div style="color:#555;font-size:12px">' +
            (_capFilter === 'crackable' ? 'No crackable captures yet' : 'No captures yet') + '</div>';
        return;
    }
    el.innerHTML = files.map(function(f) {
        var badge = f.has_handshake
            ? '<span style="color:#00d4aa;font-size:10px;margin-left:6px">\u2713 crackable</span>'
            : '<span style="color:#888;font-size:10px;margin-left:6px">~ partial</span>';
        var fn_ = encodeURIComponent(f.filename);
        return '<div class="capture-item" style="display:flex;align-items:center;gap:6px">' +
            '<a href="/api/download/' + fn_ + '" style="color:#00d4aa;text-decoration:none;flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">' + capDisplayName(f) + '</a>' +
            badge + ' <span style="color:#555;font-size:11px">(' + fmtBytes(f.size_bytes) + ')</span>' +
            '<button onclick="deleteCapture(\'' + fn_ + '\')" style="background:none;border:none;color:#c0392b;font-size:14px;cursor:pointer;padding:0 2px;flex-shrink:0" title="Delete">\u2715</button>' +
            '</div>';
    }).join('');
}

function deleteCapture(encodedFilename) {
    var filename = decodeURIComponent(encodedFilename);
    if (!confirm('Delete ' + filename + '?\nThis removes the file from the SD card.')) return;
    _capFiles = _capFiles.filter(function(f) { return f.filename !== filename; });
    renderCapList();
    api('DELETE', '/api/captures/' + encodedFilename, null);
}

function refreshCaptures() {
    api('GET', '/api/captures').then(function(d) {
        if (!d) return;
        document.getElementById('cap-total').textContent = d.total_files;
        document.getElementById('cap-hs').textContent = d.handshake_files;
        document.getElementById('cap-pending').textContent = d.pending_upload;
        document.getElementById('cap-size').textContent = fmtBytes(d.total_size_bytes);
        syncCaptureModeUi(d.capture_all);
        _capFiles = d.files || [];
        renderCapList();
    });
}

function refreshRecovery() {
    api('GET', '/api/recovery').then(function(d) {
        if (!d) return;
        document.getElementById('rec-state').textContent = d.state;
        document.getElementById('rec-state').style.color = d.state === 'Healthy' ? '#00d4aa' : '#f0c040';
        document.getElementById('rec-total').textContent = d.total_recoveries;
        document.getElementById('rec-last').textContent = d.last_recovery;
        // Firmware health
        document.getElementById('fw-health').textContent = d.fw_health || '-';
        var fwColor = d.fw_health === 'Healthy' ? '#00d4aa' : d.fw_health === 'Degraded' ? '#f0c040' : d.fw_health === 'Critical' ? '#e74c3c' : '#888';
        document.getElementById('fw-health').style.color = fwColor;
        document.getElementById('fw-crash').textContent = d.fw_crash_suppress != null ? d.fw_crash_suppress : '-';
        document.getElementById('fw-fault').textContent = d.fw_hardfault != null ? d.fw_hardfault : '-';
        // Firmware health dot
        var fdot = document.getElementById('h-firmware');
        fdot.className = 'dot ' + (d.fw_health === 'Healthy' ? 'dot-green' : d.fw_health === 'Degraded' ? 'dot-yellow' : d.fw_health === 'Critical' ? 'dot-red' : 'dot-gray');
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
            var label = esc(c.ssid || c.bssid);
            if (c.ssid && c.bssid) label += ' \u00b7 ' + esc(c.bssid);
            if (c.date) label += ' \u00b7 ' + esc(c.date);
            return '<div style="padding:4px 0;border-bottom:1px solid #0f346022">' +
                '<span style="color:#00d4aa;font-weight:bold;font-size:11px">' + label + '</span>' +
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
            var hsIcon = ap.has_handshake ? '<span style="color:#00d4aa" title="Handshake or PMKID captured — hash saved, ready to crack">&#9733;</span>' : '';
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

var _savedChannels = [1, 6, 11]; // remembered channels when autohunt is toggled off

function renderChannelButtons(activeChannels) {
    var container = document.getElementById('ch-btns');
    if (!container) return;
    var autohunt = document.getElementById('autohunt-toggle');
    var isAutohunt = autohunt && autohunt.checked;
    var safe = [1, 6, 11];
    var html = '';
    for (var ch = 1; ch <= 13; ch++) {
        var active = !isAutohunt && activeChannels.indexOf(ch) !== -1;
        var isSafe = safe.indexOf(ch) !== -1;
        var bg, fg;
        if (isAutohunt) {
            bg = '#0a1628'; fg = '#444';
        } else {
            bg = active ? (isSafe ? '#00d4aa' : '#e67e22') : '#0f3460';
            fg = active ? '#1a1a2e' : '#888';
        }
        var disabled = isAutohunt ? ' pointer-events:none;opacity:0.5;' : '';
        html += '<button onclick="toggleChannel(' + ch + ')" style="width:36px;height:32px;border:none;border-radius:6px;background:' + bg + ';color:' + fg + ';font-family:inherit;font-size:13px;font-weight:bold;cursor:pointer;' + disabled + '">' + ch + '</button>';
    }
    container.innerHTML = html;
}

function toggleChannel(ch) {
    if (document.getElementById('autohunt-toggle').checked) return;
    var input = document.getElementById('ch-list');
    var channels = input.value.split(',').map(function(c){ return parseInt(c.trim()); }).filter(function(c){ return !isNaN(c) && c > 0; });
    var idx = channels.indexOf(ch);
    if (idx !== -1) {
        channels.splice(idx, 1);
    } else {
        channels.push(ch);
        channels.sort(function(a,b){ return a-b; });
    }
    if (!channels.length) channels = [1]; // at least one channel
    input.value = channels.join(',');
    _savedChannels = channels.slice();
    renderChannelButtons(channels);
}

function applyChannels() {
    breakRage();
    var chStr = document.getElementById('ch-list').value.trim();
    var dwell = parseInt(document.getElementById('ch-dwell').value) || 2000;
    var autohunt = document.getElementById('autohunt-toggle').checked;
    var channels = null;
    if (chStr) {
        channels = chStr.split(',').map(function(c){ return parseInt(c.trim()); }).filter(function(c){ return !isNaN(c) && c > 0 && c <= 14; });
        if (!channels.length) { toast('Select at least one channel'); return; }
    }
    _chConfigCooldown = Date.now() + 45000;
    api('POST', '/api/channels', {channels: channels, dwell_ms: dwell, autohunt: autohunt}).then(function(r) {
        if (r && r.ok) toast('Channel config applied');
    });
}

function toggleAutohunt(enabled) {
    breakRage();
    var input = document.getElementById('ch-list');
    var dwell = parseInt(document.getElementById('ch-dwell').value) || 2000;
    if (enabled) {
        // Save current channels before greying out
        var cur = input.value.split(',').map(function(c){ return parseInt(c.trim()); }).filter(function(c){ return !isNaN(c) && c > 0; });
        if (cur.length) _savedChannels = cur;
        renderChannelButtons([]);
    } else {
        // Restore saved channels
        input.value = _savedChannels.join(',');
        renderChannelButtons(_savedChannels);
    }
    var channels = enabled ? null : _savedChannels;
    _chConfigCooldown = Date.now() + 45000;
    api('POST', '/api/channels', {channels: channels, dwell_ms: dwell, autohunt: enabled}).then(function(r) {
        if (r && r.ok) toast('Autohunt ' + (enabled ? 'ON — AO scans all channels' : 'OFF — using selected channels'));
    });
}

function toggleSkipCaptured(on) {
    api('POST', '/api/wifi', {skip_captured: on}).then(function(r) {
        if (r && r.ok) toast('Smart Skip ' + (on ? 'ON — skipping captured APs' : 'OFF — attacking all APs'));
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
var _rageNames = {1:'Chill',2:'Lurk',3:'Prowl',4:'Hunt',5:'RAGE',6:'FURY',7:'YOLO'};
var _ragePresets = {
    1:{rate:1,dwell:5000,ch:[1,6,11]},
    2:{rate:1,dwell:2000,ch:[1,6,11]},
    3:{rate:1,dwell:2000,ch:[1,2,3,4,5,6,7,8,9,10,11,12,13]},
    4:{rate:2,dwell:2000,ch:[1,2,3,4,5,6,7,8,9,10,11,12,13]},
    5:{rate:2,dwell:1000,ch:[1,2,3,4,5,6,7,8,9,10,11,12,13]},
    6:{rate:3,dwell:1000,ch:[1,2,3,4,5,6,7,8,9,10,11,12,13]},
    7:{rate:3,dwell:500,ch:[1,2,3,4,5,6,7,8,9,10,11,12,13]}
};

function updateRageLabel(level, enabled) {
    var label = document.getElementById('rage-label');
    var slider = document.getElementById('rage-slider');
    var desc = document.getElementById('rage-desc');
    var yolo = document.getElementById('rage-yolo');
    var toggle = document.getElementById('rage-toggle');
    if (enabled && level >= 1 && level <= 7) {
        toggle.checked = true;
        slider.disabled = false;
        slider.value = level;
        label.textContent = level + ' \u2014 ' + (_rageNames[level] || '?');
        label.className = 'rage-level' + (level === 7 ? ' yolo' : '');
        desc.textContent = _rageNames[level] + ' preset active';
        yolo.style.display = level === 7 ? 'block' : 'none';
        // Sync rate buttons, channels, and dwell to preset values instantly
        var p = _ragePresets[level];
        if (p) {
            [1,2,3].forEach(function(n) {
                document.getElementById('rate-'+n).classList.toggle('active', n === p.rate);
            });
            var ahToggle = document.getElementById('autohunt-toggle');
            if (ahToggle) ahToggle.checked = false;
            document.getElementById('ch-list').value = p.ch.join(',');
            renderChannelButtons(p.ch);
            var dwInput = document.getElementById('ch-dwell');
            if (dwInput) { dwInput.value = p.dwell; document.getElementById('ch-dwell-val').textContent = p.dwell; }
        }
    } else {
        toggle.checked = false;
        slider.disabled = true;
        label.textContent = '\u2014';
        label.className = 'rage-level';
        desc.textContent = 'Custom \u2014 individual controls active';
        yolo.style.display = 'none';
    }
}

function toggleRage(on) {
    if (on) {
        var level = parseInt(document.getElementById('rage-slider').value) || 4;
        api('POST', '/api/rage', {level: level}).then(function(r) {
            if (r && r.ok) { updateRageLabel(level, true); toast('RAGE ' + _rageNames[level]); refreshWifi(); refreshAttacks(); }
        });
    } else {
        api('POST', '/api/rage', {level: null}).then(function(r) {
            if (r && r.ok) { updateRageLabel(0, false); toast('Custom mode'); refreshWifi(); }
        });
    }
}

function slideRage(level) {
    if (!document.getElementById('rage-toggle').checked) return;
    api('POST', '/api/rage', {level: level}).then(function(r) {
        if (r && r.ok) { updateRageLabel(level, true); toast('RAGE ' + _rageNames[level]); refreshWifi(); refreshAttacks(); }
    });
}

function breakRage() {
    var toggle = document.getElementById('rage-toggle');
    if (toggle.checked) {
        api('POST', '/api/rage', {level: null});
        updateRageLabel(0, false);
    }
}

function setRate(r) {
    breakRage();
    api('POST', '/api/rate', {rate: r}).then(function() {
        [1,2,3].forEach(function(n) {
            document.getElementById('rate-'+n).classList.toggle('active', n === r);
        });
        toast('Rate set to ' + r);
    });
}
function switchMode(mode) {
    document.getElementById('bt-tether-warn').style.display = mode === 'BT' ? 'block' : 'none';
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
    document.getElementById('bt-device-list').innerHTML = '<div style="color:#888;font-size:12px">Scanning for nearby devices (~10s)...</div>';
    api('POST', '/api/bluetooth/scan', {}).then(function() {
        // Poll for results every 2s
        var poll = setInterval(function() {
            api('GET', '/api/bluetooth/scan').then(function(devices) {
                if (!devices) return;
                if (devices.length > 0) {
                    clearInterval(poll);
                    btn.textContent = 'Scan for Devices';
                    btn.disabled = false;
                    renderBtDeviceList(devices);
                }
            });
        }, 2000);
        // Stop polling after 20s — scan done or no devices found
        setTimeout(function() {
            clearInterval(poll);
            btn.textContent = 'Scan for Devices';
            btn.disabled = false;
            if (document.getElementById('bt-device-list').innerHTML.indexOf('Scanning') !== -1) {
                document.getElementById('bt-device-list').innerHTML = '<div style="color:#888;font-size:12px">No devices found. Make sure your phone\'s Bluetooth is on.</div>';
            }
        }, 20000);
    });
}

function btPair(mac, name) {
    var label = name || mac;
    toast('Pairing with ' + label + '...');
    api('POST', '/api/bluetooth/pair', {mac: mac}).then(function(r) {
        if (r && r.ok) {
            toast(r.message);
            document.getElementById('bt-device-list').innerHTML = '<div style="color:#f0c040;font-size:12px" id="bt-pair-status">&#9881; Pairing with ' + esc(label) + '... confirm on your phone</div>';
        }
    });
}

function btForget(m) {
    if (confirm('Remove ' + m + ' from paired devices?')) {
        api('POST', '/api/bluetooth/forget', {mac: m}).then(function(r) {
            if (r && r.ok) toast('Device removed');
        });
    }
}

function btDisconnect() {
    api('POST', '/api/bluetooth/disconnect').then(function(r) {
        if (r && r.ok) toast('BT disconnected');
    });
}

function btConfirmPasskey(c) {
    api('POST', '/api/bluetooth/confirm-passkey', {confirmed: c});
    document.getElementById('bt-passkey-area').style.display = 'none';
}

function renderBtDeviceList(devices) {
    var dl = document.getElementById('bt-device-list');
    if (!dl || !devices || devices.length === 0) return;
    var html = '<div style="font-size:11px;color:#888;margin-bottom:4px">Found ' + devices.length + ' device(s):</div>';
    devices.forEach(function(v) {
        var ms = v.mac ? v.mac.slice(-5) : '';
        html += '<div onclick="btPair(\'' + esc(v.mac) + '\',\'' + esc(v.name || '') + '\')" style="display:flex;justify-content:space-between;align-items:center;padding:8px;margin:4px 0;border:1px solid #0f3460;border-radius:8px;background:#0a1628;cursor:pointer;transition:all .2s" onmouseover="this.style.borderColor=\'#00d4aa\';this.style.background=\'#0f3460\'" onmouseout="this.style.borderColor=\'#0f3460\';this.style.background=\'#0a1628\'">' +
            '<span style="color:#e0e0e0;font-size:12px">' + esc(v.name || 'Unknown') + ' <span style="color:#666;font-size:10px">(' + ms + ')</span></span>' +
            '<span style="color:#00d4aa;font-size:11px;font-weight:600">Pair &rarr;</span></div>';
    });
    dl.innerHTML = html;
}

// --- BT Offensive functions ---

window._btManualPending = false;
window._btManualTarget = null;
window._btManualAttack = null;
window._btRageLevel = 'Medium';
window._btPatchramReady = false;

function isPatchramReady() { return window._btPatchramReady; }

function toggleBtAttack(name, val) {
    var data = {};
    data[name] = val;
    api('POST', '/api/bt/attacks/toggle', data).then(function() {
        toast('BT attack ' + name + (val ? ' ON' : ' OFF'));
        updateBtPatchramConstraintState();
    });
}

function launchManualAttack(address, attack) {
    if (window._btManualPending) {
        toast('Manual attack already pending');
        return;
    }
    window._btManualPending = true;
    window._btManualTarget = address;
    window._btManualAttack = attack;
    var label = {knob:'KNOB',ble_adv_injection:'Clone',l2cap_fuzz:'Fuzz',l2cap_conn_flood:'Flood',att_gatt_fuzz:'Fuzz'}[attack] || attack;
    toast(label + ' attack launched on ' + address);
    fetch('/api/bt/attacks/manual', {
        method: 'POST',
        headers: {'Content-Type': 'application/json'},
        body: JSON.stringify({address: address, attack: attack})
    }).then(function(r) { return r.json(); }).then(function(j) {
        if (!j.ok) {
            toast(j.message || 'Attack failed to queue');
            window._btManualPending = false;
        }
    }).catch(function() {
        toast('Failed to send attack request');
        window._btManualPending = false;
    });
}

function launchVendorDiagnostics() {
    if (window._btManualPending) {
        toast('Manual attack already pending');
        return;
    }
    window._btManualPending = true;
    toast('Running controller diagnostics...');
    fetch('/api/bt/attacks/manual', {
        method: 'POST',
        headers: {'Content-Type': 'application/json'},
        body: JSON.stringify({attack: 'vendor_cmd_unlock'})
    }).then(function(r) { return r.json(); }).then(function(j) {
        if (!j.ok) {
            toast(j.message || 'Diagnostics failed to queue');
            window._btManualPending = false;
        }
    }).catch(function() {
        toast('Failed to send diagnostics request');
        window._btManualPending = false;
    });
}

function rssiToSignal(rssi) {
    if (rssi == null) return {cls:'sig-dead',bars:[5,0,0,0]};
    if (rssi > -50) return {cls:'sig-strong',bars:[5,9,13,18]};
    if (rssi > -65) return {cls:'sig-medium',bars:[5,9,13,0]};
    if (rssi > -80) return {cls:'sig-weak',bars:[5,9,0,0]};
    return {cls:'sig-dead',bars:[5,0,0,0]};
}
function updateBtDevicesFromWs(btDevices) {
    if (!btDevices || !btDevices.devices) return;
    var list = document.getElementById('bt-dev-list');
    var countEl = document.getElementById('bt-dev-count');
    if (btDevices.devices.length === 0) {
        list.innerHTML = '<div class="bt-dev-empty">No devices yet</div>';
        if (countEl) countEl.textContent = '';
        return;
    }
    if (countEl) countEl.textContent = btDevices.devices.length + ' device' + (btDevices.devices.length !== 1 ? 's' : '');
    var rage = (window._btRageLevel || 'Medium').toLowerCase();
    var rageMedium = rage === 'medium' || rage === 'high';
    var html = '';
    btDevices.devices.forEach(function(d) {
        var sig = rssiToSignal(d.rssi);
        var bars = '<div class="bt-dev-signal ' + sig.cls + '">';
        for (var i = 0; i < 4; i++) bars += '<div class="bar" style="height:' + (sig.bars[i] || 3) + 'px"></div>';
        bars += '</div>';

        var nameStr = d.name ? esc(d.name) : (d.vendor ? esc(d.vendor) + ' Device' : d.address.substring(0, 8) + '...');
        var nameCls = d.name ? '' : ' unnamed';

        var meta = '';
        if (d.vendor) meta += '<span class="bt-dev-vendor">' + esc(d.vendor) + '</span>';
        meta += '<span class="bt-dev-addr">' + esc(d.address) + '</span>';
        if (d.last_attack && st !== 'untouched') {
            var atkShort = d.last_attack.replace(/([A-Z])/g, ' $1').trim();
            var detail = d.last_attack_detail ? ' \u2014 ' + esc(d.last_attack_detail) : '';
            meta += '<span class="bt-dev-atk-detail">' + esc(atkShort) + detail + '</span>';
        }

        var tCls = (d.transport || '').toLowerCase() === 'classic' ? 'classic' : 'ble';
        var transport = '<span class="bt-dev-transport ' + tCls + '">' + esc(d.transport || 'BLE') + '</span>';

        var rssiStr = d.rssi != null ? d.rssi + ' dBm' : '';

        var st = (d.attack_state || 'untouched').toLowerCase();
        var stLabel = d.attack_state || 'Idle';
        if (st === 'untouched') stLabel = 'Idle';
        // Build tooltip with attack detail
        var tooltip = '';
        if (d.last_attack) {
            var atkName = d.last_attack.replace(/([A-Z])/g, ' $1').trim();
            tooltip = atkName;
            if (d.last_attack_detail) tooltip += ': ' + d.last_attack_detail;
        }
        var titleAttr = tooltip ? ' title="' + esc(tooltip) + '"' : '';
        var state = '<span class="bt-dev-state st-' + st + '"' + titleAttr + '>' + esc(stLabel) + '</span>';

        var attacking = st === 'attacking';
        var pending = !!window._btManualPending;
        var isManualTarget = pending && window._btManualTarget === d.address;
        var lastRes = window._btLastResult;
        var hasResult = lastRes && lastRes.address === d.address;
        var dis = (attacking || pending || !rageMedium) ? ' disabled' : '';
        var disHigh = (attacking || pending || rage !== 'high') ? ' disabled' : '';
        var actions = '';
        if (isManualTarget) {
            var atkLabel = {knob:'KNOB',l2cap_fuzz:'Fuzzing',l2cap_conn_flood:'Flooding',att_gatt_fuzz:'Fuzzing',ble_adv_injection:'Cloning'}[window._btManualAttack] || window._btManualAttack;
            actions += '<span style="color:#f0c040;font-size:11px;animation:pulse-attack 1.5s infinite">' + esc(atkLabel) + '...</span>';
        } else if (hasResult) {
            var rc = lastRes.success ? '#00d4aa' : '#e94560';
            actions += '<span style="color:' + rc + ';font-size:11px">' + esc(lastRes.message) + '</span>';
        } else {
            if (d.transport === 'Classic' || d.transport === 'Dual') {
                actions += '<button class="bt-action-btn"' + dis + ' onclick="launchManualAttack(\'' + esc(d.address) + '\',\'knob\')">KNOB</button>';
                actions += '<button class="bt-action-btn"' + dis + ' onclick="launchManualAttack(\'' + esc(d.address) + '\',\'l2cap_fuzz\')">Fuzz</button>';
                actions += '<button class="bt-action-btn"' + disHigh + ' onclick="launchManualAttack(\'' + esc(d.address) + '\',\'l2cap_conn_flood\')">Flood</button>';
            }
            if (d.transport === 'Ble' || d.transport === 'Dual') {
                actions += '<button class="bt-action-btn"' + dis + ' onclick="launchManualAttack(\'' + esc(d.address) + '\',\'att_gatt_fuzz\')">Fuzz</button>';
                actions += '<button class="bt-action-btn"' + dis + ' onclick="launchManualAttack(\'' + esc(d.address) + '\',\'ble_adv_injection\')">Clone</button>';
            }
        }

        html += '<div class="bt-dev">' +
            bars +
            '<div class="bt-dev-info"><div class="bt-dev-name' + nameCls + '">' + nameStr + '</div>' +
            '<div class="bt-dev-meta">' + transport + meta + '</div></div>' +
            '<div class="bt-dev-rssi">' + rssiStr + '</div>' +
            state +
            '<div class="bt-dev-actions">' + actions + '</div>' +
            '</div>';
    });
    list.innerHTML = html;
}

function updateBtAttacksFromWs(btAttacks) {
    if (!btAttacks || !btAttacks.toggles) return;
    var t = btAttacks.toggles;
    var map = {
        smp_downgrade: t.smp_downgrade,
        knob: t.knob,
        l2cap_fuzz: t.l2cap_fuzz,
        l2cap_conn_flood: t.l2cap_conn_flood,
        att_gatt_fuzz: t.att_gatt_fuzz
    };
    Object.keys(map).forEach(function(key) {
        var el = document.getElementById('bt-atk-' + key);
        if (el && !el.matches(':focus')) el.checked = map[key];
    });
    window._btRageLevel = btAttacks.rage_level || 'Medium';
    updateBtAttackConstraintState(btAttacks.rage_level || 'Medium');
    updateBtPatchramConstraintState();
}

function updateBtAttackConstraintState(rageLevel) {
    var label = document.getElementById('bt-rage-label');
    if (label) label.textContent = rageLevel;
}

function updateBtPatchramConstraintState() {
    [{row: 'bt-row-knob', input: 'bt-atk-knob'}].forEach(function(entry) {
        var row = document.getElementById(entry.row);
        var input = document.getElementById(entry.input);
        if (!row || !input) return;
        var blocked = input.checked && !isPatchramReady();
        row.classList.toggle('bt-row-disabled', blocked);
        if (blocked) {
            row.title = 'Requires patchram attack firmware';
        } else if (!row.classList.contains('bt-row-warning')) {
            row.title = '';
        }
    });
    // Also disable vendor diagnostics button if no patchram
    var btn = document.getElementById('btn-vendor-diag');
    if (btn) btn.disabled = !isPatchramReady();
}

function refreshBtDevices() {
    api('GET', '/api/bt/devices').then(function(d) {
        if (d) updateBtDevicesFromWs(d);
    });
}

function refreshBtAttacks() {
    api('GET', '/api/bt/attacks').then(function(d) {
        if (d) updateBtAttacksFromWs(d);
    });
}

function refreshBtPatchram() {
    api('GET', '/api/bt/patchram').then(function(d) {
        if (d) {
            window._btPatchramReady = d.state === 'Applied' || d.state === 'Ready';
            updateBtPatchramConstraintState();
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
    var body = {name: name};
    body.display_invert = document.getElementById('setting-invert').checked;
    body.display_rotation = parseInt(document.getElementById('setting-rotation').value) || 0;
    body.min_rssi = parseInt(document.getElementById('setting-rssi').value) || -100;
    body.ap_ttl_secs = parseInt(document.getElementById('setting-ttl').value) || 120;
    body.display_refresh_interval = parseInt(document.getElementById('setting-refresh').value) || 10;
    api('POST', '/api/settings', body).then(function(r) {
        if (r && r.ok) toast('Settings saved');
    });
}

// --- WebSocket live updates with polling fallback ---

var _ws = null;
var _pollTimers = [];
var _wsConnected = false;

function updateStatusFromWs(d) {
    mergeOverviewState(d);
    syncModeUi(d.mode);
    updateOverview(_overviewState);
    var nameInput = document.getElementById('setting-name');
    if (nameInput && !nameInput.matches(':focus')) nameInput.value = d.name || '';
}

function syncSettingsFromData(d) {
    if (d.display_invert != null) {
        var inv = document.getElementById('setting-invert');
        if (inv && !inv.matches(':focus')) inv.checked = d.display_invert;
    }
    if (d.display_rotation != null) {
        var rot = document.getElementById('setting-rotation');
        if (rot && !rot.matches(':focus')) rot.value = String(d.display_rotation);
    }
    if (d.min_rssi != null) {
        var rs = document.getElementById('setting-rssi');
        if (rs && !rs.matches(':active')) { rs.value = d.min_rssi; document.getElementById('setting-rssi-val').textContent = d.min_rssi; }
    }
    if (d.ap_ttl_secs != null) {
        var ttl = document.getElementById('setting-ttl');
        if (ttl && !ttl.matches(':active')) { ttl.value = d.ap_ttl_secs; document.getElementById('setting-ttl-val').textContent = d.ap_ttl_secs; }
    }
    if (d.display_refresh_interval != null) {
        var ri = document.getElementById('setting-refresh');
        if (ri && !ri.matches(':active')) { ri.value = d.display_refresh_interval; document.getElementById('setting-refresh-val').textContent = d.display_refresh_interval; }
    }
}

function updateBatteryFromWs(b) {
    if (b.available) {
        document.getElementById('bat-level').textContent = b.level + '%';
        document.getElementById('bat-level').style.color = b.critical ? '#e94560' : (b.low ? '#f0c040' : '#00d4aa');
        document.getElementById('bat-state').textContent = b.charging ? 'Charging' : 'Discharging';
        document.getElementById('bat-voltage').textContent = (b.voltage_mv / 1000).toFixed(2) + 'V';
        document.getElementById('bat-bar').style.width = b.level + '%';
        document.getElementById('bat-bar').style.background = b.critical ? '#e94560' : (b.low ? '#f0c040' : '#00d4aa');
    } else {
        document.getElementById('bat-level').textContent = 'N/A';
        document.getElementById('bat-state').textContent = 'Not detected';
        document.getElementById('bat-voltage').textContent = '-';
    }
}

function updateBluetoothFromWs(d) {
    mergeOverviewState({bluetooth: d});
    updateOverview(_overviewState);
    document.getElementById('bt-status').textContent = d.connected ? 'Connected' : d.state;
    document.getElementById('bt-status').style.color = d.connected ? '#00d4aa' : '#888';
    document.getElementById('bt-device').textContent = d.device_name || '-';
    document.getElementById('bt-ip').textContent = d.ip || '-';
    document.getElementById('bt-internet').textContent = d.internet_available ? 'Yes' : 'No';
    document.getElementById('bt-internet').style.color = d.internet_available ? '#00d4aa' : '#888';
    document.getElementById('bt-retries').textContent = d.retry_count;
    document.getElementById('bt-feature-mode').textContent = d.feature_mode || '-';
    document.getElementById('bt-nearby').textContent = d.nearby_devices != null ? d.nearby_devices : '-';
    document.getElementById('bt-contention').textContent = d.contention_score != null ? d.contention_score : '-';
    // disconnect button visibility
    var dBtn = document.getElementById('bt-disconnect-btn');
    if (dBtn) { dBtn.style.display = d.connected ? 'inline-block' : 'none'; }
    // passkey display
    if (d.passkey != null && d.passkey > 0) {
        document.getElementById('bt-passkey-code').textContent = String(d.passkey).padStart(6, '0');
        document.getElementById('bt-passkey-area').style.display = 'block';
    } else {
        var pa = document.getElementById('bt-passkey-area');
        if (pa) pa.style.display = 'none';
    }
    // live pairing status update
    var ps = document.getElementById('bt-pair-status');
    if (ps) {
        if (d.pair_in_progress) {
            if (d.passkey) {
                ps.innerHTML = '&#128273; Passkey: <b>' + String(d.passkey).padStart(6,'0') + '</b> — confirm on phone';
                ps.style.color = '#00d4aa';
            }
        } else if (d.connected) {
            ps.innerHTML = '&#10004; Connected!';
            ps.style.color = '#00d4aa';
        } else if (d.state === 'Error') {
            ps.innerHTML = '&#10006; Pairing failed';
            ps.style.color = '#e94560';
        }
    }
}

function updateWifiFromWs(d) {
    document.getElementById('wifi-state').textContent = d.state;
    document.getElementById('wifi-state').style.color = d.state === 'Monitor' ? '#00d4aa' : '#e94560';
    document.getElementById('wifi-ch').textContent = d.channel;
    document.getElementById('wifi-aps').textContent = d.aps_tracked;
    document.getElementById('wifi-channels').textContent = d.channels.join(', ') || '-';
    document.getElementById('wifi-dwell').textContent = d.dwell_ms + 'ms';
    if (Date.now() < _chConfigCooldown) return;
    if (!d.autohunt_enabled) {
        document.getElementById('ch-list').value = d.channels.join(',');
        _savedChannels = d.channels.slice();
    }
    renderChannelButtons(d.autohunt_enabled ? [] : d.channels);
    var dwInput = document.getElementById('ch-dwell');
    if (dwInput && !dwInput.matches(':active')) { dwInput.value = d.dwell_ms; document.getElementById('ch-dwell-val').textContent = d.dwell_ms; }
    var ahToggle = document.getElementById('autohunt-toggle');
    if (ahToggle) ahToggle.checked = d.autohunt_enabled;
    var scToggle = document.getElementById('skip-captured-toggle');
    if (scToggle) scToggle.checked = d.skip_captured;
    if (d.rage_level != null) {
        updateRageLabel(d.rage_level, true);
    } else {
        updateRageLabel(0, false);
    }
}

function updateAttacksFromWs(d) {
    mergeOverviewState({attacks: d});
    updateOverview(_overviewState);
    ['deauth','pmkid','csa','disassoc','anon_reassoc','rogue_m2'].forEach(function(k) {
        var cb = document.getElementById('atk-'+k);
        if (cb) cb.checked = d[k];
    });
    [1,2,3].forEach(function(n) {
        document.getElementById('rate-'+n).classList.toggle('active', n === d.attack_rate);
    });
}

function updateCapturesFromWs(d) {
    document.getElementById('cap-total').textContent = d.total_files;
    document.getElementById('cap-hs').textContent = d.handshake_files;
    document.getElementById('cap-pending').textContent = d.pending_upload;
    document.getElementById('cap-size').textContent = fmtBytes(d.total_size_bytes);
    syncCaptureModeUi(d.capture_all);
    _capFiles = d.files || [];
    renderCapList();
}

function updateRecoveryFromWs(rec, h) {
    document.getElementById('rec-state').textContent = rec.state;
    document.getElementById('rec-state').style.color = rec.state === 'Healthy' ? '#00d4aa' : '#f0c040';
    document.getElementById('rec-total').textContent = rec.total_recoveries;
    document.getElementById('rec-last').textContent = rec.last_recovery;
    document.getElementById('rec-crashes').textContent = h.ao_crash_count;
    document.getElementById('rec-crashes').style.color = h.ao_crash_count > 0 ? '#f0c040' : '#e0e0e0';
    document.getElementById('rec-pid').textContent = h.ao_pid || '-';
    document.getElementById('rec-ao-up').textContent = h.ao_uptime;
    // Firmware health from recovery payload
    document.getElementById('fw-health').textContent = rec.fw_health || '-';
    var fwColor = rec.fw_health === 'Healthy' ? '#00d4aa' : rec.fw_health === 'Degraded' ? '#f0c040' : rec.fw_health === 'Critical' ? '#e74c3c' : '#888';
    document.getElementById('fw-health').style.color = fwColor;
    document.getElementById('fw-crash').textContent = rec.fw_crash_suppress != null ? rec.fw_crash_suppress : '-';
    document.getElementById('fw-fault').textContent = rec.fw_hardfault != null ? rec.fw_hardfault : '-';
    var fdot = document.getElementById('h-firmware');
    fdot.className = 'dot ' + (rec.fw_health === 'Healthy' ? 'dot-green' : rec.fw_health === 'Degraded' ? 'dot-yellow' : rec.fw_health === 'Critical' ? 'dot-red' : 'dot-gray');
    var wdot = document.getElementById('h-wifi');
    wdot.className = 'dot ' + (h.wifi_state === 'Monitor' ? 'dot-green' : 'dot-red');
    var adot = document.getElementById('h-ao');
    adot.className = 'dot ' + (h.ao_state === 'RUNNING' ? 'dot-green' : 'dot-red');
    var rdot = document.getElementById('h-recovery');
    rdot.className = 'dot ' + (h.ao_crash_count === 0 ? 'dot-green' : 'dot-yellow');
    var gdot = document.getElementById('h-gps');
    gdot.className = 'dot ' + (h.gpsd_available ? 'dot-green' : 'dot-gray');
    var gpsEl = document.getElementById('sys-gps');
    gpsEl.textContent = h.gpsd_available ? 'Connected' : 'N/A';
    gpsEl.style.color = h.gpsd_available ? '#00d4aa' : '#888';
    document.getElementById('sys-uptime').textContent = fmtUptime(h.uptime_secs);
}

function updatePersonalityFromWs(d) {
    document.getElementById('p-mood').textContent = Math.round(d.mood * 100) + '%';
    document.getElementById('p-face').textContent = d.face;
    document.getElementById('p-xp').textContent = d.xp;
    document.getElementById('p-level').textContent = d.level;
    document.getElementById('p-blind').textContent = d.blind_epochs;
    document.getElementById('mood-bar').style.width = Math.round(d.mood * 100) + '%';
    var moodColor = d.mood > 0.7 ? '#00d4aa' : (d.mood > 0.3 ? '#f0c040' : '#e94560');
    document.getElementById('mood-bar').style.background = moodColor;
}

function updateSystemFromWs(d) {
    document.getElementById('sys-temp').textContent = d.cpu_temp_c > 0 ? d.cpu_temp_c.toFixed(1) + '\u00B0C' : '-';
    document.getElementById('sys-temp').style.color = d.cpu_temp_c > 70 ? '#e94560' : (d.cpu_temp_c > 55 ? '#f0c040' : '#00d4aa');
    document.getElementById('sys-cpu').textContent = d.cpu_percent > 0 ? d.cpu_percent.toFixed(0) + '%' : '-';
    document.getElementById('sys-mem').textContent = d.mem_total_mb > 0 ? d.mem_used_mb + '/' + d.mem_total_mb + ' MB' : '-';
    document.getElementById('sys-disk').textContent = d.disk_total_mb > 0 ? d.disk_used_mb + '/' + d.disk_total_mb + ' MB' : '-';
}

function updateCrackedFromWs(list) {
    var el = document.getElementById('cracked-list');
    if (!list || !list.length) {
        el.innerHTML = '<div style="color:#555;font-size:12px">No cracked passwords yet</div>';
        return;
    }
    el.innerHTML = list.map(function(c) {
        var label = esc(c.ssid || c.bssid);
        if (c.ssid && c.bssid) label += ' \u00b7 ' + esc(c.bssid);
        if (c.date) label += ' \u00b7 ' + esc(c.date);
        return '<div style="padding:4px 0;border-bottom:1px solid #0f346022">' +
            '<span style="color:#00d4aa;font-weight:bold;font-size:11px">' + label + '</span>' +
            '<br><span style="color:#f0c040;font-family:monospace;font-size:12px">' + esc(c.password) + '</span></div>';
    }).join('');
}

function updateApsFromWs(aps) {
    var el = document.getElementById('ap-tbody');
    if (!aps || !aps.length) {
        el.innerHTML = '<tr><td colspan="6" style="color:#555">No APs detected</td></tr>';
        return;
    }
    aps.sort(function(a,b){ return b.rssi - a.rssi; });
    el.innerHTML = aps.map(function(ap) {
        var rssiColor = ap.rssi > -50 ? '#00d4aa' : (ap.rssi > -70 ? '#f0c040' : '#e94560');
        var hsIcon = ap.has_handshake ? '<span style="color:#00d4aa" title="Handshake or PMKID captured — hash saved, ready to crack">&#9733;</span>' : '';
        return '<tr><td>' + esc(ap.ssid || '<hidden>') + '</td>' +
            '<td style="color:#888;font-size:10px">' + esc(ap.bssid) + '</td>' +
            '<td style="color:' + rssiColor + '">' + ap.rssi + '</td>' +
            '<td>' + ap.channel + '</td>' +
            '<td>' + ap.clients + '</td>' +
            '<td>' + hsIcon + '</td></tr>';
    }).join('');
}

function updateWhitelistFromWs(entries) {
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
}

function updatePluginsFromWs(plugins) {
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
}

function updateBtOpsFromWs(btAttacks, btPatchram) {
    if (!btAttacks) return;
    mergeOverviewState({bt_attacks: btAttacks});
    updateOverview(_overviewState);
    document.getElementById('bt-ops-engine').textContent = btAttacks.enabled ? 'Active' : 'Disabled';
    document.getElementById('bt-ops-engine').style.color = btAttacks.enabled ? '#00d4aa' : '#888';
    document.getElementById('bt-ops-rage').textContent = btAttacks.rage_level || '-';
    var st = btAttacks.stats || {};
    document.getElementById('bt-ops-devices').textContent = st.devices_seen != null ? st.devices_seen : '-';
    document.getElementById('bt-ops-active').textContent = st.active_attacks != null ? st.active_attacks : '-';
    document.getElementById('bt-ops-total').textContent = st.total_attacks != null ? st.total_attacks : '-';
    if (btPatchram && btPatchram.state != null) {
        _btPatchramState = btPatchram.state;
        document.getElementById('bt-ops-patchram').textContent = btPatchram.state;
    }
    updateBtPatchramConstraintState();
}

function isPatchramReady() {
    return _btPatchramState === 'attack';
}

function updateBtPatchramConstraintState() {
    [
        {row: 'bt-row-knob', input: 'bt-atk-knob'}
    ].forEach(function(entry) {
        var row = document.getElementById(entry.row);
        var input = document.getElementById(entry.input);
        if (!row || !input) return;
        var blocked = input.checked && !isPatchramReady();
        row.classList.toggle('bt-row-disabled', blocked);
        if (blocked) {
            row.title = 'Requires patchram attack firmware';
        } else if (!row.classList.contains('bt-row-warning')) {
            row.title = '';
        }
    });
}

function isValidBtAddress(address) {
    return /^[0-9A-F]{2}(?::[0-9A-F]{2}){5}$/i.test(address || '');
}

// updateBtDevicesFromWs is defined once in the Hunting section above

function setBtTarget(address) {
    if (!isValidBtAddress(address)) {
        toast('Invalid BT address');
        return;
    }
    toast('Target queued: ' + address);
    api('POST', '/api/bt/attacks/target', {address: address});
}

var _btRageLevel = 'Medium';
var _btRageDescs = {
    'Low': 'Passive diagnostics only — targets own controller (VendorCmdUnlock)',
    'Medium': 'Active attacks targeting external devices',
    'High': 'Aggressive — includes MITM and connection hijack'
};

function setBtRage(level) {
    _btRageLevel = level;
    document.getElementById('bt-rage-low').classList.toggle('active', level === 'Low');
    document.getElementById('bt-rage-medium').classList.toggle('active', level === 'Medium');
    document.getElementById('bt-rage-high').classList.toggle('active', level === 'High');
    document.getElementById('bt-rage-desc').textContent = (level + ': ' + (_btRageDescs[level] || ''));
    updateBtAttackConstraintState(level);
    api('POST', '/api/bt/attacks/rage', {level: level});
}

function updateBtAttackConstraintState(level) {
    var isLow = (level === 'Low');
    var isNotHigh = (level !== 'High');
    // Medium+ attacks: disabled at Low rage
    ['smp_downgrade','knob','l2cap_fuzz','att_gatt_fuzz'].forEach(function(key) {
        var row = document.getElementById('bt-row-' + key);
        var input = document.getElementById('bt-atk-' + key);
        if (row) row.classList.toggle('bt-row-disabled', isLow);
        if (input) input.disabled = isLow;
    });
    // High-only attacks: disabled at Low and Medium
    ['l2cap_conn_flood'].forEach(function(key) {
        var row = document.getElementById('bt-row-' + key);
        var input = document.getElementById('bt-atk-' + key);
        if (row) row.classList.toggle('bt-row-disabled', isNotHigh);
        if (input) input.disabled = isNotHigh;
    });
}

function updateBtRageFromWs(btAttacks) {
    if (!btAttacks) return;
    var level = btAttacks.rage_level || 'Medium';
    _btRageLevel = level;
    document.getElementById('bt-rage-low').classList.toggle('active', level === 'Low');
    document.getElementById('bt-rage-medium').classList.toggle('active', level === 'Medium');
    document.getElementById('bt-rage-high').classList.toggle('active', level === 'High');
    document.getElementById('bt-rage-desc').textContent = (level + ': ' + (_btRageDescs[level] || ''));
    updateBtAttackConstraintState(level);
}

function toggleBtAttack(name, enabled) {
    updateBtAttackConstraintState(_btRageLevel);
    api('POST', '/api/bt/attacks/toggle', {attack: name, enabled: enabled});
}

var _btScanMode = 'both';

function setBtScanMode(mode) {
    _btScanMode = mode;
    document.getElementById('scan-mode-ble').classList.toggle('active', mode === 'ble' || mode === 'both');
    document.getElementById('scan-mode-classic').classList.toggle('active', mode === 'classic' || mode === 'both');
    document.getElementById('scan-mode-both').classList.toggle('active', mode === 'both');

    // Show/hide attack groups based on scan mode
    document.querySelectorAll('.bt-scan-ble').forEach(function(el) {
        el.style.display = (mode === 'ble' || mode === 'both') ? '' : 'none';
    });
    document.querySelectorAll('.bt-scan-classic').forEach(function(el) {
        el.style.display = (mode === 'classic' || mode === 'both') ? '' : 'none';
    });

    api('POST', '/api/bt/scan-mode', {mode: mode});
}

function updateBtScanModeFromWs(scanMode) {
    if (!scanMode) return;
    _btScanMode = scanMode;
    document.getElementById('scan-mode-ble').classList.toggle('active', scanMode === 'ble' || scanMode === 'both');
    document.getElementById('scan-mode-classic').classList.toggle('active', scanMode === 'classic' || scanMode === 'both');
    document.getElementById('scan-mode-both').classList.toggle('active', scanMode === 'both');
    document.querySelectorAll('.bt-scan-ble').forEach(function(el) {
        el.style.display = (scanMode === 'ble' || scanMode === 'both') ? '' : 'none';
    });
    document.querySelectorAll('.bt-scan-classic').forEach(function(el) {
        el.style.display = (scanMode === 'classic' || scanMode === 'both') ? '' : 'none';
    });
}

function updateBtAttacksFromWs(btAttacks) {
    if (!btAttacks || !btAttacks.toggles) return;
    var t = btAttacks.toggles;
    var map = {
        smp_downgrade: t.smp_downgrade,
        knob: t.knob,
        l2cap_fuzz: t.l2cap_fuzz,
        l2cap_conn_flood: t.l2cap_conn_flood,
        att_gatt_fuzz: t.att_gatt_fuzz
    };
    Object.keys(map).forEach(function(key) {
        var el = document.getElementById('bt-atk-' + key);
        if (el && !el.matches(':focus')) el.checked = map[key];
    });
    updateBtAttackConstraintState(btAttacks.rage_level || 'Medium');
    updateBtPatchramConstraintState();
    if (btAttacks.scan_mode) updateBtScanModeFromWs(btAttacks.scan_mode);
}

function updateBtCapturesFromWs(btCaptures) {
    if (!btCaptures) return;
    document.getElementById('bt-cap-keys').textContent = btCaptures.keys != null ? btCaptures.keys : '-';
    document.getElementById('bt-cap-crashes').textContent = btCaptures.crashes != null ? btCaptures.crashes : '-';
    document.getElementById('bt-cap-vendor').textContent = btCaptures.vendor != null ? btCaptures.vendor : '-';
    document.getElementById('bt-cap-total').textContent = btCaptures.total != null ? btCaptures.total : '-';
    document.getElementById('bt-cap-transcripts').textContent = btCaptures.transcripts != null ? btCaptures.transcripts : '-';
}

function refreshBtAttacks() {
    if (_currentMode !== 'bt') return;
    api('GET', '/api/bt/attacks').then(function(d) {
        if (!d) return;
        updateBtOpsFromWs(d);
        updateBtRageFromWs(d);
        updateBtAttacksFromWs(d);
    });
    api('GET', '/api/bt/devices').then(function(d) {
        if (!d) return;
        updateBtDevicesFromWs(d);
    });
    api('GET', '/api/bt/captures').then(function(d) {
        if (!d) return;
        updateBtCapturesFromWs(d);
    });
    api('GET', '/api/bt/patchram').then(function(d) {
        if (!d) return;
        _btPatchramState = d.state || '';
        document.getElementById('bt-ops-patchram').textContent = d.state || '-';
        updateBtPatchramConstraintState();
    });
}

function updateAllCards(state) {
    if (state.epoch !== undefined) updateStatusFromWs(state);
    if (state.battery) updateBatteryFromWs(state.battery);
    if (state.bluetooth) updateBluetoothFromWs(state.bluetooth);
    if (state.wifi) updateWifiFromWs(state.wifi);
    if (state.attacks) updateAttacksFromWs(state.attacks);
    if (state.qpu) updateRfFromWs(state.qpu);
    if (state.captures) updateCapturesFromWs(state.captures);
    if (state.recovery && state.health) updateRecoveryFromWs(state.recovery, state.health);
    if (state.personality) updatePersonalityFromWs(state.personality);
    if (state.system) updateSystemFromWs(state.system);
    if (state.cracked) updateCrackedFromWs(state.cracked);
    if (state.aps) updateApsFromWs(state.aps);
    if (state.whitelist) updateWhitelistFromWs(state.whitelist);
    if (state.plugins) updatePluginsFromWs(state.plugins);
    if (state.bt_attacks) updateBtOpsFromWs(state.bt_attacks, state.bt_patchram);
    if (state.bt_devices) updateBtDevicesFromWs(state.bt_devices);
    if (state.bt_attacks) updateBtRageFromWs(state.bt_attacks);
    if (state.bt_attacks) updateBtAttacksFromWs(state.bt_attacks);
    if (state.bt_captures) updateBtCapturesFromWs(state.bt_captures);
    if (state.bt_scan_results && state.bt_scan_results.length > 0) renderBtDeviceList(state.bt_scan_results);
}

function startPolling() {
    if (_pollTimers.length > 0) return; // already polling
    _pollTimers.push(setInterval(refreshStatus, 5000));
    _pollTimers.push(setInterval(refreshBattery, 15000));
    _pollTimers.push(setInterval(refreshBluetooth, 15000));
    _pollTimers.push(setInterval(refreshWifi, 5000));
    _pollTimers.push(setInterval(refreshAttacks, 10000));
    _pollTimers.push(setInterval(refreshRf, 10000));
    _pollTimers.push(setInterval(refreshCaptures, 30000));
    _pollTimers.push(setInterval(refreshRecovery, 15000));
    _pollTimers.push(setInterval(refreshPersonality, 10000));
    _pollTimers.push(setInterval(refreshSystem, 15000));
    _pollTimers.push(setInterval(refreshCracked, 60000));
    _pollTimers.push(setInterval(refreshPlugins, 15000));
    _pollTimers.push(setInterval(refreshAps, 10000));
    _pollTimers.push(setInterval(refreshWhitelist, 30000));
    _pollTimers.push(setInterval(refreshLogs, 10000));
    _pollTimers.push(setInterval(refreshWpaSec, 30000));
    _pollTimers.push(setInterval(refreshDiscord, 30000));
    _pollTimers.push(setInterval(refreshBtDevices, 10000));
    _pollTimers.push(setInterval(refreshBtAttacks, 15000));
    _pollTimers.push(setInterval(refreshBtPatchram, 30000));
}

function stopPolling() {
    _pollTimers.forEach(function(t) { clearInterval(t); });
    _pollTimers = [];
}

function connectWebSocket() {
    var proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    _ws = new WebSocket(proto + '//' + location.host + '/ws');

    _ws.onopen = function() {
        _wsConnected = true;
        stopPolling();
    };

    _ws.onmessage = function(event) {
        try {
            var state = JSON.parse(event.data);
            updateAllCards(state);
            if (state.bt_manual_result) {
                var r = state.bt_manual_result;
                window._btLastResult = r;
                if (window._btManualPending) {
                    var icon = r.success ? '\u2713' : '\u2717';
                    var target = r.address || 'local';
                    toast(icon + ' ' + r.attack + ' on ' + target + ': ' + r.message);
                }
                window._btManualPending = false;
                window._btManualTarget = null;
                window._btManualAttack = null;
            } else {
                window._btLastResult = null;
            }
        } catch(e) {
            console.error('WS parse error:', e);
        }
    };

    _ws.onclose = function() {
        _wsConnected = false;
        // Fallback to polling, retry WS after 3s
        startPolling();
        setTimeout(connectWebSocket, 3000);
    };

    _ws.onerror = function() {
        // onclose will fire after onerror
    };
}

// --- Initial load ---
renderChannelButtons([1, 6, 11]); // default until refreshWifi populates
// Do one immediate fetch of all cards to populate before WS connects
refreshStatus();
setTimeout(refreshBattery, 500);
setTimeout(refreshBluetooth, 1000);
setTimeout(refreshWifi, 1500);
setTimeout(refreshAttacks, 2500);
setTimeout(refreshRf, 2800);
setTimeout(refreshCaptures, 3000);
setTimeout(refreshRecovery, 3500);
setTimeout(refreshPersonality, 4000);
setTimeout(refreshSystem, 4500);
setTimeout(refreshCracked, 5000);
setTimeout(refreshPlugins, 5500);
setTimeout(refreshAps, 6000);
setTimeout(refreshWhitelist, 6500);
setTimeout(refreshWpaSec, 7000);
setTimeout(refreshDiscord, 7500);
setTimeout(refreshBtDevices, 8000);
setTimeout(refreshBtAttacks, 8500);
setTimeout(refreshBtPatchram, 9000);

// Start polling as initial strategy; WS will take over once connected
startPolling();
// Connect WebSocket for live updates
connectWebSocket();
refreshStatus();
// Display image stays on its own interval (binary, not suitable for WS)
setInterval(function(){ document.getElementById('eink-img').src='/api/display.png?t='+Date.now(); }, 5000);
var _interactCooldownTimer = null;
function interact(action) {
  var allBtns = ['btn-pet', 'btn-treat', 'btn-praise'];
  allBtns.forEach(function(id) {
    var b = document.getElementById(id);
    if (b) { b.disabled = true; b.classList.add('on-cooldown'); }
  });
  api('POST', '/api/interact', { action: action }).then(function(d) {
    if (!d) { enableAllInteract(); return; }
    var resp = document.getElementById('interact-response');
    if (resp) { resp.textContent = d.message || ''; resp.style.opacity = '1'; resp.style.color = d.ok ? '#00d4aa' : '#e94560'; }
    if (d.ok) { refreshPersonality(); startInteractCooldown(d.cooldown_secs); }
    else { enableAllInteract(); }
  });
}
function enableAllInteract() {
  ['pet', 'treat', 'praise'].forEach(function(action) {
    var b = document.getElementById('btn-' + action);
    if (b) { b.disabled = false; b.classList.remove('on-cooldown'); b.textContent = action.charAt(0).toUpperCase() + action.slice(1); }
  });
}
function startInteractCooldown(secs) {
  if (secs <= 0) { enableAllInteract(); return; }
  if (_interactCooldownTimer) clearInterval(_interactCooldownTimer);
  var end = Date.now() + secs * 1000;
  var allBtns = ['btn-pet', 'btn-treat', 'btn-praise'];
  _interactCooldownTimer = setInterval(function() {
    var left = Math.max(0, Math.round((end - Date.now()) / 1000));
    if (left <= 0) {
      clearInterval(_interactCooldownTimer); _interactCooldownTimer = null;
      enableAllInteract();
      var resp = document.getElementById('interact-response');
      if (resp) { resp.style.opacity = '0'; }
      return;
    }
    allBtns.forEach(function(id) {
      var b = document.getElementById(id);
      if (b) { b.disabled = true; b.classList.add('on-cooldown'); }
    });
    var m = Math.floor(left / 60), s = left % 60;
    var timeStr = m + ':' + (s < 10 ? '0' : '') + s;
    allBtns.forEach(function(id) {
      var b = document.getElementById(id);
      if (b) {
        var label = id.replace('btn-', '');
        b.textContent = label.charAt(0).toUpperCase() + label.slice(1) + ' ' + timeStr;
      }
    });
  }, 1000);
}
function loadInteractCooldowns() {
  api('GET', '/api/interact').then(function(d) {
    if (!d) return;
    var maxSecs = Math.max(d.pet || 0, d.treat || 0, d.praise || 0);
    if (maxSecs > 0) startInteractCooldown(maxSecs);
  });
}
loadInteractCooldowns();
</script>
</body>
</html>"##;
