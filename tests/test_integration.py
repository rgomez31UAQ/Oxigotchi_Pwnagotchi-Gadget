"""
Live integration tests for the Oxigotchi system.

Connects to the real Pi and verifies the full stack:
firmware, AO binary, plugin, web dashboard, services, and safety features.

Run with:
    python -m pytest test_integration.py -v --tb=short -m integration
"""

import json
import os
import subprocess
import urllib.request
import urllib.error

import pytest

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

PI_HOST = os.environ.get("PI_HOST", "10.0.0.2")
PI_USER = "pi"
PI_PASS = "raspberry"
WEB_PORT = 8080
WEB_BASE = f"http://{PI_HOST}:{WEB_PORT}"
API_BASE = f"{WEB_BASE}/plugins/angryoxide"

SSH_OPTS = [
    "-o", "StrictHostKeyChecking=no",
    "-o", "LogLevel=ERROR",
    "-o", "ConnectTimeout=5",
    "-o", "BatchMode=yes",
]

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def ssh_cmd(cmd, timeout=10):
    """Run a command on the Pi via SSH. Returns (stdout, stderr, returncode)."""
    result = subprocess.run(
        ["ssh"] + SSH_OPTS + [f"{PI_USER}@{PI_HOST}", cmd],
        capture_output=True, text=True, timeout=timeout,
    )
    return result.stdout.strip(), result.stderr.strip(), result.returncode


def api_get(path):
    """GET a JSON endpoint from the AO plugin API."""
    url = API_BASE + path
    req = urllib.request.Request(url)
    with urllib.request.urlopen(req, timeout=10) as r:
        return json.loads(r.read())


def web_get(path):
    """GET raw text from the web UI."""
    url = API_BASE + path
    req = urllib.request.Request(url)
    with urllib.request.urlopen(req, timeout=10) as r:
        return r.read().decode("utf-8", errors="replace")


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


def _pi_is_reachable():
    """Quick connectivity check (SSH)."""
    try:
        _, _, rc = ssh_cmd("echo ok", timeout=5)
        return rc == 0
    except (subprocess.TimeoutExpired, OSError):
        return False


@pytest.fixture(scope="session", autouse=True)
def require_pi():
    """Skip the entire session if the Pi is unreachable."""
    if not _pi_is_reachable():
        pytest.skip("Pi is not reachable at " + PI_HOST)


# ---------------------------------------------------------------------------
# 1. System Health
# ---------------------------------------------------------------------------


@pytest.mark.integration
class TestSystemHealth:

    def test_pi_reachable(self):
        out, _, rc = ssh_cmd("echo pong")
        assert rc == 0
        assert out == "pong"

    def test_wlan0mon_up(self):
        out, _, rc = ssh_cmd("ip link show wlan0mon")
        assert rc == 0, "wlan0mon interface does not exist"
        assert "wlan0mon" in out

    def test_pwnagotchi_active(self):
        out, _, rc = ssh_cmd("systemctl is-active pwnagotchi")
        assert out == "active", f"pwnagotchi service is {out}"

    def test_angryoxide_running(self):
        out, _, rc = ssh_cmd("pgrep -x angryoxide")
        assert rc == 0, "angryoxide process not found"
        assert out != ""

    def test_usb0_up(self):
        out, _, rc = ssh_cmd("ip link show usb0")
        assert rc == 0, "usb0 interface does not exist"
        assert "usb0" in out

    def test_networkmanager_active(self):
        out, _, rc = ssh_cmd("systemctl is-active NetworkManager")
        assert out == "active", f"NetworkManager is {out}"


# ---------------------------------------------------------------------------
# 2. Plugin Deployment
# ---------------------------------------------------------------------------


