#!/bin/bash
# apply-oxigotchi-patches.sh — Reapply oxigotchi core file patches after pwnagotchi updates.
#
# Can be run manually:   sudo /usr/local/bin/apply-oxigotchi-patches.sh
# Or triggered by:       oxigotchi-patches.path systemd unit on package changes
#
# Idempotent: checks each patch before applying. Safe to run repeatedly.

set -euo pipefail

LOG_TAG="oxigotchi-patches"
SITE_PKG="/home/pi/.pwn/lib/python3.13/site-packages/pwnagotchi"
PATCHED=0
SKIPPED=0
FAILED=0

log() { logger -t "$LOG_TAG" "$1"; echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1"; }
die() { log "FATAL: $1"; exit 1; }

# ─── Patch 1: pwnlib — comment out reload_brcm in stop_monitor_interface ───
patch_pwnlib() {
    local f="/usr/bin/pwnlib"
    [ -f "$f" ] || { log "SKIP pwnlib: $f not found"; SKIPPED=$((SKIPPED+1)); return; }

    if grep -q '#.*reload_brcm.*disabled.*SDIO' "$f" 2>/dev/null; then
        log "OK   pwnlib: already patched"
        SKIPPED=$((SKIPPED+1))
    else
        # Comment out bare reload_brcm calls inside stop_monitor_interface
        if sed -i '/stop_monitor_interface/,/^}$/ s/^\([[:space:]]*\)reload_brcm\b/\1#reload_brcm  # disabled: causes SDIO crash (oxigotchi)/' "$f"; then
            log "DONE pwnlib: commented out reload_brcm in stop_monitor_interface"
            PATCHED=$((PATCHED+1))
        else
            log "FAIL pwnlib: sed failed"
            FAILED=$((FAILED+1))
        fi
    fi
}

# ─── Patch 2: cache.py — isinstance check for AO handshakes ───
patch_cache() {
    local f="$SITE_PKG/plugins/default/cache.py"
    [ -f "$f" ] || { log "SKIP cache.py: $f not found"; SKIPPED=$((SKIPPED+1)); return; }

    if grep -q 'isinstance(access_point, dict)' "$f" 2>/dev/null; then
        log "OK   cache.py: already patched"
        SKIPPED=$((SKIPPED+1))
    else
        if sed -i 's/if self\.ready:/if self.ready and isinstance(access_point, dict):/' "$f"; then
            log "DONE cache.py: added isinstance check for AO handshakes"
            PATCHED=$((PATCHED+1))
        else
            log "FAIL cache.py: sed failed"
            FAILED=$((FAILED+1))
        fi
    fi
}

# ─── Patch 3: handler.py — CSRF exemption for plugin webhooks ───
patch_handler() {
    local f="$SITE_PKG/ui/web/handler.py"
    [ -f "$f" ] || { log "SKIP handler.py: $f not found"; SKIPPED=$((SKIPPED+1)); return; }

    if grep -q 'csrf\.exempt' "$f" 2>/dev/null; then
        log "OK   handler.py: already patched"
        SKIPPED=$((SKIPPED+1))
    else
        # After the line that sets plugins_with_auth, add CSRF exemption
        if sed -i '/plugins_with_auth = self\.with_auth(self\.plugins)/a\        # Exempt plugin webhooks from CSRF (plugins handle their own auth) [oxigotchi]\n        if hasattr(self._app, '"'"'csrf'"'"'):\n            plugins_with_auth = self._app.csrf.exempt(plugins_with_auth)' "$f"; then
            log "DONE handler.py: added CSRF exemption for plugin webhooks"
            PATCHED=$((PATCHED+1))
        else
            log "FAIL handler.py: sed failed"
            FAILED=$((FAILED+1))
        fi
    fi
}

# ─── Patch 4: server.py — store csrf instance on app ───
patch_server() {
    local f="$SITE_PKG/ui/web/server.py"
    [ -f "$f" ] || { log "SKIP server.py: $f not found"; SKIPPED=$((SKIPPED+1)); return; }

    if grep -q 'app\.csrf = csrf' "$f" 2>/dev/null; then
        log "OK   server.py: already patched"
        SKIPPED=$((SKIPPED+1))
    else
        # Replace CSRFProtect(app) with csrf = CSRFProtect(app); app.csrf = csrf
        if sed -i 's/CSRFProtect(app)/csrf = CSRFProtect(app)\n            app.csrf = csrf/' "$f"; then
            log "DONE server.py: stored csrf instance on app"
            PATCHED=$((PATCHED+1))
        else
            log "FAIL server.py: sed failed"
            FAILED=$((FAILED+1))
        fi
    fi
}

# ─── Patch 5: log.py — session cache for LastSession.parse() ───
patch_log() {
    local f="$SITE_PKG/log.py"
    [ -f "$f" ] || { log "SKIP log.py: $f not found"; SKIPPED=$((SKIPPED+1)); return; }

    if grep -q '_CACHE_FILE' "$f" 2>/dev/null; then
        log "OK   log.py: already patched"
        SKIPPED=$((SKIPPED+1))
    else
        # This patch is too large for sed. Use python to apply it.
        python3 - "$f" <<'PYEOF'
import sys

f = sys.argv[1]
with open(f) as fh:
    code = fh.read()

# Bail if already patched
if '_CACHE_FILE' in code:
    sys.exit(0)

# Add 'import json' if not present
if 'import json' not in code:
    code = code.replace('import hashlib', 'import hashlib\nimport json', 1)

# Insert _CACHE_FILE, _save_cache, _load_cache before the parse method
cache_block = '''
    _CACHE_FILE = '/home/pi/last_session_cache.json'

    def _save_cache(self):
        """Save parsed session data to cache file for fast boot."""
        try:
            import os as _os
            stat = _os.stat(self.path) if _os.path.exists(self.path) else None
            peer_data = None
            if self.last_peer:
                try:
                    peer_data = {
                        'session_id': self.last_peer.session_id(),
                        'channel': 1,
                        'rssi': self.last_peer.rssi,
                        'identity': self.last_peer.identity(),
                        'name': self.last_peer.name(),
                        'pwnd_tot': self.last_peer.pwnd_total(),
                    }
                except Exception:
                    pass
            data = {
                'version': 1,
                'log_mtime': stat.st_mtime if stat else 0,
                'log_size': stat.st_size if stat else 0,
                'last_session_id': self.last_session_id,
                'duration': self.duration,
                'duration_human': self.duration_human,
                'deauthed': self.deauthed,
                'associated': self.associated,
                'handshakes': self.handshakes,
                'epochs': self.epochs,
                'train_epochs': self.train_epochs,
                'peers': self.peers,
                'last_peer': peer_data,
                'min_reward': self.min_reward,
                'max_reward': self.max_reward,
                'avg_reward': self.avg_reward,
            }
            with open(self._CACHE_FILE, 'w') as f:
                json.dump(data, f)
        except Exception as e:
            logging.debug("could not save session cache: %s" % e)

    def _load_cache(self):
        """Load cached session data if cache is valid (log hasn't changed)."""
        try:
            import os as _os
            if not _os.path.isfile(self._CACHE_FILE):
                return False
            stat = _os.stat(self.path) if _os.path.exists(self.path) else None
            if not stat:
                return False
            with open(self._CACHE_FILE, 'r') as f:
                data = json.load(f)
            if data.get('version') != 1:
                return False
            if data.get('log_mtime') != stat.st_mtime or data.get('log_size') != stat.st_size:
                return False
            from pwnagotchi.mesh.peer import Peer as _Peer
            self.last_session_id = data.get('last_session_id', '')
            self.duration = data.get('duration', '')
            self.duration_human = data.get('duration_human', '')
            self.deauthed = data.get('deauthed', 0)
            self.associated = data.get('associated', 0)
            self.handshakes = data.get('handshakes', 0)
            self.epochs = data.get('epochs', 0)
            self.train_epochs = data.get('train_epochs', 0)
            self.peers = data.get('peers', 0)
            self.min_reward = data.get('min_reward', 1000)
            self.max_reward = data.get('max_reward', -1000)
            self.avg_reward = data.get('avg_reward', 0)
            peer_data = data.get('last_peer')
            if peer_data:
                self.last_peer = _Peer({
                    'session_id': peer_data.get('session_id', ''),
                    'channel': peer_data.get('channel', 1),
                    'rssi': peer_data.get('rssi', 0),
                    'identity': peer_data.get('identity', ''),
                    'advertisement': {
                        'name': peer_data.get('name', ''),
                        'pwnd_tot': peer_data.get('pwnd_tot', 0),
                    }
                })
            self.last_saved_session_id = self._get_last_saved_session_id()
            logging.info("loaded session data from cache (skipped log parsing)")
            return True
        except Exception as e:
            logging.debug("could not load session cache: %s" % e)
            return False

'''

# Insert before def parse(
code = code.replace('    def parse(self,', cache_block + '    def parse(self,', 1)

# Patch parse() to use cache: replace the 'else:' branch after 'if skip:'
# We need to add cache loading between 'if skip:' and 'else:'
old_parse = """        if skip:
            logging.debug("skipping parsing of the last session logs ...")
        else:"""
new_parse = """        if skip:
            logging.debug("skipping parsing of the last session logs ...")
        elif self._load_cache():
            logging.debug("session data loaded from cache")
        else:"""
code = code.replace(old_parse, new_parse, 1)

# Add _save_cache() call after _parse_stats() in parse()
code = code.replace(
    '            self._parse_stats()\n        self.parsed = True',
    '            self._parse_stats()\n            self._save_cache()\n        self.parsed = True',
    1
)

with open(f, 'w') as fh:
    fh.write(code)
PYEOF
        if [ $? -eq 0 ]; then
            log "DONE log.py: added session cache to LastSession.parse()"
            PATCHED=$((PATCHED+1))
        else
            log "FAIL log.py: python patch failed"
            FAILED=$((FAILED+1))
        fi
    fi
}

# ─── Patch 6: __init__.py — skip bettercap restart when disabled ───
patch_init() {
    local f="$SITE_PKG/__init__.py"
    [ -f "$f" ] || { log "SKIP __init__.py: $f not found"; SKIPPED=$((SKIPPED+1)); return; }

    if grep -q 'bettercap.*disabled' "$f" 2>/dev/null; then
        log "OK   __init__.py: already patched"
        SKIPPED=$((SKIPPED+1))
    else
        python3 - "$f" <<'PYEOF'
import sys
f = sys.argv[1]
with open(f) as fh:
    code = fh.read()
if 'bettercap' in code and 'disabled' in code:
    sys.exit(0)
old = '''    os.system("service bettercap restart")
    time.sleep(1)
    os.system("service pwnagotchi restart")'''
new = '''    if not (config or {}).get('bettercap', {}).get('disabled', False):
        os.system("service bettercap restart")
        time.sleep(1)
    os.system("service pwnagotchi restart")'''
code = code.replace(old, new, 1)
with open(f, 'w') as fh:
    fh.write(code)
PYEOF
        if [ $? -eq 0 ]; then
            log "DONE __init__.py: skip bettercap restart when disabled"
            PATCHED=$((PATCHED+1))
        else
            log "FAIL __init__.py: python patch failed"
            FAILED=$((FAILED+1))
        fi
    fi
}

# ─── Patch 7: agent.py — AO mode, StubClient, frame padding, synthetic blind epoch fix ───
patch_agent() {
    local f="$SITE_PKG/agent.py"
    [ -f "$f" ] || { log "SKIP agent.py: $f not found"; SKIPPED=$((SKIPPED+1)); return; }

    if grep -q 'ao_active' "$f" 2>/dev/null; then
        log "OK   agent.py: already patched (full, with blind epoch fix)"
        SKIPPED=$((SKIPPED+1))
    else
        python3 - "$f" <<'PYEOF'
import sys
f = sys.argv[1]
with open(f) as fh:
    code = fh.read()
if '_ao_mode' in code:
    sys.exit(0)

# Add imports after existing imports
code = code.replace(
    "from pwnagotchi.bettercap import Client",
    "from pwnagotchi.bettercap import Client\nfrom pwnagotchi.stub_client import StubClient\nfrom pwnagotchi.frame_padding import send_padded_deauth, send_padded_assoc",
    1
)

# Replace Client.__init__ block with AO mode detection
old_init = '''    def __init__(self, view, config, keypair):
        Client.__init__(self,
                        "127.0.0.1" if "hostname" not in config['bettercap'] else config['bettercap']['hostname'],
                        "http" if "scheme" not in config['bettercap'] else config['bettercap']['scheme'],
                        8081 if "port" not in config['bettercap'] else config['bettercap']['port'],
                        "pwnagotchi" if "username" not in config['bettercap'] else config['bettercap']['username'],
                        "pwnagotchi" if "password" not in config['bettercap'] else config['bettercap']['password'])'''
new_init = '''    def __init__(self, view, config, keypair):
        self._ao_mode = config.get('bettercap', {}).get('disabled', False)

        _bc = config['bettercap']
        _host = _bc.get('hostname', '127.0.0.1')
        _scheme = _bc.get('scheme', 'http')
        _port = _bc.get('port', 8081)
        _user = _bc.get('username', 'pwnagotchi')
        _pass = _bc.get('password', 'pwnagotchi')

        if self._ao_mode:
            self._stub = StubClient(_host, _scheme, _port, _user, _pass)
            Client.__init__(self, _host, _scheme, _port, _user, _pass)
            self.run = self._stub.run
            self.session = self._stub.session
            self.start_websocket = self._stub.start_websocket
            self.set_stub_aps = self._stub.set_stub_aps
            logging.info("[ao_mode] bettercap disabled — using StubClient")
        else:
            Client.__init__(self, _host, _scheme, _port, _user, _pass)'''
if old_init in code:
    code = code.replace(old_init, new_init, 1)

# Patch start() for AO mode
old_start = '''    def start(self):
        self._wait_bettercap()
        self.setup_events()
        self.set_starting()
        self.start_monitor_mode()
        self.start_event_polling()'''
new_start = '''    def _start_monitor_mode_direct(self):
        """Start monitor mode via subprocess (no bettercap needed)."""
        import subprocess as _sp
        mon_iface = self._config['main']['iface']
        mon_start_cmd = self._config['main'].get('mon_start_cmd', '')
        if os.path.exists('/sys/class/net/%s' % mon_iface):
            logging.info("[ao_mode] monitor interface %s already exists", mon_iface)
            self.start_advertising()
            return
        if mon_start_cmd:
            logging.info("[ao_mode] starting monitor interface via: %s", mon_start_cmd)
            try:
                _sp.run(mon_start_cmd, shell=True, timeout=15, check=False)
            except _sp.TimeoutExpired:
                logging.error("[ao_mode] mon_start_cmd timed out")
        for i in range(15):
            if os.path.exists('/sys/class/net/%s' % mon_iface):
                logging.info("[ao_mode] monitor interface %s is up", mon_iface)
                break
            time.sleep(1)
        else:
            logging.error("[ao_mode] monitor interface %s did not appear after 15s", mon_iface)
        logging.info("supported channels: %s", self._supported_channels)
        logging.info("handshakes will be collected inside %s", self._config['bettercap']['handshakes'])
        self.start_advertising()

    def start(self):
        if not self._ao_mode:
            self._wait_bettercap()
            self.setup_events()
        self.set_starting()
        if self._ao_mode:
            self._start_monitor_mode_direct()
        else:
            self.start_monitor_mode()
        if not self._ao_mode:
            self.start_event_polling()
        else:
            self._load_recovery_data()'''
if old_start in code:
    code = code.replace(old_start, new_start, 1)

# Patch get_access_points for synthetic blind epoch fix
old_gap = '''        aps.sort(key=lambda ap: ap['channel'])
        return self.set_access_points(aps)'''
new_gap = '''        # AO mode: if monitor interface is up, inject a synthetic AP so
        # epoch.observe() sees activity and blind_for resets. Without this,
        # pwnagotchi thinks the interface is dead and restarts.
        if self._ao_mode and not aps:
            mon_iface = self._config['main']['iface']
            if os.path.exists('/sys/class/net/%s' % mon_iface):
                aps.append({
                    'hostname': '[ao_active]',
                    'mac': '00:00:00:00:00:00',
                    'encryption': 'WPA2',
                    'channel': 0,
                    'rssi': 0,
                    'vendor': '',
                    'clients': [],
                })

        aps.sort(key=lambda ap: ap['channel'])
        return self.set_access_points(aps)'''
if old_gap in code:
    code = code.replace(old_gap, new_gap, 1)

with open(f, 'w') as fh:
    fh.write(code)
PYEOF
        if [ $? -eq 0 ]; then
            log "DONE agent.py: AO mode + StubClient + frame padding + blind epoch fix"
            PATCHED=$((PATCHED+1))
        else
            log "FAIL agent.py: python patch failed"
            FAILED=$((FAILED+1))
        fi
    fi
}

# ─── Patch 8: view.py — AO mode hides name, face near top ───
patch_view() {
    local f="$SITE_PKG/ui/view.py"
    [ -f "$f" ] || { log "SKIP view.py: $f not found"; SKIPPED=$((SKIPPED+1)); return; }

    if grep -q '_ao_mode' "$f" 2>/dev/null; then
        log "OK   view.py: already patched"
        SKIPPED=$((SKIPPED+1))
    else
        python3 - "$f" <<'PYEOF'
import sys
f = sys.argv[1]
with open(f) as fh:
    code = fh.read()
if '_ao_mode' in code:
    sys.exit(0)

# Add ao_mode detection after config assignment
code = code.replace(
    "self._canvas = None",
    "self._ao_mode = config.get('bettercap', {}).get('disabled', False)\n        self._canvas = None",
    1
)

# Replace face/name initialization with mode-aware version
old_face = """            'face': Text(value=faces.SLEEP,
                         position=(config['ui']['faces']['position_x'], config['ui']['faces']['position_y']),
                         color=BLACK, font=fonts.Huge, png=config['ui']['faces']['png']),"""
new_face = """            'face': Text(value=faces.SLEEP,
                         position=(config['ui']['faces']['position_x'],
                                   self._layout['line1'][1] + 2 if self._ao_mode else config['ui']['faces']['position_y']),
                         color=BLACK, font=fonts.Huge, png=config['ui']['faces']['png']),"""
if old_face in code:
    code = code.replace(old_face, new_face, 1)

# Replace name with mode-aware version
old_name = """            'name': Text(value='%s>' % 'pwnagotchi', position=self._layout['name'], color=BLACK, font=fonts.Bold),"""
new_name = """            'name': Text(value='' if self._ao_mode else '%s>' % 'pwnagotchi', position=self._layout['name'], color=BLACK, font=fonts.Bold),"""
if old_name in code:
    code = code.replace(old_name, new_name, 1)

# Disable cursor blink in AO mode
old_cursor = "                if self._config['ui'].get('cursor', True) == True:"
new_cursor = "                if not self._ao_mode and self._config['ui'].get('cursor', True) == True:"
if old_cursor in code:
    code = code.replace(old_cursor, new_cursor, 1)

with open(f, 'w') as fh:
    fh.write(code)
PYEOF
        if [ $? -eq 0 ]; then
            log "DONE view.py: AO mode hides name, face near top, no cursor blink"
            PATCHED=$((PATCHED+1))
        else
            log "FAIL view.py: python patch failed"
            FAILED=$((FAILED+1))
        fi
    fi
}

# ─── Patch 9: cli.py — empty name in AO mode ───
patch_cli() {
    local f="$SITE_PKG/cli.py"
    [ -f "$f" ] || { log "SKIP cli.py: $f not found"; SKIPPED=$((SKIPPED+1)); return; }

    if grep -q 'ao_mode' "$f" 2>/dev/null; then
        log "OK   cli.py: already patched"
        SKIPPED=$((SKIPPED+1))
    else
        if sed -i "s/display = Display(config=config, state={'name': '%s>' % pwnagotchi.name()})/ao_mode = config.get('bettercap', {}).get('disabled', False)\n    display = Display(config=config, state={'name': '' if ao_mode else '%s>' % pwnagotchi.name()})/" "$f"; then
            log "DONE cli.py: empty name in AO mode"
            PATCHED=$((PATCHED+1))
        else
            log "FAIL cli.py: sed failed"
            FAILED=$((FAILED+1))
        fi
    fi
}

# ─── Patch 10: components.py — PNG face with text fallback ───
patch_components() {
    local f="$SITE_PKG/ui/components.py"
    [ -f "$f" ] || { log "SKIP components.py: $f not found"; SKIPPED=$((SKIPPED+1)); return; }

    if grep -q 'os.path.sep' "$f" 2>/dev/null; then
        log "OK   components.py: already patched"
        SKIPPED=$((SKIPPED+1))
    else
        python3 - "$f" <<'PYEOF'
import sys
f = sys.argv[1]
with open(f) as fh:
    code = fh.read()
if 'os.path.sep' in code:
    sys.exit(0)

# Add import os at the top if not present
if 'import os' not in code:
    code = code.replace('from PIL import', 'import os\nfrom PIL import', 1)

# Wrap the PNG loading block in try/except with text fallback
old_png = """                self.image = Image.open(self.value)
                self.image = self.image.convert('RGBA')
                self.pixels = self.image.load()
                for y in range(self.image.size[1]):
                    for x in range(self.image.size[0]):
                        if self.pixels[x,y][3] < 255:    # check alpha
                            self.pixels[x,y] = (255, 255, 255, 255)
                if self.color == 255:
                    self._image = ImageOps.colorize(self.image.convert('L'), black = "white", white = "black")
                else:
                    self._image = self.image
                self.image = self._image.convert('1')
                canvas.paste(self.image, self.xy)"""
new_png = """                try:
                    self.image = Image.open(self.value)
                    self.image = self.image.convert('RGBA')
                    self.pixels = self.image.load()
                    for y in range(self.image.size[1]):
                        for x in range(self.image.size[0]):
                            if self.pixels[x,y][3] < 255:    # check alpha
                                self.pixels[x,y] = (255, 255, 255, 255)
                    if self.color == 255:
                        self._image = ImageOps.colorize(self.image.convert('L'), black = "white", white = "black")
                    else:
                        self._image = self.image
                    self.image = self._image.convert('1')
                    canvas.paste(self.image, self.xy)
                except Exception:
                    # PNG load failed (value is a text face, not a file path) — render as text
                    if isinstance(self.value, str) and not os.path.sep in self.value:
                        drawer.text(self.xy, self.value, font=self.font, fill=self.color)"""
if old_png in code:
    code = code.replace(old_png, new_png, 1)

with open(f, 'w') as fh:
    fh.write(code)
PYEOF
        if [ $? -eq 0 ]; then
            log "DONE components.py: PNG face with text fallback"
            PATCHED=$((PATCHED+1))
        else
            log "FAIL components.py: python patch failed"
            FAILED=$((FAILED+1))
        fi
    fi
}

# ─── Main ───
main() {
    log "=== Oxigotchi patch check starting ==="

    # Must be root for writing to /usr/bin and site-packages
    if [ "$(id -u)" -ne 0 ]; then
        die "Must be run as root (use sudo)"
    fi

    patch_pwnlib
    patch_cache
    patch_handler
    patch_server
    patch_log
    patch_init
    patch_agent
    patch_view
    patch_cli
    patch_components

    log "=== Done: $PATCHED applied, $SKIPPED already ok, $FAILED failed ==="

    if [ "$FAILED" -gt 0 ]; then
        exit 1
    fi

    # If any patches were applied, restart pwnagotchi to pick them up
    if [ "$PATCHED" -gt 0 ]; then
        if systemctl is-active --quiet pwnagotchi 2>/dev/null; then
            log "Restarting pwnagotchi to pick up patches..."
            systemctl restart pwnagotchi
        fi
    fi
}

main "$@"
