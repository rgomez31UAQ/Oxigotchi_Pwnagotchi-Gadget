import json
import logging
import os
import re
import subprocess
import signal
import time
import glob
import shutil
from threading import Lock

import pwnagotchi.plugins as plugins
import pwnagotchi.ui.faces as faces
from pwnagotchi.ui.components import LabeledValue
from pwnagotchi.ui.view import BLACK
import pwnagotchi.ui.fonts as fonts


class AngryOxide(plugins.Plugin):
    __author__ = 'pwnoxide'
    __version__ = '2.1.0'
    __license__ = 'GPL3'
    __description__ = 'Oxigotchi v2 attack engine — integrates AngryOxide with v5 firmware. No nexmon throttle needed.'
    __name__ = 'angryoxide'
    csrf_exempt = True
    __help__ = """
    Runs AngryOxide alongside bettercap. Bettercap handles recon (AP/client discovery),
    while AngryOxide handles all active attacks (PMKID, CSA, deauth) on the shared
    monitor interface.

    v5 firmware handles injection natively — no nexmon_mode, no adaptive throttle,
    no injection delay tuning. Just works.
    """

    # Firmware crash signature: -110 errors from brcmfmac channel set failures
    _FW_CRASH_PATTERN = re.compile(
        r'brcmf.*Set Channel failed.*-110|brcmf.*firmware has halted',
        re.IGNORECASE
    )

    # Fast boot: defer non-essential plugins until AO is running
    _DELAY_PLUGINS = [
        'bt-tether', 'grid', 'wpa-sec', 'auto_backup', 'memtemp-plus',
        'display-password', 'IPDisplay', 'handshakes-dl', 'better_quickdic',
        'pwnstore_ui', 'internet-connection',
    ]
    _KEEP_PLUGINS = [
        'angryoxide', 'pisugarx', 'cache', 'fix_services', 'logtail',
        'exp', 'tweak_view', 'webcfg', 'button-feedback',
    ]
    _DELAY_STATE_FILE = '/home/pi/delayed_plugins.json'

    def __init__(self):
        self.options = dict()
        self._lock = Lock()
        self._process = None
        self._running = False
        self._captures = 0
        self._captures_this_epoch = 0
        self._known_files = {}  # {filepath: mtime}
        self._original_deauth = None
        self._original_associate = None
        self._fw_crash_count = 0
        self._last_recovery = 0
        # backoff state
        self._crash_count = 0
        self._last_crash_time = 0
        self._stopped_permanently = False
        self._base_backoff_secs = 5
        self._stable_epochs = 0
        # uptime tracking
        self._start_time = None
        # Runtime attack config (these override _build_cmd)
        self._attacks = {
            'deauth': True,
            'pmkid': True,
            'csa': True,
            'disassoc': True,
            'anon_reassoc': True,
            'rogue_m2': True,
        }
        self._rate = 1          # 1, 2, or 3 (rate 2+ crashes BCM43436B0 in ~68s)
        self._channels = ''     # empty = default (1,6,11)
        self._autohunt = False
        self._dwell = 2         # seconds
        self._targets = []      # list of MAC/SSID strings
        self._whitelist_entries = []  # list of MAC/SSID strings
        self._skip_captured = False  # when True, add captured AP MACs to AO whitelist
        self._state_file = '/etc/pwnagotchi/custom-plugins/angryoxide_state.json'
        self._agent = None      # store agent ref for restart from webhook
        # Edge-case face paths (set when png mode is active)
        self._face_dir = '/etc/pwnagotchi/custom-plugins/faces'
        # Cached set of captured AP MACs (refreshed by _scan_captures)
        self._captured_macs = set()
        self._captured_macs_stale = True
        # Track pcapng paths that have been confirmed as verified (.22000 companion exists)
        # Used to fire bonus XP only once per newly-verified capture
        self._known_verified = set()
        self._discord_webhook = None
        self._pwn_deauth = True
        self._pwn_associate = True
        # Non-blocking restart scheduling (avoids sleeping the main thread)
        self._next_restart_time = 0
        # Cache for expensive API calls (subprocess-based)
        self._health_cache = None
        self._health_cache_time = 0
        self._mode_cache = None
        self._mode_cache_time = 0
        # H2: monkey-patch flag for safe _update_peers wrapper
        self._peers_patched = False

    def _face(self, name):
        """Return face path for PNG mode, or fall back to text faces."""
        # Only use PNG paths if the display is configured for PNG mode
        png_enabled = False
        if self._agent:
            try:
                png_enabled = self._agent._config.get('ui', {}).get('faces', {}).get('png', False)
            except Exception:
                pass
        if png_enabled:
            png_path = os.path.join(self._face_dir, '%s.png' % name)
            if os.path.isfile(png_path):
                return png_path
        # Fallback to stock text faces
        fallback = {
            'wifi_down': faces.BROKEN, 'fw_crash': faces.BROKEN,
            'ao_crashed': faces.ANGRY, 'battery_low': faces.SAD,
            'battery_critical': faces.BROKEN, 'shutdown': faces.SLEEP,
        }
        return fallback.get(name, faces.AWAKE)

    def _get_battery_level(self):
        """Read battery percentage from PiSugar. Returns int or None."""
        try:
            with open('/tmp/pisugar-battery', 'r') as f:
                return int(float(f.read().strip()))
        except Exception:
            pass
        # Try pisugarx plugin's shared state
        try:
            with open('/sys/class/power_supply/battery/capacity', 'r') as f:
                return int(f.read().strip())
        except Exception:
            pass
        return None

    def _save_state(self, force=False):
        """Persist runtime config to disk. Debounced: writes at most once per 30s unless force=True."""
        self._state_dirty = True
        now = time.time()
        if not force and now - getattr(self, '_last_state_save', 0) < 30:
            return  # will be flushed by on_epoch or on_unload
        state = {
            'targets': self._targets,
            'whitelist': self._whitelist_entries,
            'rate': self._rate,
            'attacks': self._attacks,
            'channels': self._channels,
            'autohunt': self._autohunt,
            'dwell': self._dwell,
            'skip_captured': self._skip_captured,
            'discord_webhook': self._discord_webhook or '',
            'pwn_deauth': self._pwn_deauth,
            'pwn_associate': self._pwn_associate,
        }
        try:
            with open(self._state_file, 'w') as f:
                json.dump(state, f, indent=2)
            self._state_dirty = False
            self._last_state_save = now
        except Exception as e:
            logging.debug("[angryoxide] could not save state: %s", e)

    def _load_state(self):
        """Load persisted runtime config from disk."""
        try:
            with open(self._state_file, 'r') as f:
                state = json.load(f)
            self._targets = state.get('targets', [])
            self._whitelist_entries = state.get('whitelist', [])
            self._rate = state.get('rate', 1)
            self._attacks = state.get('attacks', self._attacks)
            self._channels = state.get('channels', '')
            self._autohunt = state.get('autohunt', False)
            self._dwell = state.get('dwell', 2)
            self._skip_captured = state.get('skip_captured', False)
            self._discord_webhook = state.get('discord_webhook', self._discord_webhook or '')
            self._pwn_deauth = state.get('pwn_deauth', True)
            self._pwn_associate = state.get('pwn_associate', True)
            logging.info("[angryoxide] loaded saved state: %d targets, %d whitelist entries", len(self._targets), len(self._whitelist_entries))
        except FileNotFoundError:
            pass
        except Exception as e:
            logging.debug("[angryoxide] could not load state: %s", e)

    def on_loaded(self):
        binary = self.options.get('binary_path', '/usr/local/bin/angryoxide')
        if not os.path.isfile(binary):
            logging.warning("[angryoxide] binary not found at %s - plugin will not start until binary is installed", binary)
            return
        logging.info("[angryoxide] plugin v%s loaded, binary found at %s", self.__version__, binary)
        self._load_state()
        self._discord_webhook = self.options.get('discord_webhook', '')

    def _save_delayed_plugins(self):
        """On shutdown, mark non-essential plugins for delayed loading."""
        import pwnagotchi.plugins as _plugins
        try:
            delayed = []
            for name in self._DELAY_PLUGINS:
                if name in _plugins.loaded and _plugins.loaded[name] is not None:
                    delayed.append(name)
            if delayed:
                with open(self._DELAY_STATE_FILE, 'w') as f:
                    json.dump({'delayed': delayed, 'timestamp': time.time()}, f)
                logging.info("[angryoxide] saved %d plugins for delayed boot: %s", len(delayed), delayed)
        except Exception as e:
            logging.debug("[angryoxide] could not save delayed plugins: %s", e)

    def _restore_delayed_plugins(self):
        """After AO is running, re-enable plugins that were delayed for fast boot."""
        try:
            if not os.path.isfile(self._DELAY_STATE_FILE):
                return
            with open(self._DELAY_STATE_FILE, 'r') as f:
                data = json.load(f)
            # Ignore stale state files (older than 10 minutes)
            ts = data.get('timestamp', 0)
            if time.time() - ts > 600:
                logging.info("[angryoxide] delayed plugins state file is stale (%.0fs old), ignoring",
                             time.time() - ts)
                os.remove(self._DELAY_STATE_FILE)
                return
            delayed = data.get('delayed', [])
            if not delayed:
                return
            import pwnagotchi.plugins as _plugins
            restored = 0
            for name in delayed:
                try:
                    _plugins.toggle_plugin(name, True)
                    logging.info("[angryoxide] restored delayed plugin: %s", name)
                    restored += 1
                except Exception as e:
                    logging.debug("[angryoxide] could not restore plugin %s: %s", name, e)
            os.remove(self._DELAY_STATE_FILE)
            logging.info("[angryoxide] restored %d delayed plugins", restored)
        except Exception as e:
            logging.debug("[angryoxide] could not restore delayed plugins: %s", e)

    def _is_ao_mode(self):
        """Check if we are currently in AO mode (vs PWN mode). Cached for 30s."""
        now = time.time()
        if self._mode_cache and now - self._mode_cache_time < 30:
            return self._mode_cache.get('mode') == 'ao'
        try:
            r = subprocess.run(['pwnoxide-mode', 'status'], capture_output=True, text=True, timeout=5)
            is_ao = 'AngryOxide' in r.stdout
            mode = 'ao' if is_ao else 'pwn'
            self._mode_cache = {'mode': mode, 'details': r.stdout.strip()}
            self._mode_cache_time = now
            return is_ao
        except Exception:
            return True  # default to AO mode if status check fails

    def on_ready(self, agent):
        self._agent = agent

        binary = self.options.get('binary_path', '/usr/local/bin/angryoxide')
        if not os.path.isfile(binary):
            logging.error("[angryoxide] binary not found at %s, cannot start", binary)
            return

        # Only disable bettercap's attacks when AO is active;
        # in PWN mode, bettercap needs them to populate the AP list.
        if self._is_ao_mode():
            self._original_deauth = agent._config['personality']['deauth']
            self._original_associate = agent._config['personality']['associate']
            agent._config['personality']['deauth'] = False
            agent._config['personality']['associate'] = False
            logging.info("[angryoxide] AO mode: disabled bettercap deauth/assoc, AO will handle attacks")
        else:
            # Apply saved PWN attack settings
            agent._config['personality']['deauth'] = self._pwn_deauth
            agent._config['personality']['associate'] = self._pwn_associate
            # Restore stock face position (below "pwnagotchi>" name)
            try:
                pos_x = agent._config['ui']['faces'].get('position_x', 0)
                agent._config['ui']['faces']['position_y'] = 34
                agent._view._state._state['face'].xy = (pos_x, 34)
            except Exception:
                pass
            logging.info("[angryoxide] PWN mode: deauth=%s associate=%s",
                         self._pwn_deauth, self._pwn_associate)

        self._start_ao(agent)

        # Restore delayed plugins 30s after AO starts (don't slow down AO startup)
        if self._running:
            import threading
            threading.Timer(30.0, self._restore_delayed_plugins).start()

        # Set awake bull face on boot — only in AO mode.
        # In PWN mode with png=false, let pwnagotchi core handle faces
        # (text kaomoji like "(◕‿‿◕)"), don't override with PNG bull face.
        if self._is_ao_mode():
            try:
                agent._view.set('face', self._face('awake'))
                agent._view.update()
            except Exception:
                pass

    def _build_cmd(self):
        binary = self.options.get('binary_path', '/usr/local/bin/angryoxide')
        iface = self.options.get('interface', 'wlan0mon')
        output_dir = self.options.get('output_dir', '/etc/pwnagotchi/handshakes/')
        notx = self.options.get('notx', False)
        no_setup = self.options.get('no_setup', True)
        extra_args = self.options.get('extra_args', '')

        # H3: AO --output is a filename prefix, not just a directory.
        # Without a prefix, captures are named "-DATETIME.pcapng" (empty prefix).
        # Append the device hostname so files become "oxigotchi-DATETIME.pcapng".
        import socket
        capture_prefix = self.options.get('capture_prefix', '')
        if not capture_prefix:
            try:
                capture_prefix = socket.gethostname() or 'oxigotchi'
            except Exception:
                capture_prefix = 'oxigotchi'
        output_path = os.path.join(output_dir, capture_prefix)
        cmd = [binary, '--interface', iface, '--headless', '--output', output_path]

        if notx:
            cmd.append('--notx')

        if no_setup:
            cmd.append('--no-setup')

        # Attack rate
        cmd.extend(['--rate', str(self._rate)])

        # Channel config
        if self._autohunt:
            cmd.append('--autohunt')
        elif self._channels:
            cmd.extend(['--channel', self._channels])

        # Dwell time
        cmd.extend(['--dwell', str(self._dwell)])

        # GPS integration — pass --gpsd if GPS daemon is running
        gpsd_host = self.options.get('gpsd_host', '127.0.0.1:2947')
        try:
            import socket
            host, port = gpsd_host.split(':')
            s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            s.settimeout(1)
            s.connect((host, int(port)))
            s.close()
            cmd.extend(['--gpsd', gpsd_host])
        except Exception:
            pass  # GPS not available, skip

        # Attack toggles — disable the ones that are off
        _ATTACK_FLAGS = {
            'deauth': '--disable-deauth',
            'pmkid': '--disable-pmkid',
            'csa': '--disable-csa',
            'disassoc': '--disable-disassoc',
            'anon_reassoc': '--disable-anon',
            'rogue_m2': '--disable-roguem2',
        }
        for attack, flag in _ATTACK_FLAGS.items():
            if not self._attacks.get(attack, True):
                cmd.append(flag)

        # Targets
        for t in self._targets:
            cmd.extend(['--target-entry', t])

        # Whitelist
        for w in self._whitelist_entries:
            cmd.extend(['--whitelist-entry', w])

        # Auto-whitelist already-captured APs
        if self._skip_captured:
            ao_dir = self.options.get('output_dir', '/etc/pwnagotchi/handshakes/')
            bc_dir = '/home/pi/handshakes'
            existing_upper = {w.upper() for w in self._whitelist_entries}
            for d in [ao_dir, bc_dir]:
                if os.path.isdir(d):
                    for f in os.listdir(d):
                        mac = self._extract_mac_from_filename(f)
                        # AO uses timestamp filenames — fall back to .22000 parser
                        if mac is None and f.endswith(('.pcapng', '.pcap')):
                            hash_path = os.path.join(d, f.rsplit('.', 1)[0] + '.22000')
                            if os.path.isfile(hash_path):
                                mac, _ = self._parse_22000_file(hash_path)
                        if mac and mac.upper() not in existing_upper:
                            cmd.extend(['--whitelist-entry', mac.upper()])
                            existing_upper.add(mac.upper())

        if extra_args:
            cmd.extend(extra_args.split())

        return cmd

    def _start_ao(self, agent):
        with self._lock:
            if self._running:
                return

            iface = self.options.get('interface', 'wlan0mon')
            if not os.path.exists('/sys/class/net/%s' % iface):
                logging.error("[angryoxide] cannot start: interface %s does not exist", iface)
                return

            output_dir = self.options.get('output_dir', '/etc/pwnagotchi/handshakes/')
            os.makedirs(output_dir, exist_ok=True)

            # snapshot existing pcapng files with mtimes so we don't double-count
            self._known_files = {}
            for f in glob.glob(os.path.join(output_dir, '*.pcapng')):
                try:
                    self._known_files[f] = os.path.getmtime(f)
                except OSError:
                    pass

            cmd = self._build_cmd()
            logging.info("[angryoxide] starting: %s", ' '.join(cmd))

            try:
                self._process = subprocess.Popen(
                    cmd,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    preexec_fn=os.setsid
                )
                self._running = True
                self._start_time = time.time()
                logging.info("[angryoxide] started with PID %d", self._process.pid)
            except Exception as e:
                logging.error("[angryoxide] failed to start: %s", e)
                self._running = False

    def _stop_ao(self):
        with self._lock:
            if self._process and self._running:
                logging.info("[angryoxide] stopping AO (PID %d)", self._process.pid)
                try:
                    pgid = os.getpgid(self._process.pid)
                    os.killpg(pgid, signal.SIGTERM)
                    self._process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    logging.warning("[angryoxide] AO did not stop gracefully, sending SIGKILL")
                    try:
                        os.killpg(pgid, signal.SIGKILL)
                        self._process.wait(timeout=5)
                    except ProcessLookupError:
                        logging.debug("[angryoxide] process already exited before SIGKILL")
                    except Exception as e:
                        logging.error("[angryoxide] error during SIGKILL: %s", e)
                except ProcessLookupError:
                    logging.debug("[angryoxide] process already exited before SIGTERM")
                except Exception as e:
                    logging.error("[angryoxide] error stopping AO: %s", e)
                finally:
                    try:
                        self._process.wait(timeout=3)  # reap zombie
                    except Exception:
                        pass
                    self._process = None
                    self._running = False
                    self._start_time = None

    def _restart_ao(self):
        """Stop and restart AO with current settings. Used by web dashboard."""
        self._stop_ao()
        if self._agent:
            self._start_ao(self._agent)

    def _backoff_seconds(self):
        """Calculate exponential backoff: min(5 * 2^(crashes-1), 300)."""
        return min(self._base_backoff_secs * (2 ** (self._crash_count - 1)), 300)

    def _check_health(self, agent):
        """Check if AO process is still alive, restart if crashed.
        Returns True if a crash was detected (needs_restart)."""
        needs_restart = False
        with self._lock:
            if not self._running:
                return False
            if self._process and self._process.poll() is not None:
                rc = self._process.returncode
                logging.warning("[angryoxide] AO process died with exit code %d", rc)
                try:
                    self._process.wait(timeout=3)  # reap zombie
                except Exception:
                    pass
                self._process = None
                self._running = False
                self._start_time = None
                needs_restart = True

        if needs_restart:
            now = time.time()
            self._crash_count += 1
            self._last_crash_time = now
            self._stable_epochs = 0

            max_crashes = self.options.get('max_crashes', 10)
            if self._crash_count >= max_crashes:
                logging.error("[angryoxide] reached max crash count (%d), stopping permanently. "
                              "Use webhook POST /plugins/angryoxide/reset to retry.", max_crashes)
                self._stopped_permanently = True
                return True

            # check if this looks like a firmware crash before restarting
            recovery_ok = self._try_fw_recovery()

            if not recovery_ok:
                logging.error("[angryoxide] firmware recovery failed, not restarting")
                return True

            backoff = self._backoff_seconds()
            logging.info("[angryoxide] scheduling restart after %.1fs backoff (crash %d/%d)",
                         backoff, self._crash_count, max_crashes)
            self._next_restart_time = time.time() + backoff

        return needs_restart

    def _try_fw_recovery(self):
        """Detect and recover from brcmfmac firmware crashes (-110 channel set errors).
        Returns True if recovery succeeded or was not needed, False otherwise."""
        now = time.time()
        # don't attempt recovery more than once per 60 seconds
        if now - self._last_recovery < 60:
            return True

        try:
            result = subprocess.run(
                ['journalctl', '-k', '--since', '-60s', '--no-pager'],
                capture_output=True, text=True, timeout=5
            )
            if self._FW_CRASH_PATTERN.search(result.stdout):
                self._fw_crash_count += 1
                self._last_recovery = now
                logging.warning("[angryoxide] firmware crash detected (count: %d), attempting brcmfmac recovery",
                                self._fw_crash_count)
                iface = self.options.get('interface', 'wlan0mon')
                try:
                    # GPIO power cycle: toggle WL_REG_ON (GPIO 41) to cold-reset
                    # the BCM43436B0 chip. This recovers from SDIO bus death (error -22)
                    # that modprobe alone cannot fix.
                    logging.info("[angryoxide] attempting GPIO power cycle recovery (WL_REG_ON)")
                    subprocess.run(['sudo', 'modprobe', '-r', 'brcmfmac'], timeout=10, capture_output=True)
                    time.sleep(1)
                    # Unbind MMC controller
                    subprocess.run(['sudo', 'bash', '-c',
                                    'echo 3f300000.mmcnr > /sys/bus/platform/drivers/mmc-bcm2835/unbind'],
                                   timeout=5, capture_output=True)
                    # Pull WL_REG_ON low (power off WiFi chip)
                    subprocess.run(['sudo', 'pinctrl', 'set', '41', 'op', 'dl'], timeout=5, capture_output=True)
                    time.sleep(2)
                    # Push WL_REG_ON high (power on WiFi chip)
                    subprocess.run(['sudo', 'pinctrl', 'set', '41', 'op', 'dh'], timeout=5, capture_output=True)
                    time.sleep(1)
                    # Rebind MMC controller
                    subprocess.run(['sudo', 'bash', '-c',
                                    'echo 3f300000.mmcnr > /sys/bus/platform/drivers/mmc-bcm2835/bind'],
                                   timeout=5, capture_output=True)
                    time.sleep(3)
                    # Reload driver
                    subprocess.run(['sudo', 'modprobe', 'brcmfmac'], timeout=10, capture_output=True)
                    time.sleep(3)
                    # Set up monitor mode
                    subprocess.run(['monstart'], timeout=10, capture_output=True)
                    logging.info("[angryoxide] GPIO power cycle recovery completed, verifying interface")

                    # poll for interface to come back
                    for attempt in range(5):
                        time.sleep(2)
                        if os.path.exists('/sys/class/net/%s' % iface):
                            logging.info("[angryoxide] interface %s is back (attempt %d)", iface, attempt + 1)
                            return True
                    logging.error("[angryoxide] interface %s did not come back after GPIO recovery", iface)
                    return False
                except Exception as e:
                    logging.error("[angryoxide] firmware recovery failed: %s", e)
                    return False
        except Exception as e:
            logging.debug("[angryoxide] could not check kernel logs: %s", e)
        return True

    def _count_pcapngs(self, output_dir):
        """Count all .pcapng files in output_dir (total captures on disk)."""
        return len(glob.glob(os.path.join(output_dir, '*.pcapng')))

    def _count_verified(self, output_dir):
        """Count pcapng files that have a companion .22000 file (confirmed crackable handshake)."""
        count = 0
        for f in glob.glob(os.path.join(output_dir, '*.pcapng')):
            if os.path.isfile(f.rsplit('.', 1)[0] + '.22000'):
                count += 1
        return count

    def _is_whitelisted(self, filename, whitelist):
        """Check if a capture filename matches any whitelist entry.
        Mirrors utils.remove_whitelisted normalization: lowercase, alphanumeric-only, substring match."""
        def normalize(name):
            return ''.join(c for c in name if c.isalnum()).lower()

        # strip both .pcapng and .pcap extensions
        base = filename
        if base.endswith('.pcapng'):
            base = base[:-7]
        elif base.endswith('.pcap'):
            base = base[:-5]
        normalized = normalize(base)

        for entry in whitelist:
            if normalize(entry) in normalized:
                return True
        return False

    def _scan_captures(self, agent):
        """Check for new or modified pcapng files from AO and trigger handshake events.
        Returns the number of new captures found this scan."""
        output_dir = self.options.get('output_dir', '/etc/pwnagotchi/handshakes/')
        handshake_dir = agent._config['bettercap']['handshakes']
        whitelist = agent._config.get('main', {}).get('whitelist', [])

        current_files = {}
        for f in glob.glob(os.path.join(output_dir, '*.pcapng')):
            try:
                current_files[f] = os.path.getmtime(f)
            except OSError:
                pass

        # detect new files and files with updated mtime
        new_or_modified = []
        for filepath, mtime in current_files.items():
            if filepath not in self._known_files or mtime > self._known_files[filepath]:
                new_or_modified.append(filepath)

        # filter against whitelist
        if whitelist:
            new_or_modified = [f for f in new_or_modified
                               if not self._is_whitelisted(os.path.basename(f), whitelist)]

        new_count = 0
        for filepath in new_or_modified:
            is_new = filepath not in self._known_files
            if is_new:
                self._captures += 1
                new_count += 1
            filename = os.path.basename(filepath)
            logging.info("[angryoxide] %s capture: %s (total: %d)",
                         "new" if is_new else "updated", filename, self._captures)

            # notify Discord on new captures
            if is_new:
                ap_mac_notify, _ = self._parse_capture_filename(filename)
                self._notify_capture(filename, ap_mac_notify)

            # copy to pwnagotchi handshake dir if different from AO output
            dest = filepath
            if os.path.abspath(output_dir) != os.path.abspath(handshake_dir):
                dest = os.path.join(handshake_dir, filename)
                try:
                    shutil.copy2(filepath, dest)
                except Exception as e:
                    logging.error("[angryoxide] failed to copy capture to %s: %s", dest, e)
                    dest = filepath

            # trigger events for downstream plugins (exp, wigle, wpa-sec, pwncrack)
            # AO bypasses bettercap so we emit association/deauth/handshake events
            # that plugins like EXP rely on for XP gain
            ap_mac, sta_mac = self._parse_capture_filename(filename)
            ap_info = {'mac': ap_mac, 'hostname': '', 'vendor': '', 'channel': 0, 'rssi': 0}
            sta_info = {'mac': sta_mac, 'vendor': ''}
            plugins.on('association', agent, ap_info)
            plugins.on('deauthentication', agent, ap_info, sta_info)
            plugins.on('handshake', agent, dest, ap_mac, sta_mac)

            # register in agent handshake tracking for display/mood
            if is_new:
                key = "%s -> %s" % (sta_mac, ap_mac)
                if key not in agent._handshakes:
                    agent._handshakes[key] = {'source': 'angryoxide', 'file': dest}
                agent._last_pwnd = ap_mac

        # update display and trigger mood if we got new captures
        if new_count > 0:
            agent._update_handshakes(new_count)
            # H1: Reset blind epoch counter — captures prove the interface is
            # working even though bettercap's wifi.recon doesn't see AO's activity.
            # Without this, blind_for climbs every epoch and the AI reward stays
            # negative, eventually triggering a needless restart.
            try:
                agent._epoch.blind_for = 0
                agent._epoch.any_activity = True
                logging.debug("[angryoxide] reset blind_for: %d new captures fed to AI", new_count)
            except Exception:
                pass

        self._known_files = current_files
        # Refresh captured MACs cache so _get_access_points doesn't rescan dirs
        self._captured_macs_stale = True

        # Check for captures that became verified this scan (.22000 companion appeared).
        # Fire a bonus handshake event so exp plugin rewards verified captures with extra XP.
        for filepath in current_files:
            if filepath in self._known_verified:
                continue
            hash_path = filepath.rsplit('.', 1)[0] + '.22000'
            if os.path.isfile(hash_path):
                self._known_verified.add(filepath)
                filename = os.path.basename(filepath)
                ap_mac, sta_mac = self._parse_capture_filename(filename)
                dest_path = filepath
                if os.path.abspath(output_dir) != os.path.abspath(handshake_dir):
                    dest_path = os.path.join(handshake_dir, filename)
                plugins.on('handshake', agent, dest_path, ap_mac, sta_mac)
                logging.info("[angryoxide] capture verified (.22000 present): %s", filename)

        return new_count

    @staticmethod
    def _extract_mac_from_filename(filename):
        """Extract MAC address from a capture filename, or return None.
        Handles AO format (AA-BB-CC-DD-EE-FF_name.pcapng) and bettercap format (AA:BB:CC:DD:EE:FF.pcap)."""
        mac_part = filename.split('_')[0].split('.')[0].replace('-', ':')
        if len(mac_part.split(':')) == 6:
            return mac_part
        return None

    @staticmethod
    def _parse_capture_filename(filename):
        """Try to extract AP MAC from AO capture filename. Returns (ap_mac, sta_mac) strings."""
        mac = AngryOxide._extract_mac_from_filename(filename)
        if mac:
            return mac, 'unknown'
        return 'unknown', 'unknown'

    @staticmethod
    def _parse_22000_file(path):
        """Parse a .22000 hashcat file to extract AP MAC and SSID.
        Format: WPA*type*pmkid_or_mic*AP_MAC*STA_MAC*ESSID_hex*...
        Returns (ap_mac, ssid) or (None, None)."""
        try:
            with open(path, 'r') as f:
                line = f.readline().strip()
            if not line.startswith('WPA*'):
                return None, None
            parts = line.split('*')
            if len(parts) < 6:
                return None, None
            raw_mac = parts[3]
            ap_mac = ':'.join(raw_mac[i:i+2] for i in range(0, 12, 2)).upper() if len(raw_mac) == 12 else raw_mac
            ssid_hex = parts[5]
            try:
                ssid = bytes.fromhex(ssid_hex).decode('utf-8', errors='replace')
            except Exception:
                ssid = ''
            return ap_mac, ssid
        except Exception:
            return None, None

    def _format_uptime(self):
        """Format uptime as Xm (minutes) or Xh (hours)."""
        if self._start_time is None:
            return '0m'
        elapsed = time.time() - self._start_time
        if elapsed < 3600:
            return '%dm' % (elapsed // 60)
        return '%dh' % (elapsed // 3600)

    def _notify_capture(self, filename, ap_mac):
        """Send Discord notification on new capture."""
        if not self._discord_webhook or self._discord_webhook.startswith('https://discord.com/api/webhooks/YOUR'):
            return
        try:
            import urllib.request
            data = json.dumps({
                'content': None,
                'embeds': [{
                    'title': 'New Capture!',
                    'description': '**%s**\nMAC: `%s`\nTotal: %d captures' % (
                        filename.replace('.pcapng', '').split('_', 1)[-1] or ap_mac,
                        ap_mac, self._captures),
                    'color': 43200,
                }]
            }).encode()
            req = urllib.request.Request(self._discord_webhook, data=data,
                                         headers={'Content-Type': 'application/json'})
            urllib.request.urlopen(req, timeout=5)
        except Exception as e:
            logging.debug('[angryoxide] discord notify failed: %s', e)

    def _get_health(self):
        """Gather system health for dashboard. Cached for 10 seconds."""
        now = time.time()
        if self._health_cache and now - self._health_cache_time < 60:
            return self._health_cache
        health = {'wifi': False, 'monitor': False, 'firmware': True, 'usb0': False, 'battery': None, 'battery_charging': None}
        try:
            health['wifi'] = os.path.exists('/sys/class/net/wlan0')
            iface = self.options.get('interface', 'wlan0mon')
            health['monitor'] = os.path.exists('/sys/class/net/%s' % iface)
            # AO process alive means nothing if interface is gone — check both
            if self._running and self._process and self._process.poll() is None:
                if health['wifi'] and health['monitor']:
                    pass  # both interface + process alive = truly healthy
                else:
                    # AO is zombie — interface dead, process lingering
                    health['wifi'] = False
                    health['monitor'] = False
            health['usb0'] = os.path.exists('/sys/class/net/usb0')
        except Exception:
            pass
        # Check firmware status: only flag as bad if crash detected AND interface is actually down
        try:
            r = subprocess.run(['journalctl', '-k', '--since', '-60s', '--no-pager'], capture_output=True, text=True, timeout=3)
            if self._FW_CRASH_PATTERN.search(r.stdout) and not health['wifi']:
                health['firmware'] = False
        except Exception:
            pass
        # Battery: reuse existing method instead of shelling out to cat
        health['battery'] = self._get_battery_level()
        self._health_cache = health
        self._health_cache_time = time.time()
        return health

    def _refresh_captured_macs(self):
        """Rebuild the cached set of captured AP MACs from handshake directories.
        Called lazily when _captured_macs_stale is True (set by _scan_captures)."""
        ao_dir = self.options.get('output_dir', '/etc/pwnagotchi/handshakes/')
        bc_dir = self._agent._config['bettercap']['handshakes'] if self._agent else '/home/pi/handshakes'
        macs = set()
        for d in [ao_dir, bc_dir]:
            if os.path.isdir(d):
                for f in os.listdir(d):
                    mac = self._extract_mac_from_filename(f)
                    # AO uses timestamp filenames — fall back to .22000 parser
                    if mac is None and f.endswith(('.pcapng', '.pcap')):
                        hash_path = os.path.join(d, f.rsplit('.', 1)[0] + '.22000')
                        if os.path.isfile(hash_path):
                            mac, _ = self._parse_22000_file(hash_path)
                    if mac:
                        macs.add(mac.lower())
        self._captured_macs = macs
        self._captured_macs_stale = False

    def _get_access_points(self):
        """Get current AP list from agent."""
        aps = []
        if self._agent:
            try:
                for ap in self._agent._access_points:
                    aps.append({
                        'ssid': ap.get('hostname', ''),
                        'mac': ap.get('mac', ''),
                        'channel': ap.get('channel', 0),
                        'rssi': ap.get('rssi', 0),
                        'encryption': ap.get('encryption', ''),
                        'clients': len(ap.get('clients', [])),
                        'vendor': ap.get('vendor', ''),
                    })
            except Exception:
                pass
        aps.sort(key=lambda a: a.get('rssi', -999), reverse=True)

        # Use cached captured MACs; only rescan dirs when stale (after new captures)
        if self._captured_macs_stale:
            self._refresh_captured_macs()

        for ap in aps:
            ap['captured'] = ap.get('mac', '').lower() in self._captured_macs

        return aps

    def on_epoch(self, agent, epoch, epoch_data):
        if self._stopped_permanently:
            return

        # H2: Monkey-patch agent._update_peers to suppress the
        # "'Array' object has no attribute 'read'" AttributeError that fires
        # every epoch from _fetch_stats -> _update_peers -> set_closest_peer.
        # Root cause is a pwngrid API response format mismatch, but patching
        # pwnagotchi core is fragile. Wrapping the method here is safe and
        # survives pwnagotchi updates (since it's applied at runtime).
        if not self._peers_patched and hasattr(agent, '_update_peers'):
            _original_update_peers = agent._update_peers
            def _safe_update_peers():
                try:
                    _original_update_peers()
                except (AttributeError, TypeError) as e:
                    logging.debug("[angryoxide] suppressed _update_peers error: %s", e)
            agent._update_peers = _safe_update_peers
            self._peers_patched = True
            logging.info("[angryoxide] patched agent._update_peers with safe wrapper")

        # In PWN mode, skip all AO-specific epoch logic (health checks,
        # capture scanning, AO face/status overrides). Let bettercap and
        # pwnagotchi core manage the display without AO interference.
        if not self._is_ao_mode():
            return

        # H1: Prevent blind epoch escalation — if AO is running with the
        # monitor interface up, reset blind_for directly. The epoch.observe()
        # call happens BEFORE on_epoch, so by the time we get here blind_for
        # has already been incremented. We correct it here. The dummy AP
        # injection below is a belt-and-suspenders backup.
        if self._running and self._agent:
            try:
                iface = self.options.get('interface', 'wlan0mon')
                if os.path.exists('/sys/class/net/%s' % iface):
                    self._agent._epoch.blind_for = 0
            except Exception:
                pass

        # Prevent blind epoch restart — report AO's AP count to pwnagotchi
        if self._running and self._agent:
            try:
                ao_aps = self._agent._access_points
                if not ao_aps or len(ao_aps) == 0:
                    # If bettercap sees no APs but AO is running, inject a dummy
                    # to prevent blind_for counter from incrementing
                    self._agent._access_points = [{'hostname': 'AO-active', 'mac': '00:00:00:00:00:00', 'channel': 0, 'rssi': 0, 'encryption': '', 'clients': []}]
            except Exception:
                pass

        # Handle deferred restart from non-blocking backoff
        if self._next_restart_time > 0 and time.time() >= self._next_restart_time:
            self._next_restart_time = 0
            self._start_ao(agent)
            return

        if not self._running and os.path.isfile(self.options.get('binary_path', '/usr/local/bin/angryoxide')):
            # try to start if not running yet (e.g. binary was installed after boot)
            self._start_ao(agent)
            return

        # --- Edge case: battery check (highest priority — might shutdown) ---
        battery = self._get_battery_level()
        if battery is not None:
            if battery < 15:
                agent._view.set('face', self._face('battery_critical'))
                agent._view.set('status', 'Battery critical! %d%%' % battery)
                agent._view.update()
                return
            elif battery < 20:
                agent._view.set('face', self._face('battery_low'))
                agent._view.set('status', 'Battery low: %d%%' % battery)
                agent._view.update()
                # don't return — still run AO checks

        # --- Edge case: WiFi interface down ---
        # Only trigger recovery if wlan0 (base interface) is gone.
        # wlan0mon cycles during normal AO restarts — don't react to that.
        iface_dead = not os.path.exists('/sys/class/net/wlan0')
        if iface_dead:
            # Kill zombie AO process if interface is gone
            if self._process and self._process.poll() is None:
                logging.warning("[angryoxide] interface gone but AO still running — killing zombie process")
                self._stop_ao()
                self._running = False
            agent._view.set('face', self._face('wifi_down'))
            agent._view.set('status', 'WiFi down! Recovering...')
            agent._view.update()
            # Attempt GPIO power cycle recovery
            logging.warning("[angryoxide] WiFi interface dead, attempting GPIO recovery")
            recovered = self._try_fw_recovery()
            if recovered:
                agent._view.set('face', self._face('awake'))
                agent._view.set('status', 'WiFi recovered!')
                agent._view.update()
                self._start_ao(agent)
            else:
                agent._view.set('status', 'WiFi down! Recovery failed.')
                agent._view.update()
            return

        # --- AO health check ---
        crashed = self._check_health(agent)
        if crashed:
            # Differentiate firmware crash vs AO crash
            if self._fw_crash_count > 0 and time.time() - self._last_recovery < 120:
                agent._view.set('face', self._face('fw_crash'))
                agent._view.set('status', 'Firmware crashed! Recovering...')
            else:
                agent._view.set('face', self._face('ao_crashed'))
                agent._view.set('status', 'AO crashed! Restart %d/%d' % (
                    self._crash_count, self.options.get('max_crashes', 10)))
            agent._view.update()
            self._captures_this_epoch = 0
            return

        self._captures_this_epoch = self._scan_captures(agent)
        self._stable_epochs += 1

        # Flush debounced state if dirty
        if getattr(self, '_state_dirty', False):
            self._save_state(force=True)

        # In AO-only mode, inject AP data into StubClient for display/epoch
        if getattr(agent, '_ao_mode', False) and hasattr(agent, 'set_stub_aps'):
            stub_aps = []
            for filepath in self._known_files:
                ap_mac, sta_mac = self._parse_capture_filename(os.path.basename(filepath))
                if ap_mac != 'unknown':
                    stub_aps.append({
                        'hostname': '', 'mac': ap_mac, 'vendor': '',
                        'channel': 0, 'rssi': 0, 'encryption': 'WPA2',
                        'clients': [{'mac': sta_mac, 'vendor': ''}] if sta_mac != 'unknown' else [],
                    })
            agent.set_stub_aps(stub_aps)

        # set pwnagotchi mood based on AO activity
        try:
            _view = agent._view
            if self._captures_this_epoch > 0:
                _view.set('face', faces.EXCITED)
                agent.set_status("AO pwnd!")
            elif self._stable_epochs > 30:
                _view.set('face', faces.BORED)
                agent.set_status("AO scanning...")
        except Exception:
            pass

        # reset crash count after 5 minutes of stability
        if self._crash_count > 0 and self._last_crash_time > 0:
            if time.time() - self._last_crash_time > 300:
                logging.info("[angryoxide] stable for 5+ minutes, resetting crash count (was %d)", self._crash_count)
                self._crash_count = 0

    def on_ui_setup(self, ui):
        with ui._lock:
            pos = self.options.get('position', None)
            if pos:
                pos = [int(x.strip()) for x in pos.split(',')]
            else:
                pos = (0, 0)
            # Start with empty value — on_ui_update will populate if in AO mode.
            # This prevents AO indicators from flashing on screen in PWN mode
            # before the first on_ui_update clears them.
            ui.add_element('angryoxide', LabeledValue(
                color=BLACK,
                label='',
                value='',
                position=pos,
                label_font=fonts.Small,
                text_font=fonts.Small
            ))
            # CRASH counter at bottom-left (where PWND was)
            ui.add_element('ao_crash', LabeledValue(
                color=BLACK,
                label='',
                value='',
                position=(0, 109),
                label_font=fonts.Small,
                text_font=fonts.Small
            ))

            # Move AUTO/MANU mode indicator to the very bottom-right corner
            # of the display (250x122). Position it so text ends at the right
            # edge and sits at the bottom of the screen.
            try:
                mode_elem = ui._state._state.get('mode')
                if mode_elem:
                    mode_elem.xy = (222, 112)
            except Exception:
                pass

    def on_ui_update(self, ui):
        with ui._lock:
            # Move mode indicator to bottom-right every update
            try:
                mode_elem = ui._state._state.get('mode')
                if mode_elem:
                    mode_elem.xy = (222, 112)
            except Exception:
                pass

            # In PWN mode, don't show any AO indicators — let bettercap plugins
            # manage their own UI elements (walkby, blitz, etc.)
            if not self._is_ao_mode():
                try:
                    ui.set('angryoxide', '')
                    ui.set('ao_crash', '')
                except Exception:
                    pass
                return

            # AO mode: hide name, bettercap elements, and PWN-mode plugins
            for elem in ('name', 'walkby', 'blitz', 'walkby_status'):
                try:
                    ui.set(elem, '')
                except Exception:
                    pass
            # Move bettercap + PWN-mode plugin elements off-screen (blanking doesn't
            # work — bettercap rewrites them after our plugin runs). Position (300, 300)
            # is off the 250x122 display so they render but are invisible.
            for hide_key in ('shakes', 'channel', 'aps', 'display-password'):
                try:
                    el = ui._state._state.get(hide_key)
                    if el and hasattr(el, 'xy'):
                        el.xy = (300, 300)
                except Exception:
                    pass

            # Show CRASH counter at bottom-left
            try:
                ui.set('ao_crash', 'CRASH:%d' % self._fw_crash_count)
            except Exception:
                pass

            if self._stopped_permanently:
                ui.set('angryoxide', 'AO: ERR')
            elif self._running:
                uptime = self._format_uptime()
                try:
                    output_dir = self.options.get('output_dir', '/etc/pwnagotchi/handshakes/')
                    tot = self._count_pcapngs(output_dir)
                    vrf = self._count_verified(output_dir)
                except Exception:
                    tot, vrf = '?', '?'
                channels = self._channels if self._channels else ('AH' if self._autohunt else '1,6,11')
                # vrf = captures with .22000 (confirmed crackable), tot = all pcapngs on disk
                ui.set('angryoxide', 'AO: %s/%s | %s | CH:%s' % (vrf, tot, uptime, channels))
                # Override status text that bt-tether/other plugins write
                try:
                    cur_status = ui.get('status')
                    if cur_status and ('bnep' in str(cur_status) or 'BT' in str(cur_status)):
                        ui.set('status', 'AO: %s/%s verified | %s' % (vrf, tot, uptime))
                except Exception:
                    pass
            else:
                ui.set('angryoxide', 'AO: off')

    @staticmethod
    def _validate_channels(channels_str):
        """Validate and sanitize channel string. Returns cleaned string or empty."""
        if not channels_str or not isinstance(channels_str, str):
            return ''
        channels_str = channels_str.strip()
        if not channels_str:
            return ''
        parts = [p.strip() for p in channels_str.split(',')]
        valid = []
        for p in parts:
            try:
                ch = int(p)
                if 1 <= ch <= 14:
                    valid.append(str(ch))
            except (ValueError, TypeError):
                continue
        return ','.join(valid)

    @staticmethod
    def _validate_mac_or_ssid(entry):
        """Validate a target/whitelist entry. Must be a MAC (XX:XX:XX:XX:XX:XX) or non-empty SSID string.
        Returns cleaned string or None if invalid."""
        if not entry or not isinstance(entry, str):
            return None
        entry = entry.strip()
        if not entry:
            return None
        # Check if it looks like a MAC address
        mac_pattern = re.compile(r'^([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}$')
        if mac_pattern.match(entry):
            return entry.upper()
        # Otherwise treat as SSID — must be non-empty printable string, max 32 chars
        if len(entry) > 32:
            entry = entry[:32]
        # Strip control characters
        entry = ''.join(c for c in entry if c.isprintable())
        return entry if entry else None

    @staticmethod
    def _validate_discord_webhook(url):
        """Validate discord webhook URL. Must start with https://discord.com/api/webhooks/ or be empty.
        Returns cleaned string or empty."""
        if not url or not isinstance(url, str):
            return ''
        url = url.strip()
        if not url:
            return ''
        if url.startswith('https://discord.com/api/webhooks/'):
            return url
        return ''

    def on_webhook(self, path, request):
        from flask import jsonify, Response

        # Check auth for POST requests if pwnagotchi web auth is enabled
        if request.method == 'POST' and self._agent:
            web_cfg = self._agent._config.get('ui', {}).get('web', {})
            if web_cfg.get('auth', False):
                auth = request.authorization
                if not auth or auth.username != web_cfg.get('username', '') or auth.password != web_cfg.get('password', ''):
                    return jsonify({'error': 'unauthorized'}), 401

        # Normalize path: pwnagotchi may pass None, '', '/', or without leading slash
        if path is None:
            path = ''
        path = '/' + path.strip('/') if path else ''

        # Helper: parse JSON body (flask's get_json can fail after CSRF consumes stream)
        def get_body():
            try:
                data = request.get_json(force=True, silent=True)
                if data:
                    return data
            except Exception:
                pass
            try:
                if request.data:
                    return json.loads(request.data)
            except Exception:
                pass
            return {}

        # ---- API ENDPOINTS ----

        if request.method == 'GET' and path in ('', '/'):
            return Response(self._dashboard_html(), mimetype='text/html')

        if request.method == 'GET' and path == '/api/status':
            uptime_secs = int(time.time() - self._start_time) if self._start_time else None
            # Fetch tether IPs for usb0 and bnep0
            def _get_iface_ip(iface):
                try:
                    out = subprocess.check_output(
                        ['ip', '-4', 'addr', 'show', iface],
                        stderr=subprocess.DEVNULL, timeout=3
                    ).decode()
                    for line in out.splitlines():
                        line = line.strip()
                        if line.startswith('inet '):
                            return line.split()[1].split('/')[0]
                except Exception:
                    pass
                return None
            usb0_ip = _get_iface_ip('usb0')
            bnep0_ip = _get_iface_ip('bnep0')
            # Count cracked passwords from potfile (not from captures)
            cracked_count = 0
            for pf in ['/home/pi/handshakes/wpa-sec.cracked.potfile',
                        '/etc/pwnagotchi/handshakes/wpa-sec.cracked.potfile']:
                try:
                    with open(pf, 'r') as f:
                        cracked_count += sum(1 for line in f if line.strip() and not line.startswith('#'))
                except Exception:
                    pass
            return jsonify({
                'running': self._running,
                'pid': self._process.pid if self._process else None,
                'captures': self._captures,
                'verified_captures': self._count_verified(self.options.get('output_dir', '/etc/pwnagotchi/handshakes/')),
                'total_captures': self._count_pcapngs(self.options.get('output_dir', '/etc/pwnagotchi/handshakes/')),
                'cracked': cracked_count,
                'crash_count': self._crash_count,
                'fw_crash_count': self._fw_crash_count,
                'stopped_permanently': self._stopped_permanently,
                'uptime_secs': uptime_secs,
                'attacks': self._attacks,
                'rate': self._rate,
                'channels': self._channels,
                'autohunt': self._autohunt,
                'dwell': self._dwell,
                'targets': self._targets,
                'whitelist': self._whitelist_entries,
                'skip_captured': self._skip_captured,
                'config_whitelist': self._agent._config.get('main', {}).get('whitelist', []) if self._agent else [],
                'usb0_ip': usb0_ip,
                'bnep0_ip': bnep0_ip,
                'discord_webhook': self._discord_webhook or '',
                # Override cumulative to prevent theme from showing false cracked count
                # (fix_exp.py on Pi injected total_crackable counting .22000 files as "cracked")
                'cumulative': {
                    'total_crackable': cracked_count,
                    'total_handshakes': self._captures,
                    'total_networks': len(self._captured_macs) if hasattr(self, '_captured_macs') else 0,
                    'total_pmkids': 0,
                },
            })

        if request.method == 'GET' and path == '/api/health':
            return jsonify(self._get_health())

        if request.method == 'GET' and path == '/api/aps':
            return jsonify(self._get_access_points())

        if request.method == 'GET' and path == '/api/mode':
            # Cache mode for 30s — subprocess call is expensive on Pi Zero
            now = time.time()
            if self._mode_cache and now - self._mode_cache_time < 30:
                return jsonify(self._mode_cache)
            try:
                r = subprocess.run(['pwnoxide-mode', 'status'], capture_output=True, text=True, timeout=5)
                mode = 'ao' if 'AngryOxide' in r.stdout else 'pwn'
                self._mode_cache = {'mode': mode, 'details': r.stdout.strip()}
                self._mode_cache_time = now
                return jsonify(self._mode_cache)
            except Exception:
                return jsonify({'mode': 'unknown'})

        if request.method == 'GET' and path == '/api/captures':
            # Scan disk directly (not _known_files) so captures survive restarts
            output_dir = self.options.get('output_dir', '/etc/pwnagotchi/handshakes/')
            try:
                handshake_dir = self._agent._config['bettercap']['handshakes'] if self._agent else output_dir
            except (KeyError, TypeError):
                handshake_dir = output_dir
            logging.debug("[angryoxide] captures API: output_dir=%s handshake_dir=%s", output_dir, handshake_dir)
            # Collect from both dirs, dedup by basename
            seen = {}
            for d in [output_dir, handshake_dir]:
                if not os.path.isdir(d):
                    continue
                for fname in os.listdir(d):
                    if fname.endswith(('.pcapng', '.pcap', '.22000')):
                        fpath = os.path.join(d, fname)
                        if fname not in seen:
                            try:
                                seen[fname] = (fpath, os.path.getmtime(fpath))
                            except OSError:
                                pass
            # Filter param: ?filter=verified or ?filter=all (default=all)
            filter_mode = 'all'
            try:
                filter_mode = request.args.get('filter', 'all')
            except Exception:
                pass
            items = sorted(seen.values(), key=lambda x: x[1], reverse=True)
            captures = []
            for f, mt in items:
                fname = os.path.basename(f)
                # Detect type from filename convention
                cap_type = 'unknown'
                if fname.endswith('.22000'):
                    cap_type = 'hashcat'
                elif 'pmkid' in fname.lower():
                    cap_type = 'PMKID'
                elif fname.endswith('.pcapng') or fname.endswith('.pcap'):
                    try:
                        size = os.path.getsize(f)
                        cap_type = 'PMKID' if size < 2048 else '4-way'
                    except Exception:
                        cap_type = 'handshake'
                # Check if hashcat-ready .22000 file exists (= verified crackable)
                verified = False
                if fname.endswith('.22000'):
                    verified = True
                else:
                    hash_name = fname.rsplit('.', 1)[0] + '.22000'
                    verified = os.path.isfile(os.path.join(os.path.dirname(f), hash_name))
                if filter_mode == 'verified' and not verified:
                    continue
                # Extract AP MAC and SSID from .22000 file if available
                ap_mac, ssid = '', ''
                hash_path = os.path.join(os.path.dirname(f), fname.rsplit('.', 1)[0] + '.22000')
                if fname.endswith('.22000'):
                    hash_path = f
                if os.path.isfile(hash_path):
                    ap_mac, ssid = self._parse_22000_file(hash_path) or ('', '')
                    if ap_mac is None:
                        ap_mac = ''
                    if ssid is None:
                        ssid = ''
                captures.append({'file': fname, 'mtime': mt, 'type': cap_type, 'verified': verified, 'ap_mac': ap_mac, 'ssid': ssid})
            logging.debug("[angryoxide] captures API: scanned=%d filter=%s returning=%d", len(seen), filter_mode, len(captures))
            return jsonify(captures[:100])

        if request.method == 'POST' and path == '/api/attacks':
            data = get_body()
            for key in self._attacks:
                if key in data:
                    self._attacks[key] = bool(data[key])
            self._restart_ao()
            logging.info("[angryoxide] attacks updated via web: %s", self._attacks)
            self._save_state()
            return jsonify({'status': 'ok', 'attacks': self._attacks})

        if request.method == 'POST' and path == '/api/pwn-attacks':
            data = get_body()
            if self._agent:
                cfg = self._agent._config
                if 'deauth' in data:
                    self._pwn_deauth = bool(data['deauth'])
                    cfg['personality']['deauth'] = self._pwn_deauth
                if 'associate' in data:
                    self._pwn_associate = bool(data['associate'])
                    cfg['personality']['associate'] = self._pwn_associate
                self._save_state()
                logging.info("[angryoxide] PWN attacks updated: deauth=%s associate=%s",
                             self._pwn_deauth, self._pwn_associate)
                return jsonify({
                    'status': 'ok',
                    'deauth': self._pwn_deauth,
                    'associate': self._pwn_associate,
                })
            return jsonify({'status': 'error', 'message': 'agent not ready'}), 503

        if request.method == 'POST' and path == '/api/rate':
            data = get_body()
            rate = data.get('rate', 2)
            if rate in (1, 2, 3):
                self._rate = rate
                self._restart_ao()
                logging.info("[angryoxide] rate changed to %d via web", self._rate)
            self._save_state()
            return jsonify({'status': 'ok', 'rate': self._rate})

        if request.method == 'POST' and path == '/api/channels':
            data = get_body()
            self._channels = self._validate_channels(data.get('channels', ''))
            self._autohunt = bool(data.get('autohunt', False))
            self._dwell = max(1, min(30, int(data.get('dwell', self._dwell))))
            self._restart_ao()
            logging.info("[angryoxide] channels updated via web: ch=%s autohunt=%s dwell=%d",
                         self._channels, self._autohunt, self._dwell)
            self._save_state()
            return jsonify({'status': 'ok', 'channels': self._channels, 'autohunt': self._autohunt, 'dwell': self._dwell})

        if request.method == 'POST' and path == '/api/targets/add':
            data = get_body()
            target = self._validate_mac_or_ssid(data.get('target', ''))
            if target and target not in self._targets:
                self._targets.append(target)
                self._restart_ao()
                logging.info("[angryoxide] target added via web: %s", target)
            self._save_state()
            return jsonify({'status': 'ok', 'targets': self._targets})

        if request.method == 'POST' and path == '/api/targets/remove':
            data = get_body()
            target = (data.get('target', '') or '').strip()
            if target in self._targets:
                self._targets.remove(target)
                self._restart_ao()
                logging.info("[angryoxide] target removed via web: %s", target)
            self._save_state()
            return jsonify({'status': 'ok', 'targets': self._targets})

        if request.method == 'POST' and path == '/api/whitelist/add':
            data = get_body()
            entry = self._validate_mac_or_ssid(data.get('entry', ''))
            if entry and entry not in self._whitelist_entries:
                self._whitelist_entries.append(entry)
                self._restart_ao()
                logging.info("[angryoxide] whitelist added via web: %s", entry)
            self._save_state()
            return jsonify({'status': 'ok', 'whitelist': self._whitelist_entries})

        if request.method == 'POST' and path == '/api/whitelist/remove':
            data = get_body()
            entry = (data.get('entry', '') or '').strip()
            if entry in self._whitelist_entries:
                self._whitelist_entries.remove(entry)
                self._restart_ao()
                logging.info("[angryoxide] whitelist removed via web: %s", entry)
            self._save_state()
            return jsonify({'status': 'ok', 'whitelist': self._whitelist_entries})

        if request.method == 'POST' and path == '/api/skip-captured':
            data = get_body()
            self._skip_captured = bool(data.get('enabled', False))
            self._save_state()
            self._restart_ao()
            logging.info("[angryoxide] skip-captured set to %s via web", self._skip_captured)
            return jsonify({'status': 'ok', 'skip_captured': self._skip_captured})

        if request.method == 'POST' and path == '/api/restart':
            self._restart_ao()
            logging.info("[angryoxide] restarted via web")
            return jsonify({'status': 'ok', 'message': 'AO restarted'})

        if request.method == 'POST' and path == '/api/stop':
            self._stop_ao()
            logging.info("[angryoxide] stopped via web")
            return jsonify({'status': 'ok', 'message': 'AO stopped'})

        if request.method == 'POST' and path == '/api/reset':
            self._stopped_permanently = False
            self._crash_count = 0
            logging.info("[angryoxide] crash state reset via web")
            return jsonify({'status': 'ok', 'message': 'crash state reset'})

        if request.method == 'POST' and path == '/api/mode':
            data = get_body()
            mode = data.get('mode', '')
            if mode in ('ao', 'pwn'):
                try:
                    # Show intense bull face during mode transition
                    if self._agent:
                        try:
                            self._agent._view.set('face', self._face('intense'))
                            self._agent._view.set('status', 'Switching to %s mode...' % mode.upper())
                            self._agent._view.update(force=True)
                        except Exception:
                            pass
                    # Disable non-essential plugins for faster mode switch restart
                    try:
                        self._save_delayed_plugins()
                    except Exception:
                        pass
                    # Run async — pwnoxide-mode restarts pwnagotchi, which kills this server
                    import threading
                    def _do_switch():
                        time.sleep(1)  # let the HTTP response flush
                        subprocess.run(['pwnoxide-mode', mode], timeout=90, capture_output=True)
                    threading.Thread(target=_do_switch, daemon=True).start()
                    logging.info("[angryoxide] mode switch to %s initiated via web", mode)
                    return jsonify({'status': 'ok', 'mode': mode, 'message': 'Switching... page will reload in ~90s'})
                except Exception as e:
                    return jsonify({'status': 'error', 'message': str(e)}), 500
            return jsonify({'status': 'error', 'message': 'invalid mode'}), 400

        if request.method == 'GET' and path == '/api/cracked':
            # Read cracked passwords from wpa-sec potfile
            # Check both common locations
            potfile_paths = [
                '/home/pi/handshakes/wpa-sec.cracked.potfile',
                '/etc/pwnagotchi/handshakes/wpa-sec.cracked.potfile',
            ]
            results = []
            for pf in potfile_paths:
                try:
                    with open(pf, 'r') as f:
                        for line in f:
                            line = line.strip()
                            if not line or line.startswith('#'):
                                continue
                            # wpa-sec potfile format: BSSID:ESSID:password
                            parts = line.split(':')
                            if len(parts) >= 3:
                                results.append({
                                    'bssid': parts[0],
                                    'ssid': parts[1],
                                    'password': ':'.join(parts[2:]),  # password may contain colons
                                })
                            elif len(parts) == 2:
                                results.append({
                                    'bssid': '',
                                    'ssid': parts[0],
                                    'password': parts[1],
                                })
                except (FileNotFoundError, PermissionError):
                    continue
                except Exception:
                    continue
            return jsonify(results)

        if request.method == 'POST' and path == '/api/discord-webhook':
            data = get_body()
            self._discord_webhook = self._validate_discord_webhook(data.get('url', ''))
            self.options['discord_webhook'] = self._discord_webhook
            self._save_state()
            return jsonify({'status': 'ok'})

        if request.method == 'GET' and path == '/api/bt-visibility':
            try:
                r = subprocess.run(['hciconfig', 'hci0'], capture_output=True, text=True, timeout=3)
                visible = 'PSCAN' in r.stdout and 'ISCAN' in r.stdout
                return jsonify({'visible': visible})
            except Exception:
                return jsonify({'visible': False})

        if request.method == 'POST' and path == '/api/bt-visibility':
            data = get_body()
            visible = bool(data.get('visible', False))
            try:
                mode = 'piscan' if visible else 'noscan'
                subprocess.run(['sudo', 'hciconfig', 'hci0', mode], timeout=5, capture_output=True)
                logging.info("[angryoxide] BT visibility set to %s via web", mode)
                return jsonify({'status': 'ok', 'visible': visible})
            except Exception as e:
                return jsonify({'status': 'error', 'message': str(e)}), 500

        if request.method == 'GET' and path.startswith('/api/download/capture/'):
            filename = path.split('/api/download/capture/', 1)[1]
            # Security: prevent path traversal
            filename = os.path.basename(filename)
            # Check both AO output dir and bettercap handshake dir
            ao_dir = self.options.get('output_dir', '/etc/pwnagotchi/handshakes/')
            bc_dir = self._agent._config['bettercap']['handshakes'] if self._agent else '/home/pi/handshakes'
            for d in [ao_dir, bc_dir]:
                fpath = os.path.join(d, filename)
                if os.path.isfile(fpath):
                    from flask import send_file
                    return send_file(fpath, as_attachment=True)
            return jsonify({'error': 'file not found'}), 404

        if request.method == 'GET' and path == '/api/download/all':
            import zipfile, io
            filter_mode = request.args.get('filter', 'all')
            ao_dir = self.options.get('output_dir', '/etc/pwnagotchi/handshakes/')
            bc_dir = self._agent._config['bettercap']['handshakes'] if self._agent else '/home/pi/handshakes'
            buf = io.BytesIO()
            with zipfile.ZipFile(buf, 'w', zipfile.ZIP_DEFLATED) as zf:
                for d, prefix in [(ao_dir, 'ao'), (bc_dir, 'bettercap')]:
                    if os.path.isdir(d):
                        all_files = os.listdir(d)
                        hash_basenames = set()
                        if filter_mode == 'verified':
                            for f in all_files:
                                if f.endswith('.22000'):
                                    hash_basenames.add(os.path.splitext(f)[0])
                        for f in all_files:
                            fpath = os.path.join(d, f)
                            if not os.path.isfile(fpath):
                                continue
                            if not (f.endswith('.pcapng') or f.endswith('.pcap') or f.endswith('.22000')):
                                continue
                            if filter_mode == 'verified':
                                base = os.path.splitext(f)[0]
                                if f.endswith('.22000') or base in hash_basenames:
                                    zf.write(fpath, f'{prefix}/{f}')
                            else:
                                zf.write(fpath, f'{prefix}/{f}')
            buf.seek(0)
            fname = 'captures_verified.zip' if filter_mode == 'verified' else 'captures.zip'
            return Response(buf.read(), mimetype='application/zip', headers={'Content-Disposition': f'attachment; filename={fname}'})

        if request.method == 'GET' and path == '/api/logs':
            try:
                r = subprocess.run(['journalctl', '-u', 'pwnagotchi', '-n', '30', '--no-pager'],
                                  capture_output=True, text=True, timeout=5)
                lines = [l for l in r.stdout.splitlines() if 'angryoxide' in l.lower() or 'ao' in l.lower() or 'angry' in l.lower()]
                if not lines:
                    lines = r.stdout.splitlines()[-20:]
                return jsonify({'lines': lines[-20:]})
            except Exception:
                return jsonify({'lines': ['Could not read logs']})

        if request.method == 'GET' and path == '/api/plugins-list':
            # Return list of all plugins with status
            plugin_list = []
            try:
                import pwnagotchi.plugins as _plugins
                for name, plugin in _plugins.loaded.items():
                    if plugin is not None:
                        plugin_list.append({
                            'name': name,
                            'enabled': True,
                            'version': getattr(plugin, '__version__', '?'),
                            'author': getattr(plugin, '__author__', '?'),
                            'description': getattr(plugin, '__description__', ''),
                            'has_webhook': hasattr(plugin, 'on_webhook'),
                        })
                for name in _plugins.database:
                    if name not in _plugins.loaded or _plugins.loaded[name] is None:
                        plugin_list.append({
                            'name': name,
                            'enabled': False,
                            'version': '?',
                            'author': '?',
                            'description': '',
                            'has_webhook': False,
                        })
            except Exception:
                pass
            plugin_list.sort(key=lambda p: (not p['enabled'], p['name']))
            return jsonify(plugin_list)

        if request.method == 'GET' and path == '/api/config':
            if not self._agent:
                return jsonify({'error': 'agent not ready'}), 503
            # Return relevant config sections
            cfg = self._agent._config
            return jsonify({
                'main': {
                    'name': cfg.get('main', {}).get('name', ''),
                    'lang': cfg.get('main', {}).get('lang', 'en'),
                    'iface': cfg.get('main', {}).get('iface', 'wlan0mon'),
                    'whitelist': cfg.get('main', {}).get('whitelist', []),
                },
                'personality': {
                    'recon_time': cfg.get('personality', {}).get('recon_time', 10),
                    'hop_recon_time': cfg.get('personality', {}).get('hop_recon_time', 5),
                    'min_recon_time': cfg.get('personality', {}).get('min_recon_time', 3),
                    'deauth': cfg.get('personality', {}).get('deauth', True),
                    'associate': cfg.get('personality', {}).get('associate', True),
                    'channels': cfg.get('personality', {}).get('channels', []),
                    'min_rssi': cfg.get('personality', {}).get('min_rssi', -200),
                    'ap_ttl': cfg.get('personality', {}).get('ap_ttl', 120),
                    'sta_ttl': cfg.get('personality', {}).get('sta_ttl', 300),
                    'excited_num_epochs': cfg.get('personality', {}).get('excited_num_epochs', 10),
                    'bored_num_epochs': cfg.get('personality', {}).get('bored_num_epochs', 15),
                    'sad_num_epochs': cfg.get('personality', {}).get('sad_num_epochs', 25),
                },
                'ui': {
                    'invert': cfg.get('ui', {}).get('invert', False),
                    'fps': cfg.get('ui', {}).get('fps', 0.0),
                    'display_type': cfg.get('ui', {}).get('display', {}).get('type', ''),
                    'display_rotation': cfg.get('ui', {}).get('display', {}).get('rotation', 0),
                },
                'web': {
                    'enabled': cfg.get('ui', {}).get('web', {}).get('enabled', True),
                    'port': cfg.get('ui', {}).get('web', {}).get('port', 8080),
                    'auth': cfg.get('ui', {}).get('web', {}).get('auth', False),
                },
            })

        if request.method == 'POST' and path == '/api/config':
            data = get_body()
            if not data:
                return jsonify({'error': 'no data'}), 400
            # Write changes to a conf.d overlay file (never touch config.toml directly)
            overlay_path = '/etc/pwnagotchi/conf.d/user-settings.toml'
            try:
                # Load existing overlay or start fresh
                existing = {}
                if os.path.isfile(overlay_path):
                    try:
                        import toml
                        with open(overlay_path, 'r') as f:
                            existing = toml.load(f)
                    except Exception:
                        existing = {}
                # Merge changes
                for section, values in data.items():
                    if section not in existing:
                        existing[section] = {}
                    existing[section].update(values)
                # Write using simple TOML writer since toml module may not be available
                def _escape_toml_string(s):
                    return s.replace('\\', '\\\\').replace('"', '\\"').replace('\n', '\\n').replace('\r', '\\r')

                lines = ['# Oxigotchi user settings (managed by web dashboard)', '']
                for section, values in existing.items():
                    lines.append('[%s]' % _escape_toml_string(str(section)))
                    for k, v in values.items():
                        if isinstance(v, bool):
                            lines.append('%s = %s' % (_escape_toml_string(str(k)), 'true' if v else 'false'))
                        elif isinstance(v, (int, float)):
                            lines.append('%s = %s' % (_escape_toml_string(str(k)), v))
                        elif isinstance(v, list):
                            items = []
                            for item in v:
                                if isinstance(item, bool):
                                    items.append('true' if item else 'false')
                                elif isinstance(item, (int, float)):
                                    items.append(str(item))
                                else:
                                    items.append('"%s"' % _escape_toml_string(str(item)))
                            lines.append('%s = [%s]' % (_escape_toml_string(str(k)), ', '.join(items)))
                        else:
                            lines.append('%s = "%s"' % (_escape_toml_string(str(k)), _escape_toml_string(str(v))))
                    lines.append('')
                with open(overlay_path, 'w') as f:
                    f.write('\\n'.join(lines))
                logging.info("[angryoxide] config saved via web dashboard")
                return jsonify({'status': 'ok', 'message': 'Saved. Restart oxigotchi to apply.'})
            except Exception as e:
                return jsonify({'status': 'error', 'message': str(e)}), 500

        if request.method == 'POST' and path == '/api/shutdown-pi':
            logging.info("[angryoxide] Pi shutdown requested via web")
            import threading
            threading.Timer(2.0, lambda: os.system('sudo shutdown -h now')).start()
            return jsonify({'status': 'ok', 'message': 'Shutting down in 2 seconds...'})

        if request.method == 'POST' and path == '/api/restart-pi':
            logging.info("[angryoxide] Pi restart requested via web")
            import threading
            threading.Timer(2.0, lambda: os.system('sudo reboot')).start()
            return jsonify({'status': 'ok', 'message': 'Restarting in 2 seconds...'})

        if request.method == 'POST' and path == '/api/restart-ssh':
            logging.info("[angryoxide] SSH restart requested via web")
            try:
                subprocess.run(['sudo', 'systemctl', 'restart', 'ssh'], capture_output=True, text=True, timeout=10, check=True)
                return jsonify({'status': 'ok', 'message': 'SSH restarted'})
            except subprocess.CalledProcessError as e:
                logging.error("[angryoxide] SSH restart failed: %s", e.stderr)
                return jsonify({'status': 'error', 'message': 'SSH restart failed: ' + (e.stderr or str(e))}), 500
            except Exception as e:
                logging.error("[angryoxide] SSH restart error: %s", e)
                return jsonify({'status': 'error', 'message': str(e)}), 500

        return jsonify({'error': 'not found'}), 404

    def _dashboard_html(self):
        return '''<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0, user-scalable=no">
<title>Oxigotchi Dashboard</title>
<style>
*{box-sizing:border-box;margin:0;padding:0}
body{background:#1a1a2e;color:#e0e0e0;font-family:'SF Mono','Fira Code','Cascadia Code',monospace;font-size:14px;padding:12px;max-width:600px;margin:0 auto;-webkit-tap-highlight-color:transparent}
h1{color:#00d4aa;font-size:20px;text-align:center;margin-bottom:16px;letter-spacing:1px}
.card{background:#16213e;border-radius:12px;padding:16px;margin-bottom:12px}
.card-title{color:#00d4aa;font-size:15px;font-weight:bold;margin-bottom:12px;padding-bottom:8px;border-bottom:1px solid #0f3460}
.health-row{display:flex;flex-wrap:wrap;gap:10px;margin-bottom:4px}
.health-item{display:flex;align-items:center;gap:6px;font-size:13px}
.dot{width:10px;height:10px;border-radius:50%;display:inline-block;flex-shrink:0}
.dot-green{background:#00d4aa}
.dot-red{background:#e94560}
.dot-gray{background:#555}
.dot-yellow{background:#f0c040}
.status-grid{display:grid;grid-template-columns:1fr 1fr;gap:6px 16px}
.status-grid .label{color:#888;font-size:12px}
.status-grid .value{color:#e0e0e0;font-size:13px;font-weight:bold}
.toggle-row{display:flex;align-items:center;justify-content:space-between;padding:10px 0;border-bottom:1px solid #0f3460}
.toggle-row:last-child{border-bottom:none}
.toggle-info{flex:1;margin-right:12px}
.toggle-label{font-size:14px;font-weight:bold;color:#e0e0e0}
.toggle-desc{font-size:11px;color:#888;margin-top:2px;line-height:1.3}
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
.switch-risky .slider{background:#e67e22 !important}
.switch-risky .slider:before{background:#fff !important}
.rate-btn:active{transform:scale(0.95)}
.channel-row{display:flex;align-items:center;gap:10px;margin-top:10px}
.channel-row label{font-size:13px;color:#888;white-space:nowrap}
.channel-row input[type=text]{flex:1;background:#1a1a2e;border:1px solid #0f3460;border-radius:6px;padding:8px 10px;color:#e0e0e0;font-family:inherit;font-size:13px}
.dwell-row{display:flex;align-items:center;gap:10px;margin-top:10px}
.dwell-row label{font-size:13px;color:#888;white-space:nowrap}
.dwell-row input[type=range]{flex:1;accent-color:#00d4aa}
.dwell-val{color:#00d4aa;font-weight:bold;min-width:30px;text-align:right}
.list-section{margin-top:8px}
.list-input-row{display:flex;gap:8px;margin-bottom:8px}
.list-input-row input{flex:1;background:#1a1a2e;border:1px solid #0f3460;border-radius:6px;padding:8px 10px;color:#e0e0e0;font-family:inherit;font-size:13px}
.list-input-row button{background:#0f3460;color:#00d4aa;border:none;border-radius:6px;padding:8px 14px;font-family:inherit;font-size:13px;cursor:pointer;font-weight:bold}
.list-item{display:flex;align-items:center;justify-content:space-between;padding:6px 10px;background:#1a1a2e;border-radius:6px;margin-bottom:4px;font-size:13px}
.list-item .remove-btn{background:#e94560;color:#fff;border:none;border-radius:4px;padding:4px 10px;font-size:11px;cursor:pointer;font-family:inherit}
.action-btns{display:flex;flex-wrap:wrap;gap:8px}
.action-btn{flex:1;min-width:100px;padding:14px 8px;border:none;border-radius:10px;font-family:inherit;font-size:13px;font-weight:bold;cursor:pointer;text-align:center;transition:.2s}
.action-btn:active{transform:scale(0.95)}
.btn-restart{background:#0f3460;color:#00d4aa}
.btn-stop{background:#e94560;color:#fff}
.btn-reset{background:#333;color:#f0c040}
.mode-btns{display:flex;gap:8px;margin-top:8px}
.mode-btn{flex:1;padding:14px 0;border:2px solid #0f3460;border-radius:10px;background:transparent;color:#e0e0e0;font-size:16px;font-weight:bold;font-family:inherit;cursor:pointer;text-align:center;transition:.2s}
.mode-btn.active{background:#00d4aa;color:#1a1a2e;border-color:#00d4aa}
.mode-btn:active{transform:scale(0.95)}
.captures-list{max-height:200px;overflow-y:auto;margin-top:8px}
.capture-item{font-size:12px;color:#aaa;padding:4px 0;border-bottom:1px solid #0f346033;word-break:break-all}
.capture-item:last-child{border-bottom:none}
.toast{position:fixed;bottom:20px;left:50%;transform:translateX(-50%);background:#00d4aa;color:#1a1a2e;padding:10px 20px;border-radius:8px;font-size:13px;font-weight:bold;opacity:0;transition:opacity .3s;pointer-events:none;z-index:999}
.toast.show{opacity:1}
.autohunt-row{display:flex;align-items:center;gap:10px;margin-top:10px}
.autohunt-row label{font-size:13px;color:#888}
.stopped-banner{background:#e94560;color:#fff;text-align:center;padding:10px;border-radius:8px;margin-bottom:12px;font-weight:bold;font-size:13px}
</style>
</head>
<body>
<h1>Oxigotchi Control Panel</h1>
<div style="text-align:center;color:#888;font-size:11px;margin:-12px 0 14px">Oxigotchi v2 &mdash; WiFi attack engine</div>
<div id="stopped-banner" class="stopped-banner" style="display:none">AO STOPPED - Max crashes reached. Hit "Reset Crashes" below to retry.</div>

<div class="card">
<div class="card-title">System Health</div>
<div style="color:#888;font-size:11px;margin-bottom:8px">Green = OK, Red = problem. USB0 is your SSH lifeline.</div>
<div class="health-row" id="health-row">
<div class="health-item"><span class="dot dot-gray" id="h-wifi"></span>WiFi</div>
<div class="health-item"><span class="dot dot-gray" id="h-monitor"></span>Monitor</div>
<div class="health-item"><span class="dot dot-gray" id="h-firmware"></span>Firmware</div>
<div class="health-item"><span class="dot dot-gray" id="h-usb0"></span>USB0</div>
<div class="health-item" id="h-battery-wrap" style="display:none"><span class="dot dot-gray" id="h-battery-dot"></span>Bat: <span id="h-battery-val">--</span>%</div>
</div>
</div>

<div class="card">
<div class="card-title">Status</div>
<div style="color:#888;font-size:11px;margin-bottom:8px">Live stats, auto-refreshes every 5 seconds.</div>
<div class="status-grid">
<div class="label">State</div><div class="value" id="s-state">--</div>
<div class="label">PID</div><div class="value" id="s-pid">--</div>
<div class="label">Uptime</div><div class="value" id="s-uptime">--</div>
<div class="label">Verified / Total</div><div class="value" id="s-captures">--</div>
<div class="label">Crashes</div><div class="value" id="s-crashes">--</div>
<div class="label">FW Crashes</div><div class="value" id="s-fwcrashes">--</div>
<div class="label">USB Tether</div><div class="value" id="s-usb0-ip">--</div>
<div class="label">BT Tether</div><div class="value" id="s-bnep0-ip">--</div>
</div>
</div>

<div class="card" style="text-align:center">
<div class="card-title">Live Display</div>
<img id="eink-preview" src="/ui" style="width:100%;max-width:250px;image-rendering:pixelated;border:1px solid #0f3460;border-radius:4px;background:#000" alt="e-ink display">
<div style="color:#555;font-size:10px;margin-top:4px">Refreshes every 3 seconds</div>
</div>

<div class="card" id="nearby-networks-card" style="display:none">
<div class="card-title">Nearby Networks <span id="ap-count" style="color:#888;font-size:12px;font-weight:normal"></span></div>
<div style="color:#888;font-size:11px;margin-bottom:8px">Access points discovered by bettercap. This card is only available in PWN mode &#8212; AngryOxide does its own target discovery internally and doesn&#39;t expose a live AP list.</div>
<div style="overflow-x:auto">
<table id="ap-table" style="width:100%;border-collapse:collapse;font-size:11px">
<thead><tr style="color:#00d4aa;border-bottom:1px solid #0f3460;text-align:left">
<th style="padding:4px 6px">SSID</th>
<th style="padding:4px 4px">CH</th>
<th style="padding:4px 4px">dBm</th>
<th style="padding:4px 4px">Enc</th>
<th style="padding:4px 4px">Dev</th>
<th style="padding:4px 4px">PWN</th>
<th style="padding:4px 4px" title="Target = focus attacks on this network">ATK</th>
<th style="padding:4px 4px" title="Protect = never attack this network">WL</th>
</tr></thead>
<tbody id="ap-tbody"><tr><td colspan="8" style="color:#555;padding:8px">Scanning...</td></tr></tbody>
</table>
</div>
</div>

<div class="card">
<div class="card-title">Recent Captures</div>
<div style="color:#888;font-size:11px;margin-bottom:8px">AO validates every capture before saving &#8212; no junk pcaps. &#10003; verified = .22000 hashcat-ready hash exists. Click to download individual files or use Download All for a ZIP.</div>
<div style="display:flex;gap:8px;margin-bottom:8px;align-items:center">
<a href="" id="dl-all-btn" class="action-btn btn-restart" style="text-decoration:none;text-align:center;padding:8px;font-size:11px;flex:1">Download All (ZIP)</a>
<a href="" id="dl-verified-btn" class="action-btn btn-restart" style="text-decoration:none;text-align:center;padding:8px;font-size:11px;flex:1">Download Verified (ZIP)</a>
<select id="capture-filter" onchange="refreshCaptures()" style="background:#1a1a2e;color:#e0e0e0;border:1px solid #333;border-radius:4px;padding:6px 8px;font-size:11px;font-family:inherit">
<option value="all">Show All</option>
<option value="verified">Verified Only</option>
</select>
</div>
<div class="captures-list" id="captures-list"><div style="color:#555;font-size:12px">Loading...</div></div>
</div>

<div class="card">
<div class="card-title">Cracked Passwords</div>
<div style="color:#888;font-size:11px;margin-bottom:8px">Passwords cracked from captured handshakes. Updated when new results are available.</div>
<div id="cracked-list"><div style="color:#555;font-size:12px">No cracked passwords yet</div></div>
</div>

<div class="card" id="pwn-attacks-card" style="display:none">
<div class="card-title">Bettercap Attacks</div>
<div style="color:#888;font-size:11px;margin-bottom:8px">Bettercap&#39;s built-in attack methods. These are the only 2 attack types available in PWN mode. Changes apply immediately &#8212; no restart needed.</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Deauth</div><div class="toggle-desc">Kick clients off networks to capture reconnection handshakes</div></div>
<label class="switch"><input type="checkbox" id="pwn-deauth" onchange="togglePwnAttack('deauth',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row" style="border-bottom:none">
<div class="toggle-info"><div class="toggle-label">Associate</div><div class="toggle-desc">Send association frames to capture PMKID hashes from routers</div></div>
<label class="switch"><input type="checkbox" id="pwn-associate" onchange="togglePwnAttack('associate',this.checked)"><span class="slider"></span></label>
</div>
</div>

<div class="card" id="ao-attacks-card">
<div class="card-title">Attack Types</div>
<div style="color:#00d4aa;font-size:11px;margin-bottom:10px;padding:8px;background:#0f346033;border-radius:6px">&#9889; All 6 ON is the sweet spot &#8212; they complement each other, not interfere. Only turn one off if you have a specific reason (stealth, debugging, fragile target).</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Deauth</div><div class="toggle-desc">Kick clients off networks to capture reconnection handshakes</div></div>
<label class="switch"><input type="checkbox" id="atk-deauth" checked onchange="toggleAttack('deauth',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">PMKID</div><div class="toggle-desc">Grab router password hashes without any clients needed</div></div>
<label class="switch"><input type="checkbox" id="atk-pmkid" checked onchange="toggleAttack('pmkid',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">CSA</div><div class="toggle-desc">Trick clients into switching channels &#8212; stealthier than deauth</div></div>
<label class="switch"><input type="checkbox" id="atk-csa" checked onchange="toggleAttack('csa',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Disassociation</div><div class="toggle-desc">Another way to disconnect clients &#8212; catches some that resist deauth</div></div>
<label class="switch"><input type="checkbox" id="atk-disassoc" checked onchange="toggleAttack('disassoc',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Anon Reassoc</div><div class="toggle-desc">Capture PMKID from routers that reject normal connections</div></div>
<label class="switch"><input type="checkbox" id="atk-anon_reassoc" checked onchange="toggleAttack('anon_reassoc',this.checked)"><span class="slider"></span></label>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Rogue M2</div><div class="toggle-desc">Fake access point trick &#8212; captures handshakes without touching the real router</div></div>
<label class="switch"><input type="checkbox" id="atk-rogue_m2" checked onchange="toggleAttack('rogue_m2',this.checked)"><span class="slider"></span></label>
</div>
</div>

<div class="card ao-only-card">
<div class="card-title">Smart Skip</div>
<div style="color:#888;font-size:11px;margin-bottom:8px">When enabled, AO will skip networks you already have handshakes for. Saves time and focuses on new targets.</div>
<div class="toggle-row" style="border-bottom:none">
<div class="toggle-info"><div class="toggle-label">Skip Already Captured</div><div class="toggle-desc">Auto-whitelist APs that already have .pcapng/.pcap files</div></div>
<label class="switch"><input type="checkbox" id="skip-captured" onchange="toggleSkipCaptured(this.checked)"><span class="slider"></span></label>
</div>
</div>

<div class="card ao-only-card">
<div class="card-title">Attack Rate</div>
<div style="color:#888;font-size:11px;margin-bottom:8px">Attack frame injection speed. Rate 1 is the maximum safe rate for BCM43436B0 — higher rates cause firmware crashes. Do not increase above 1.</div>
<div class="rate-btns">
<button class="rate-btn" id="rate-1" onclick="setRate(1)">1<br><span style="font-size:10px;font-weight:normal;color:#888">Quiet</span><br><span style="font-size:9px;font-weight:normal;color:#555">Low profile, fewer frames</span></button>
<button class="rate-btn risky" id="rate-2" onclick="setRate(2)">2<br><span style="font-size:10px;font-weight:normal">Normal</span><br><span style="font-size:9px;font-weight:normal">&#9888; May crash built-in WiFi</span></button>
<button class="rate-btn risky" id="rate-3" onclick="setRate(3)">3<br><span style="font-size:10px;font-weight:normal">Aggressive</span><br><span style="font-size:9px;font-weight:normal">&#9888; Will crash built-in WiFi</span></button>
</div>
</div>

<div class="card ao-only-card">
<div class="card-title">Channels</div>
<div style="color:#888;font-size:11px;margin-bottom:4px">Which WiFi channels to scan. Leave empty for default (1,6,11). Autohunt scans all channels then locks onto ones with targets. Dwell = how long to stay on each channel.</div>
<div style="color:#e67e22;font-size:11px;margin-bottom:4px">&#9888; <b>Built-in WiFi warning:</b> Scanning many channels (especially all 13) increases firmware crash risk on the BCM43436B0 chip. Stick to 1,6,11 for stability. External dongles (e.g. Alfa) are not affected.</div>
<div class="channel-row">
<label>Channels:</label>
<input type="text" id="ch-input" placeholder="e.g. 1,6,11 (empty=default)">
</div>
<div class="autohunt-row">
<label>Autohunt:</label>
<label class="switch switch-risky"><input type="checkbox" id="ch-autohunt" onchange="applyChannels()"><span class="slider"></span></label>
<span style="font-size:11px;color:#e67e22;margin-left:4px">&#9888; Scans ALL channels first — will likely crash built-in WiFi. Safe with external dongle only.</span>
</div>
<div class="dwell-row">
<label>Dwell:</label>
<input type="range" id="ch-dwell" min="1" max="30" value="2" oninput="document.getElementById('dwell-val').textContent=this.value+'s'">
<span class="dwell-val" id="dwell-val">2s</span>
</div>
<div style="color:#e67e22;font-size:11px;margin-top:6px">&#9888; <b>Dwell warning:</b> Very fast hopping (1-2s) across many channels stresses the BCM43436B0 firmware and can cause crashes. This is a hardware limitation that cannot be patched further. For stable operation, use channels 1,6,11 with dwell 2s+. If you need fast hopping across all channels, use an external USB dongle (e.g. Alfa AWUS036ACH).</div>
<button class="action-btn btn-restart" style="margin-top:10px;width:100%" onclick="applyChannels()">Apply Channel Settings</button>
</div>

<div class="card">
<div class="card-title">Targets</div>
<div style="color:#888;font-size:11px;margin-bottom:8px">Only attack specific networks. Leave empty to attack everything in range (default). Add a MAC address (AA:BB:CC:DD:EE:FF) or SSID name.</div>
<div class="list-section">
<div class="list-input-row">
<input type="text" id="target-input" placeholder="MAC or SSID">
<button onclick="addTarget()">Add</button>
</div>
<div id="target-list"></div>
</div>
</div>

<div class="card">
<div class="card-title">Whitelist <span id="wl-count" style="color:#888;font-size:12px;font-weight:normal"></span></div>
<div style="color:#888;font-size:11px;margin-bottom:8px">Networks that will NEVER be attacked. Add your home WiFi, phone hotspot, etc. Changes take effect immediately.</div>
<div class="list-input-row" style="margin-bottom:10px">
<input type="text" id="wl-input" placeholder="MAC or SSID to protect">
<button onclick="addWhitelist()">Add</button>
</div>
<div style="overflow-x:auto">
<table id="wl-table" style="width:100%;border-collapse:collapse;font-size:12px">
<thead><tr style="color:#00d4aa;border-bottom:1px solid #0f3460;text-align:left">
<th style="padding:4px 6px">Network / MAC</th>
<th style="padding:4px 6px">Source</th>
<th style="padding:4px 6px;width:50px">Action</th>
</tr></thead>
<tbody id="wl-tbody"><tr><td colspan="3" style="color:#555;padding:8px">Loading...</td></tr></tbody>
</table>
</div>
</div>

<div class="card">
<div class="card-title">Controls</div>

<div style="font-size:12px;color:#888;margin-bottom:4px">Mode</div>
<div style="color:#888;font-size:11px;margin-bottom:8px">AO Mode = AngryOxide attacks + bull faces. PWN Mode = stock bettercap. Switching takes ~90s.</div>
<div class="mode-btns">
<button class="mode-btn" id="mode-ao" onclick="switchMode('ao')">AO Mode</button>
<button class="mode-btn" id="mode-pwn" onclick="switchMode('pwn')">PWN Mode</button>
</div>

<div style="margin-top:12px;padding-top:10px;border-top:1px solid #0f3460">
<div style="font-size:12px;color:#888;margin-bottom:4px">Bluetooth Visibility</div>
<div style="color:#888;font-size:11px;margin-bottom:8px">Turn OFF when leaving unattended &#8212; this image uses auto-pair.</div>
<div class="toggle-row" style="border-bottom:none">
<div class="toggle-info"><div class="toggle-label">BT Visible</div><div class="toggle-desc">When ON, any nearby device can discover and pair with the Pi. Turn OFF in public.</div></div>
<label class="switch"><input type="checkbox" id="bt-visible" onchange="toggleBTVisible(this.checked)"><span class="slider"></span></label>
</div>
<div id="bt-status" style="font-size:11px;color:#555;margin-top:4px"></div>
</div>

<div style="margin-top:12px;padding-top:10px;border-top:1px solid #0f3460">
<div style="font-size:12px;color:#888;margin-bottom:4px">Actions</div>
<div style="color:#888;font-size:11px;margin-bottom:8px">Restart applies config changes. Reset clears crash counter.</div>
<div class="action-btns">
<button class="action-btn btn-restart" onclick="doAction('restart')">Restart AO</button>
<button class="action-btn btn-stop" onclick="doAction('stop')">Stop AO</button>
<button class="action-btn btn-reset" onclick="doAction('reset')">Reset Crashes</button>
</div>
<div style="margin-top:8px;display:flex;gap:8px">
<button class="action-btn" style="flex:1;background:#e94560;color:#fff" onclick="if(confirm('Shut down the Pi?'))doAction('shutdown-pi')">Shutdown Pi</button>
<button class="action-btn" style="flex:1;background:#f0c040;color:#1a1a2e" onclick="if(confirm('Restart the Pi?'))doAction('restart-pi')">Restart Pi</button>
</div>
<div style="margin-top:8px;display:flex;gap:8px">
<button class="action-btn btn-restart" style="flex:1" onclick="doAction('restart-ssh')">Restart SSH</button>
</div>
</div>

<div style="margin-top:12px;padding-top:10px;border-top:1px solid #0f3460">
<div style="font-size:12px;color:#888;margin-bottom:4px">Discord Notifications</div>
<div class="list-input-row">
<input type="text" id="discord-webhook" placeholder="Discord webhook URL" style="font-size:11px">
<button onclick="saveDiscordWebhook()" style="font-size:11px">Save</button>
</div>
</div>
</div>

<div class="card">
<div class="card-title" onclick="document.getElementById('logs-panel').style.display=document.getElementById('logs-panel').style.display==='none'?'block':'none'" style="cursor:pointer">AO Logs &#9656;</div>
<div id="logs-panel" style="display:none">
<div style="color:#888;font-size:11px;margin-bottom:8px">Recent AngryOxide log entries. Auto-refreshes every 30 seconds.</div>
<div id="log-viewer" style="max-height:250px;overflow-y:auto;font-family:monospace;font-size:10px;color:#888;background:#111;border-radius:6px;padding:8px"></div>
</div>
</div>

<div class="card">
<div class="card-title" onclick="document.getElementById('config-panel').style.display=document.getElementById('config-panel').style.display==='none'?'block':'none'" style="cursor:pointer">Settings &#9656;</div>
<div id="config-panel" style="display:none">
<div style="color:#888;font-size:11px;margin-bottom:10px">Oxigotchi configuration. Changes save to an overlay file and take effect after restart.</div>

<div style="font-size:13px;color:#00d4aa;font-weight:bold;margin:12px 0 6px">General</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Name</div></div>
<input type="text" id="cfg-name" style="background:#1a1a2e;border:1px solid #0f3460;border-radius:6px;padding:6px 10px;color:#e0e0e0;font-family:inherit;font-size:13px;width:120px">
</div>

<div style="font-size:13px;color:#00d4aa;font-weight:bold;margin:12px 0 6px">Personality</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Recon Time</div><div class="toggle-desc">Seconds per channel scan sweep</div></div>
<div style="display:flex;align-items:center;gap:6px"><input type="range" id="cfg-recon-time" min="5" max="60" value="15" oninput="document.getElementById('cfg-recon-val').textContent=this.value+'s'" style="accent-color:#00d4aa"><span id="cfg-recon-val" style="color:#00d4aa;font-weight:bold;min-width:30px">15s</span></div>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Min RSSI</div><div class="toggle-desc">Ignore APs weaker than this (dBm)</div></div>
<div style="display:flex;align-items:center;gap:6px"><input type="range" id="cfg-min-rssi" min="-200" max="-30" value="-200" oninput="document.getElementById('cfg-rssi-val').textContent=this.value" style="accent-color:#00d4aa"><span id="cfg-rssi-val" style="color:#00d4aa;font-weight:bold;min-width:40px">-200</span></div>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">AP TTL</div><div class="toggle-desc">Seconds before forgetting an AP</div></div>
<div style="display:flex;align-items:center;gap:6px"><input type="range" id="cfg-ap-ttl" min="30" max="600" value="120" oninput="document.getElementById('cfg-ttl-val').textContent=this.value+'s'" style="accent-color:#00d4aa"><span id="cfg-ttl-val" style="color:#00d4aa;font-weight:bold;min-width:40px">120s</span></div>
</div>

<div style="font-size:13px;color:#00d4aa;font-weight:bold;margin:12px 0 6px">Display</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Invert Display</div><div class="toggle-desc">White on black (recommended for e-ink)</div></div>
<label class="switch"><input type="checkbox" id="cfg-invert"><span class="slider"></span></label>
</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Rotation</div></div>
<select id="cfg-rotation" style="background:#1a1a2e;border:1px solid #0f3460;border-radius:6px;padding:6px 10px;color:#e0e0e0;font-family:inherit;font-size:13px">
<option value="0">0</option><option value="90">90</option><option value="180">180</option><option value="270">270</option>
</select>
</div>

<div style="font-size:13px;color:#00d4aa;font-weight:bold;margin:12px 0 6px">Web UI</div>
<div class="toggle-row">
<div class="toggle-info"><div class="toggle-label">Authentication</div><div class="toggle-desc">Require login for web UI</div></div>
<label class="switch"><input type="checkbox" id="cfg-auth"><span class="slider"></span></label>
</div>

<div style="margin-top:16px;display:flex;gap:8px">
<button class="action-btn btn-restart" style="flex:1" onclick="saveConfig()">Save Settings</button>
<button class="action-btn btn-reset" style="flex:1" onclick="loadConfig()">Reset</button>
</div>
<div id="cfg-status" style="font-size:11px;color:#555;margin-top:6px;text-align:center"></div>
</div>
</div>

<div class="card">
<div class="card-title" onclick="document.getElementById('plugins-panel').style.display=document.getElementById('plugins-panel').style.display==='none'?'block':'none'" style="cursor:pointer">Installed Plugins &#9656;</div>
<div id="plugins-panel" style="display:none">
<div style="color:#888;font-size:11px;margin-bottom:8px">All installed plugins. Green = enabled and running.</div>
<div id="plugins-list"></div>
</div>
</div>

<div class="toast" id="toast"></div>

<script>
var BASE = location.pathname.replace(/\\/+$/,'');
function api(method, path, body) {
    var opts = {method: method, headers: {'Content-Type':'application/json'}};
    if (body) opts.body = JSON.stringify(body);
    return fetch(BASE + path, opts).then(function(r){return r.json()}).catch(function(e){console.error('API error:',path,e)});
}
function toast(msg) {
    var t = document.getElementById('toast');
    t.textContent = msg;
    t.classList.add('show');
    setTimeout(function(){t.classList.remove('show')}, 1500);
}
function toggleAttack(name, val) {
    var data = {};
    data[name] = val;
    api('POST', '/api/attacks', data).then(function(r){
        toast('Attack ' + name + (val ? ' ON' : ' OFF'));
    });
}
function togglePwnAttack(name, val) {
    var data = {};
    data[name] = val;
    api('POST', '/api/pwn-attacks', data).then(function(r){
        toast(name + (val ? ' ON' : ' OFF'));
    });
}
function setRate(r) {
    api('POST', '/api/rate', {rate: r}).then(function(res){
        [1,2,3].forEach(function(n){
            document.getElementById('rate-'+n).classList.toggle('active', n===res.rate);
        });
        toast('Rate set to ' + res.rate);
    });
}
function applyChannels() {
    api('POST', '/api/channels', {
        channels: document.getElementById('ch-input').value,
        autohunt: document.getElementById('ch-autohunt').checked,
        dwell: parseInt(document.getElementById('ch-dwell').value)
    }).then(function(res){
        toast('Channels updated');
    });
}
function addTarget() {
    var inp = document.getElementById('target-input');
    var v = inp.value.trim();
    if (!v) return;
    api('POST', '/api/targets/add', {target: v}).then(function(res){
        inp.value = '';
        renderTargets(res.targets);
        toast('Target added');
    });
}
function removeTarget(t) {
    api('POST', '/api/targets/remove', {target: t}).then(function(res){
        renderTargets(res.targets);
        toast('Target removed');
    });
}
function addTargetFromAP(mac) {
    api('POST', '/api/targets/add', {target: mac}).then(function(res){
        renderTargets(res.targets);
        toast('Target: ' + mac);
    });
}
function whitelistFromAP(name) {
    api('POST', '/api/whitelist/add', {entry: name}).then(function(res){
        refreshStatus();
        toast('Protected: ' + name);
    });
}
function renderTargets(list) {
    var el = document.getElementById('target-list');
    if (!list || !list.length) { el.innerHTML = '<div style="color:#555;font-size:12px">No targets (attacks all networks)</div>'; return; }
    el.innerHTML = list.map(function(t){return '<div class="list-item"><span>'+esc(t)+'</span><button class="remove-btn" onclick="removeTarget(\\''+escAttr(t)+'\\')">X</button></div>'}).join('');
}
function addWhitelist() {
    var inp = document.getElementById('wl-input');
    var v = inp.value.trim();
    if (!v) return;
    api('POST', '/api/whitelist/add', {entry: v}).then(function(res){
        inp.value = '';
        refreshStatus();
        toast('Added: ' + v);
    });
}
function removeWhitelist(e) {
    api('POST', '/api/whitelist/remove', {entry: e}).then(function(res){
        refreshStatus();
        toast('Removed: ' + e);
    });
}
function toggleSkipCaptured(val) {
    api('POST', '/api/skip-captured', {enabled: val}).then(function(r){
        toast('Skip captured: ' + (val ? 'ON' : 'OFF'));
        refreshStatus();
    });
}
function renderWhitelistTable(aoList, configList) {
    var el = document.getElementById('wl-tbody');
    var total = (aoList ? aoList.length : 0) + (configList ? configList.length : 0);
    document.getElementById('wl-count').textContent = '(' + total + ')';
    if (total === 0) { el.innerHTML = '<tr><td colspan="3" style="color:#555;padding:8px">No whitelisted networks</td></tr>'; return; }
    var rows = '';
    if (aoList) aoList.forEach(function(e){
        rows += '<tr style="border-bottom:1px solid #0f346022">' +
            '<td style="padding:6px;color:#e0e0e0">' + esc(e) + '</td>' +
            '<td style="padding:6px;color:#00d4aa;font-size:11px">AO plugin</td>' +
            '<td style="padding:6px"><button class="remove-btn" onclick="removeWhitelist(\\'' + escAttr(e) + '\\')" title="Remove from whitelist">X</button></td>' +
            '</tr>';
    });
    if (configList) configList.forEach(function(e){
        rows += '<tr style="border-bottom:1px solid #0f346022">' +
            '<td style="padding:6px;color:#aaa">' + esc(e) + '</td>' +
            '<td style="padding:6px;color:#555;font-size:11px">config.toml</td>' +
            '<td style="padding:6px;color:#333;font-size:10px">-</td>' +
            '</tr>';
    });
    el.innerHTML = rows;
}
function doAction(action) {
    api('POST', '/api/' + action, {}).then(function(res){
        toast(res.message || 'Done');
        refreshStatus();
    });
}
function switchMode(mode) {
    toast('Switching to ' + mode.toUpperCase() + '...');
    api('POST', '/api/mode', {mode: mode}).then(function(res){
        if (res.status === 'ok') toast('Mode: ' + mode.toUpperCase());
        else toast('Error: ' + (res.message || 'failed'));
    }).catch(function(){toast('Mode switch failed')});
}
function saveDiscordWebhook() {
    var url = document.getElementById('discord-webhook').value.trim();
    api('POST', '/api/discord-webhook', {url: url}).then(function(r){
        toast(url ? 'Discord webhook saved' : 'Discord notifications disabled');
    });
}
function esc(s) { var d = document.createElement('div'); d.textContent = s; return d.innerHTML; }
function escAttr(s) { return esc(s).replace(/'/g,'&#39;'); }
function fmtUptime(secs) {
    if (secs === null || secs === undefined) return '--';
    if (secs < 60) return secs + 's';
    if (secs < 3600) return Math.floor(secs/60) + 'm ' + (secs%60) + 's';
    var h = Math.floor(secs/3600); var m = Math.floor((secs%3600)/60);
    return h + 'h ' + m + 'm';
}
function refreshDisplay() {
    var img = document.getElementById('eink-preview');
    if (img) img.src = '/ui?' + Date.now();
}
function refreshStatus() {
    api('GET', '/api/status').then(function(d){
        document.getElementById('s-state').textContent = d.running ? 'RUNNING' : (d.stopped_permanently ? 'STOPPED (MAX CRASHES)' : 'STOPPED');
        document.getElementById('s-state').style.color = d.running ? '#00d4aa' : '#e94560';
        document.getElementById('s-pid').textContent = d.pid || '--';
        document.getElementById('s-uptime').textContent = fmtUptime(d.uptime_secs);
        var vrf = (d.verified_captures !== undefined) ? d.verified_captures : d.captures;
        var tot = (d.total_captures !== undefined) ? d.total_captures : d.captures;
        document.getElementById('s-captures').textContent = vrf + ' / ' + tot;
        document.getElementById('s-captures').style.color = vrf > 0 ? '#00d4aa' : '#e0e0e0';
        document.getElementById('s-crashes').textContent = d.crash_count;
        document.getElementById('s-crashes').style.color = d.crash_count > 0 ? '#f0c040' : '#e0e0e0';
        document.getElementById('s-fwcrashes').textContent = d.fw_crash_count;
        document.getElementById('s-fwcrashes').style.color = d.fw_crash_count > 0 ? '#e94560' : '#e0e0e0';
        document.getElementById('s-usb0-ip').textContent = d.usb0_ip || 'down';
        document.getElementById('s-usb0-ip').style.color = d.usb0_ip ? '#00d4aa' : '#666';
        document.getElementById('s-bnep0-ip').textContent = d.bnep0_ip || 'down';
        document.getElementById('s-bnep0-ip').style.color = d.bnep0_ip ? '#00d4aa' : '#666';
        document.getElementById('stopped-banner').style.display = d.stopped_permanently ? 'block' : 'none';
        // sync attacks
        var attacks = d.attacks || {};
        ['deauth','pmkid','csa','disassoc','anon_reassoc','rogue_m2'].forEach(function(k){
            var cb = document.getElementById('atk-'+k);
            if (cb) cb.checked = attacks[k] !== false;
        });
        // sync rate
        [1,2,3].forEach(function(n){
            document.getElementById('rate-'+n).classList.toggle('active', n===d.rate);
        });
        // sync channels
        document.getElementById('ch-input').value = d.channels || '';
        document.getElementById('ch-autohunt').checked = !!d.autohunt;
        document.getElementById('ch-dwell').value = d.dwell || 2;
        document.getElementById('dwell-val').textContent = (d.dwell || 2) + 's';
        // sync lists
        renderTargets(d.targets || []);
        renderWhitelistTable(d.whitelist || [], d.config_whitelist || []);
        var dcInput = document.getElementById('discord-webhook');
        if (dcInput && d.discord_webhook) dcInput.value = d.discord_webhook;
        var skipCb = document.getElementById('skip-captured');
        if (skipCb) skipCb.checked = !!d.skip_captured;
    }).catch(function(){});
}
function refreshHealth() {
    api('GET', '/api/health').then(function(h){
        function setDot(id, ok) {
            var el = document.getElementById(id);
            el.className = 'dot ' + (ok ? 'dot-green' : 'dot-red');
        }
        setDot('h-wifi', h.wifi);
        setDot('h-monitor', h.monitor);
        setDot('h-firmware', h.firmware);
        setDot('h-usb0', h.usb0);
        if (h.battery !== null && h.battery !== undefined) {
            document.getElementById('h-battery-wrap').style.display = 'flex';
            document.getElementById('h-battery-val').textContent = h.battery;
            var bdot = document.getElementById('h-battery-dot');
            bdot.className = 'dot ' + (h.battery > 20 ? 'dot-green' : (h.battery > 10 ? 'dot-yellow' : 'dot-red'));
        }
    }).catch(function(){});
}
function refreshAPs() {
    var nnCard = document.getElementById('nearby-networks-card');
    if (nnCard && nnCard.style.display === 'none') return;
    api('GET', '/api/aps').then(function(aps){
        var el = document.getElementById('ap-tbody');
        document.getElementById('ap-count').textContent = '(' + aps.length + ')';
        if (!aps || !aps.length) { el.innerHTML = '<tr><td colspan="8" style="color:#555;padding:8px">No networks found</td></tr>'; return; }
        el.innerHTML = aps.map(function(a){
            var rssiColor = a.rssi > -50 ? '#00d4aa' : (a.rssi > -70 ? '#f0c040' : '#e94560');
            var ssid = a.ssid || '<hidden>';
            return '<tr style="border-bottom:1px solid #0f346022">' +
                '<td style="padding:4px 6px;max-width:120px;overflow:hidden;text-overflow:ellipsis"><span style="color:#e0e0e0">' + esc(ssid) + '</span><br><span style="color:#555;font-size:9px">' + esc(a.mac) + '</span></td>' +
                '<td style="padding:4px 4px;color:#888">' + a.channel + '</td>' +
                '<td style="padding:4px 4px;color:'+rssiColor+';font-weight:bold">' + a.rssi + '</td>' +
                '<td style="padding:4px 4px;color:#888;font-size:10px">' + esc(a.encryption||'?').replace('WPA2','W2').replace('WPA3','W3') + '</td>' +
                '<td style="padding:4px 4px;color:'+(a.clients>0?'#00d4aa':'#333')+';font-weight:'+(a.clients>0?'bold':'normal')+'">' + a.clients + '</td>' +
                '<td style="padding:4px 4px;text-align:center">' + (a.captured ? '<span style="color:#00d4aa" title="Handshake captured">&#10003;</span>' : '<span style="color:#333">&middot;</span>') + '</td>' +
                '<td style="padding:4px 4px;text-align:center"><button style="background:none;border:1px solid #0f3460;color:#e94560;border-radius:4px;padding:2px 6px;font-size:10px;cursor:pointer;font-family:inherit" onclick="addTargetFromAP(\\''+escAttr(a.mac)+'\\')" title="Focus attacks on this network">&#9876;</button></td>' +
                '<td style="padding:4px 4px;text-align:center"><button style="background:none;border:1px solid #0f3460;color:#00d4aa;border-radius:4px;padding:2px 6px;font-size:10px;cursor:pointer;font-family:inherit" onclick="whitelistFromAP(\\''+escAttr(a.ssid||a.mac)+'\\')" title="Protect — never attack this network">&#9741;</button></td>' +
                '</tr>';
        }).join('');
    }).catch(function(){});
}
function refreshMode() {
    api('GET', '/api/mode').then(function(d){
        var isPwn = d.mode === 'pwn';
        document.getElementById('mode-ao').classList.toggle('active', d.mode === 'ao');
        document.getElementById('mode-pwn').classList.toggle('active', isPwn);
        // Show/hide mode-specific cards
        var nnCard = document.getElementById('nearby-networks-card');
        if (nnCard) nnCard.style.display = isPwn ? '' : 'none';
        var aoAtk = document.getElementById('ao-attacks-card');
        if (aoAtk) aoAtk.style.display = isPwn ? 'none' : '';
        var pwnAtk = document.getElementById('pwn-attacks-card');
        if (pwnAtk) pwnAtk.style.display = isPwn ? '' : 'none';
        // Hide all AO-only cards in PWN mode
        document.querySelectorAll('.ao-only-card').forEach(function(el){ el.style.display = isPwn ? 'none' : ''; });
        // Load PWN attack state from config
        if (isPwn) {
            api('GET', '/api/config').then(function(c){
                if (c.personality) {
                    var d = document.getElementById('pwn-deauth');
                    if (d) d.checked = c.personality.deauth !== false;
                    var a = document.getElementById('pwn-associate');
                    if (a) a.checked = c.personality.associate !== false;
                }
            }).catch(function(){});
        }
    }).catch(function(){});
}
function toggleBTVisible(val) {
    api('POST', '/api/bt-visibility', {visible: val}).then(function(r){
        toast('Bluetooth ' + (val ? 'VISIBLE' : 'HIDDEN'));
        document.getElementById('bt-status').textContent = val ? 'Discoverable — turn off in public' : 'Hidden — safe in public';
        document.getElementById('bt-status').style.color = val ? '#f0c040' : '#00d4aa';
    });
}
function refreshBT() {
    api('GET', '/api/bt-visibility').then(function(d){
        var cb = document.getElementById('bt-visible');
        if (cb) cb.checked = d.visible;
        var st = document.getElementById('bt-status');
        if (st) {
            st.textContent = d.visible ? 'Discoverable — turn off in public' : 'Hidden — safe in public';
            st.style.color = d.visible ? '#f0c040' : '#00d4aa';
        }
    }).catch(function(){});
}
function refreshCaptures() {
    var filterEl = document.getElementById('capture-filter');
    var filterVal = filterEl ? filterEl.value : 'all';
    api('GET', '/api/captures?filter=' + filterVal).then(function(list){
        var el = document.getElementById('captures-list');
        if (!list || !list.length) { el.innerHTML = '<div style="color:#555;font-size:12px">No captures yet</div>'; return; }
        el.innerHTML = list.map(function(c){
            var d = new Date(c.mtime * 1000);
            var ts = d.toLocaleString();
            var fname = c.file.replace('.pcapng','').replace('.pcap','').replace('.failed','').replace('.22000','');
            var mac = c.ap_mac || '';
            var ssid = c.ssid || '';
            if (!ssid && mac) ssid = 'HIDDEN';
            var label = ssid || mac || fname;
            // If no SSID/MAC from API, try parsing filename (bettercap format)
            if (!mac && !ssid) {
                var parts = fname.split('_');
                if (parts[0] && parts[0].split('-').length === 6) {
                    mac = parts[0].replace(/-/g, ':');
                    ssid = parts.length > 1 ? parts.slice(1).join('_') : '';
                    label = ssid || mac;
                } else {
                    label = fname;
                }
            }
            var typeBadge = '';
            if (c.type === 'PMKID') typeBadge = '<span style="background:#0f3460;color:#00d4aa;padding:1px 4px;border-radius:3px;font-size:9px;margin-left:4px">PMKID</span>';
            else if (c.type === '4-way') typeBadge = '<span style="background:#0f3460;color:#f0c040;padding:1px 4px;border-radius:3px;font-size:9px;margin-left:4px">4-WAY</span>';
            else if (c.type === 'hashcat') typeBadge = '<span style="background:#0f3460;color:#e0e0e0;padding:1px 4px;border-radius:3px;font-size:9px;margin-left:4px">HC22000</span>';
            var verifiedBadge = c.verified ? '<span style="color:#00d4aa;font-size:9px;margin-left:4px" title="Hashcat-ready .22000 file exists">&#10003; verified</span>' : '<span style="color:#e94560;font-size:9px;margin-left:4px" title="No .22000 hash file found">&#10007; unverified</span>';
            var macLine = mac ? ' <span style="color:#666;font-size:10px">['+esc(mac)+']</span>' : '';
            return '<div class="capture-item"><a href="'+BASE+'/api/download/capture/'+encodeURIComponent(c.file)+'" style="color:#00d4aa;text-decoration:none" download>'+esc(label)+'</a>'+macLine+typeBadge+verifiedBadge+' <span style="color:#555;font-size:10px">'+ts+'</span></div>';
        }).join('');
    }).catch(function(){});
}
function refreshCracked() {
    api('GET', '/api/cracked').then(function(list){
        var el = document.getElementById('cracked-list');
        if (!list || !list.length) { el.innerHTML = '<div style="color:#555;font-size:12px">No cracked passwords yet</div>'; return; }
        el.innerHTML = list.map(function(c){
            return '<div style="padding:4px 0;border-bottom:1px solid #0f346022">' +
                '<span style="color:#00d4aa;font-weight:bold">' + esc(c.ssid || c.bssid) + '</span>' +
                (c.bssid ? ' <span style="color:#666;font-size:10px">['+esc(c.bssid)+']</span>' : '') +
                '<br><span style="color:#f0c040;font-family:monospace;font-size:12px">' + esc(c.password) + '</span></div>';
        }).join('');
    }).catch(function(){});
}
function refreshLogs() {
    api('GET', '/api/logs').then(function(d){
        var el = document.getElementById('log-viewer');
        if (d.lines && d.lines.length) {
            el.innerHTML = d.lines.map(function(l){
                var color = '#888';
                if (l.indexOf('error') > -1 || l.indexOf('ERROR') > -1) color = '#e94560';
                else if (l.indexOf('warning') > -1 || l.indexOf('WARNING') > -1) color = '#f0c040';
                else if (l.indexOf('started') > -1 || l.indexOf('capture') > -1) color = '#00d4aa';
                return '<div style="color:'+color+';padding:1px 0;border-bottom:1px solid #1a1a2e">'+esc(l)+'</div>';
            }).join('');
            el.scrollTop = el.scrollHeight;
        } else {
            el.innerHTML = '<div style="color:#555">No AO logs yet</div>';
        }
    }).catch(function(){});
}
function refreshPlugins() {
    api('GET', '/api/plugins-list').then(function(list){
        var el = document.getElementById('plugins-list');
        if (!list || !list.length) { el.innerHTML = '<div style="color:#555">No plugins</div>'; return; }
        el.innerHTML = list.map(function(p){
            var dot = p.enabled ? '#00d4aa' : '#555';
            var webhook = p.has_webhook ? ' <a href="/plugins/'+esc(p.name)+'/" style="color:#0f3460;font-size:10px;text-decoration:none">[open]</a>' : '';
            return '<div style="padding:6px 0;border-bottom:1px solid #0f346022;display:flex;align-items:center;gap:8px">' +
                '<span style="width:8px;height:8px;border-radius:50%;background:'+dot+';flex-shrink:0"></span>' +
                '<div style="flex:1"><span style="color:#e0e0e0;font-size:12px;font-weight:bold">'+esc(p.name)+'</span>' + webhook +
                '<br><span style="color:#555;font-size:10px">'+esc(p.description).substring(0,60)+'</span></div>' +
                '<span style="color:#555;font-size:10px">v'+esc(p.version)+'</span>' +
                '</div>';
        }).join('');
    }).catch(function(){});
}
function loadConfig() {
    api('GET', '/api/config').then(function(c){
        if (c.error) return;
        document.getElementById('cfg-name').value = c.main.name || '';
        document.getElementById('cfg-recon-time').value = c.personality.recon_time;
        document.getElementById('cfg-recon-val').textContent = c.personality.recon_time + 's';
        document.getElementById('cfg-min-rssi').value = c.personality.min_rssi;
        document.getElementById('cfg-rssi-val').textContent = c.personality.min_rssi;
        document.getElementById('cfg-ap-ttl').value = c.personality.ap_ttl;
        document.getElementById('cfg-ttl-val').textContent = c.personality.ap_ttl + 's';
        document.getElementById('cfg-invert').checked = c.ui.invert;
        document.getElementById('cfg-rotation').value = c.ui.display_rotation;
        document.getElementById('cfg-auth').checked = c.web.auth;
    }).catch(function(){});
}
function saveConfig() {
    var data = {
        main: { name: document.getElementById('cfg-name').value },
        personality: {
            recon_time: parseInt(document.getElementById('cfg-recon-time').value),
            min_rssi: parseInt(document.getElementById('cfg-min-rssi').value),
            ap_ttl: parseInt(document.getElementById('cfg-ap-ttl').value),
        },
        ui: { invert: document.getElementById('cfg-invert').checked },
        'ui.display': { rotation: parseInt(document.getElementById('cfg-rotation').value) },
        'ui.web': { auth: document.getElementById('cfg-auth').checked },
    };
    api('POST', '/api/config', data).then(function(r){
        var st = document.getElementById('cfg-status');
        st.textContent = r.message || 'Saved';
        st.style.color = r.status === 'ok' ? '#00d4aa' : '#e94560';
        if (r.status === 'ok') toast('Settings saved — restart to apply');
    });
}
// Initial load — stagger to avoid hammering Pi Zero CPU
document.getElementById('dl-all-btn').href = BASE + '/api/download/all';
document.getElementById('dl-verified-btn').href = BASE + '/api/download/all?filter=verified';
refreshStatus();
setTimeout(refreshHealth, 1000);
setTimeout(refreshMode, 2000);
setTimeout(refreshAPs, 3000);
setTimeout(refreshCaptures, 4000);
setTimeout(refreshBT, 5000);
setTimeout(refreshLogs, 6000);
setTimeout(loadConfig, 7000);
setTimeout(refreshPlugins, 8000);
setTimeout(refreshCracked, 9000);
// Auto-refresh — stagger intervals to avoid simultaneous requests
setInterval(refreshDisplay, 3000);
setInterval(refreshStatus, 5000);
setInterval(refreshAPs, 10000);
setInterval(refreshHealth, 15000);
setInterval(refreshCaptures, 30000);
setInterval(refreshMode, 30000);
setInterval(refreshBT, 30000);
setInterval(refreshLogs, 30000);
setInterval(refreshCracked, 60000);
</script>
</body>
</html>'''

    def on_unload(self, ui):
        # Flush any pending state to disk
        if getattr(self, '_state_dirty', False):
            self._save_state(force=True)
        # Fast boot: save plugin states before shutdown
        self._save_delayed_plugins()

        # Show shutdown bull face
        try:
            ui.set('face', self._face('shutdown'))
            ui._state.reset()
        except Exception:
            pass

        self._stop_ao()

        # restore bettercap attack settings
        # agent not available in on_unload, but the config is shared
        # next epoch will use restored values
        if self._original_deauth is not None or self._original_associate is not None:
            logging.info("[angryoxide] plugin unloaded, bettercap attacks will resume on next restart")

        with ui._lock:
            try:
                ui.remove_element('angryoxide')
            except Exception:
                pass