@pytest.mark.integration
class TestPluginDeployment:

    def test_plugin_file_exists(self):
        _, _, rc = ssh_cmd("test -f /etc/pwnagotchi/custom-plugins/angryoxide.py")
        assert rc == 0, "angryoxide.py plugin not found"

    def test_config_overlay_exists(self):
        _, _, rc = ssh_cmd("test -f /etc/pwnagotchi/conf.d/angryoxide-v5.toml")
        assert rc == 0, "angryoxide-v5.toml config overlay not found"

    def test_mode_switcher_exists(self):
        _, _, rc = ssh_cmd("test -x /usr/local/bin/pwnoxide-mode")
        assert rc == 0, "pwnoxide-mode not found or not executable"

    def test_faces_deployed(self):
        out, _, rc = ssh_cmd(
            "find /etc/pwnagotchi/custom-plugins/faces/ -name '*.png' | wc -l"
        )
        assert rc == 0
        count = int(out)
        assert count >= 20, f"Only {count} PNG faces deployed, expected >= 20"

    def test_state_file_exists(self):
        # State file may be in plugin data dir or alongside the plugin
        out, _, rc = ssh_cmd(
            "find /etc/pwnagotchi /var/local/pwnagotchi -name 'angryoxide_state.json' 2>/dev/null | head -1"
        )
        assert out != "", "angryoxide_state.json not found anywhere"


# ---------------------------------------------------------------------------
# 3. Web Dashboard API
# ---------------------------------------------------------------------------


@pytest.mark.integration
class TestWebDashboardAPI:

    def test_api_status(self):
        data = api_get("/api/status")
        assert isinstance(data, dict), f"Expected dict, got {type(data)}"
        assert "running" in data, f"Missing 'running' key in {list(data.keys())}"

    def test_api_health(self):
        data = api_get("/api/health")
        assert isinstance(data, dict), f"Expected dict, got {type(data)}"
        assert "wifi" in data, f"Missing 'wifi' key in {list(data.keys())}"

    def test_api_aps(self):
        data = api_get("/api/aps")
        assert isinstance(data, list), f"Expected list, got {type(data)}"

    def test_api_mode(self):
        data = api_get("/api/mode")
        assert isinstance(data, dict), f"Expected dict, got {type(data)}"
        assert "mode" in data, f"Missing 'mode' key in {list(data.keys())}"

    def test_api_captures(self):
        data = api_get("/api/captures")
        assert isinstance(data, list), f"Expected list, got {type(data)}"

    def test_dashboard_html(self):
        html = web_get("/")
        assert "AngryOxide" in html, "Dashboard HTML does not contain 'AngryOxide'"


# ---------------------------------------------------------------------------
# 4. AO Binary
# ---------------------------------------------------------------------------


@pytest.mark.integration
class TestAOBinary:

    def test_ao_binary_exists(self):
        _, _, rc = ssh_cmd("test -f /usr/local/bin/angryoxide")
        assert rc == 0, "angryoxide binary not found"

    def test_ao_binary_executable(self):
        _, _, rc = ssh_cmd("test -x /usr/local/bin/angryoxide")
        assert rc == 0, "angryoxide binary is not executable"

    def test_ao_has_no_setup(self):
        out, _, rc = ssh_cmd("ps aux | grep '[a]ngryoxide'")
        assert rc == 0, "angryoxide process not found"
        assert "--no-setup" in out, f"--no-setup flag missing from AO process: {out}"


# ---------------------------------------------------------------------------
# 5. Firmware
# ---------------------------------------------------------------------------


@pytest.mark.integration
class TestFirmware:

    FIRMWARE_PATH = "/lib/firmware/brcm/brcmfmac43436-sdio.bin"

    def test_firmware_deployed(self):
        _, _, rc = ssh_cmd(f"test -f {self.FIRMWARE_PATH}")
        assert rc == 0, "v5 firmware not deployed"

    def test_firmware_backup_exists(self):
        _, _, rc = ssh_cmd(f"test -f {self.FIRMWARE_PATH}.orig")
        assert rc == 0, "firmware backup (.bin.orig) not found"

    def test_firmware_size(self):
        out, _, rc = ssh_cmd(f"stat -c %s {self.FIRMWARE_PATH}")
        assert rc == 0
        size = int(out)
        # v5 firmware is ~414,696 bytes; allow +/- 5 KB tolerance
        assert 409_000 <= size <= 420_000, (
            f"Firmware size {size} bytes is outside expected range for v5 (~414696)"
        )


# ---------------------------------------------------------------------------
# 6. Safety
# ---------------------------------------------------------------------------


@pytest.mark.integration
class TestSafety:

    def test_wifi_fix_pwnlib(self):
        """reload_brcm must be commented out in stop_monitor_interface (pwnlib)."""
        out, _, rc = ssh_cmd(
            "grep -n 'reload_brcm' /usr/local/lib/python3*/dist-packages/pwnagotchi/mesh/wifi.py 2>/dev/null || "
            "grep -rn 'reload_brcm' /usr/local/lib/python3*/dist-packages/pwnagotchi/ 2>/dev/null | "
            "grep 'stop_monitor'"
        )
        if out:
            # Every occurrence in stop_monitor_interface context should be commented
            for line in out.splitlines():
                # Lines containing reload_brcm near stop_monitor should be commented
                assert line.lstrip().startswith("#") or "def " in line or rc != 0, (
                    f"reload_brcm is NOT commented out: {line}"
                )

    def test_wifi_fix_bettercap_launcher(self):
        """reload_brcm in bettercap-launcher must be inside a conditional block."""
        out, _, rc = ssh_cmd("cat /usr/bin/bettercap-launcher 2>/dev/null")
        assert rc == 0, "bettercap-launcher not found"
        lines = out.splitlines()
        for i, line in enumerate(lines):
            if line.strip() == 'reload_brcm':
                # Bare reload_brcm at top level — check if inside an if block
                context = '\n'.join(lines[max(0, i - 3):i + 1])
                assert 'if ' in context, (
                    f"reload_brcm at line {i+1} is not inside a conditional block"
                )

    def test_boot_splash_service_enabled(self):
        out, _, rc = ssh_cmd("systemctl is-enabled oxagotchi-splash.service")
        assert out == "enabled", f"oxigotchi-splash service is {out}, expected enabled"

    def test_csrf_patch_applied(self):
        """Plugin webhooks must be exempted from CSRF."""
        out, _, rc = ssh_cmd(
            "grep -c 'csrf.exempt' "
            "/home/pi/.pwn/lib/python3.13/site-packages/pwnagotchi/ui/web/handler.py"
        )
        assert rc == 0 and out.strip() != '0', "CSRF exemption not found in handler.py"

    def test_apt_holds_active(self):
        """Critical packages must be held to prevent breakage from apt upgrade."""
        out, _, rc = ssh_cmd("apt-mark showhold")
        assert rc == 0
        held = set(out.splitlines())
        for pkg in ["linux-image-rpi-v8", "firmware-brcm80211"]:
            assert pkg in held, f"{pkg} is not held — apt upgrade could break the system"

    def test_firmware_protection_hook(self):
        """Apt hook must exist to protect patched firmware binary."""
        _, _, rc = ssh_cmd(
            "test -f /etc/apt/apt.conf.d/99-oxigotchi-firmware-protect"
        )
        assert rc == 0, "Firmware protection apt hook not found"

    def test_bt_keepalive_timer_enabled(self):
        """BT keepalive timer must be enabled to prevent tether drops."""
        out, _, rc = ssh_cmd("systemctl is-enabled bt-keepalive.timer")
        assert out == "enabled", f"bt-keepalive.timer is {out}, expected enabled"

    def test_tweak_view_deployed(self):
        """tweak_view.json display layout must be deployed."""
        _, _, rc = ssh_cmd(
            "test -f /etc/pwnagotchi/custom-plugins/tweak_view.json"
        )
        assert rc == 0, "tweak_view.json not found on Pi"

    def test_avahi_hostname(self):
        """Avahi hostname should be set to oxigotchi for mDNS discovery."""
        out, _, rc = ssh_cmd(
            "grep 'host-name=oxigotchi' /etc/avahi/avahi-daemon.conf"
        )
        assert rc == 0, "Avahi hostname not set to oxigotchi"
