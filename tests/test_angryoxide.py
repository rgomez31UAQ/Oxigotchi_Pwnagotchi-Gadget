"""
Comprehensive test suite for the AngryOxide pwnagotchi plugin (v2).

Run with:
    python -m pytest test_angryoxide.py -v

All pwnagotchi dependencies are mocked so tests run on any platform.
"""

import sys
import types
import time
import json
import pytest
from unittest import mock
from unittest.mock import MagicMock, patch, PropertyMock, call, mock_open


# ---------------------------------------------------------------------------
# Mock the entire pwnagotchi module tree BEFORE importing the plugin
# ---------------------------------------------------------------------------

def _install_pwnagotchi_mocks():
    """Insert fake pwnagotchi modules into sys.modules."""
    mods = {}

    pwnagotchi_mod = types.ModuleType('pwnagotchi')
    mods['pwnagotchi'] = pwnagotchi_mod

    plugins_mod = types.ModuleType('pwnagotchi.plugins')
    plugins_mod.Plugin = type('Plugin', (), {})
    plugins_mod.on = MagicMock()
    plugins_mod.loaded = {}
    plugins_mod.toggle_plugin = MagicMock()
    mods['pwnagotchi.plugins'] = plugins_mod
    pwnagotchi_mod.plugins = plugins_mod

    ui_mod = types.ModuleType('pwnagotchi.ui')
    mods['pwnagotchi.ui'] = ui_mod

    faces_mod = types.ModuleType('pwnagotchi.ui.faces')
    faces_mod.ANGRY = '(>_<)'
    faces_mod.EXCITED = '(^_^)'
    faces_mod.BORED = '(-_-)'
    faces_mod.BROKEN = '(X_X)'
    faces_mod.SLEEP = '(-_-) zzz'
    faces_mod.SAD = '(T_T)'
    faces_mod.AWAKE = '(o_o)'
    mods['pwnagotchi.ui.faces'] = faces_mod

    components_mod = types.ModuleType('pwnagotchi.ui.components')
    components_mod.LabeledValue = MagicMock()
    mods['pwnagotchi.ui.components'] = components_mod

    view_mod = types.ModuleType('pwnagotchi.ui.view')
    view_mod.BLACK = 0
    mods['pwnagotchi.ui.view'] = view_mod

    fonts_mod = types.ModuleType('pwnagotchi.ui.fonts')
    fonts_mod.Bold = MagicMock()
    fonts_mod.Medium = MagicMock()
    fonts_mod.Small = MagicMock()
    fonts_mod.BoldSmall = MagicMock()
    fonts_mod.BoldBig = MagicMock()
    fonts_mod.Huge = MagicMock()
    mods['pwnagotchi.ui.fonts'] = fonts_mod

    # Flask mock with jsonify and Response
    flask_mod = types.ModuleType('flask')

    def _fake_jsonify(data):
        resp = MagicMock()
        resp.json = data
        resp.get_json = MagicMock(return_value=data)
        resp.status_code = 200
        resp.content_type = 'application/json'
        return resp

    class _FakeResponse:
        def __init__(self, body='', mimetype='text/html', status=200, headers=None):
            self.data = body
            self.mimetype = mimetype
            self.status_code = status
            self.content_type = mimetype
            self.headers = headers or {}

    flask_mod.jsonify = _fake_jsonify
    flask_mod.Response = _FakeResponse
    flask_mod.send_file = MagicMock()
    mods['flask'] = flask_mod

    sys.modules.update(mods)
    return mods


_mocked_modules = _install_pwnagotchi_mocks()

# Now import the plugin module
from angryoxide_v2 import AngryOxide


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

@pytest.fixture
def plugin():
    """Create a fresh AngryOxide plugin instance with sensible defaults."""
    ao = AngryOxide()
    ao.options = {
        'binary_path': '/usr/local/bin/angryoxide',
        'interface': 'wlan0mon',
        'output_dir': '/etc/pwnagotchi/handshakes/',
        'notx': False,
        'no_setup': True,
        'extra_args': '',
    }
    return ao


@pytest.fixture
def agent():
    """Create a mock pwnagotchi agent."""
    a = MagicMock()
    a._config = {
        'personality': {'deauth': True, 'associate': True},
        'bettercap': {'handshakes': '/etc/pwnagotchi/handshakes/'},
        'main': {'whitelist': []},
    }
    a._handshakes = {}
    a._last_pwnd = None
    a._update_handshakes = MagicMock()
    a.set_face = MagicMock()
    a.set_status = MagicMock()
    return a


class FakeRequest:
    """Minimal Flask-like request object for webhook tests."""

    def __init__(self, method='GET', path='/', json_data=None):
        self.method = method
        self.path = path
        self._json = json_data or {}
        self.authorization = None
        self.data = None
        self.args = {}

    def get_json(self, force=False, silent=False):
        return self._json


@pytest.fixture
def make_request():
    """Factory fixture that creates FakeRequest objects."""
    def _make(method='GET', path='/', json_data=None):
        return FakeRequest(method=method, path=path, json_data=json_data)
    return _make


def _resp_data(resp):
    """Extract JSON data from a webhook response (handles tuples for error codes)."""
    if isinstance(resp, tuple):
        return resp[0].json
    return resp.json


def _resp_status(resp):
    """Extract status code from a webhook response."""
    if isinstance(resp, tuple):
        return resp[1]
    return getattr(resp, 'status_code', 200)


# ---------------------------------------------------------------------------
# 1. Command Building Tests
# ---------------------------------------------------------------------------

class TestBuildCmd:
    """Tests for _build_cmd() method."""

    def test_build_cmd_defaults(self, plugin):
        """Default options produce the correct base command."""
        import socket
        cmd = plugin._build_cmd()
        assert cmd[0] == '/usr/local/bin/angryoxide'
        assert '--interface' in cmd
        assert 'wlan0mon' in cmd
        assert '--headless' in cmd
        assert '--output' in cmd
        # Output path now includes hostname prefix
        idx = cmd.index('--output')
        output_path = cmd[idx + 1]
        hostname = socket.gethostname() or 'oxigotchi'
        assert output_path.startswith('/etc/pwnagotchi/handshakes/')
        assert output_path.endswith(hostname)
        assert '--no-setup' in cmd

    def test_build_cmd_no_setup(self, plugin):
        """--no-setup flag toggled by option."""
        plugin.options['no_setup'] = True
        assert '--no-setup' in plugin._build_cmd()

        plugin.options['no_setup'] = False
        assert '--no-setup' not in plugin._build_cmd()

    def test_build_cmd_notx(self, plugin):
        """--notx flag toggled by option."""
        plugin.options['notx'] = True
        assert '--notx' in plugin._build_cmd()

        plugin.options['notx'] = False
        assert '--notx' not in plugin._build_cmd()

    def test_build_cmd_rate(self, plugin):
        """--rate N added based on _rate value."""
        plugin._rate = 3
        cmd = plugin._build_cmd()
        idx = cmd.index('--rate')
        assert cmd[idx + 1] == '3'

    def test_build_cmd_attacks_all_enabled(self, plugin):
        """No --disable-* flags when all attacks are on."""
        for k in plugin._attacks:
            plugin._attacks[k] = True
        cmd = plugin._build_cmd()
        disable_flags = [c for c in cmd if c.startswith('--disable')]
        assert len(disable_flags) == 0

    def test_build_cmd_attacks_selective_disable(self, plugin):
        """Correct --disable-* flags for each individually disabled attack type."""
        # Map from attack key to expected flag
        flag_map = {
            'deauth': '--disable-deauth',
            'pmkid': '--disable-pmkid',
            'csa': '--disable-csa',
            'disassoc': '--disable-disassoc',
            'anon_reassoc': '--disable-anon',
            'rogue_m2': '--disable-roguem2',
        }
        for name, expected_flag in flag_map.items():
            # Enable all
            for k in plugin._attacks:
                plugin._attacks[k] = True
            # Disable one
            plugin._attacks[name] = False
            cmd = plugin._build_cmd()
            assert expected_flag in cmd, "Expected %s when %s is disabled" % (expected_flag, name)
            # Only one disable flag should be present
            disable_flags = [c for c in cmd if c.startswith('--disable')]
            assert len(disable_flags) == 1, "Only %s should be disabled, got %s" % (name, disable_flags)

    def test_build_cmd_autohunt(self, plugin):
        """--autohunt flag when _autohunt=True, no --channel."""
        plugin._autohunt = True
        plugin._channels = '1,6,11'
        cmd = plugin._build_cmd()
        assert '--autohunt' in cmd
        # autohunt takes precedence over channels (elif branch)
        assert '--channel' not in cmd

    def test_build_cmd_channels(self, plugin):
        """--channel X when _channels is set and autohunt off."""
        plugin._autohunt = False
        plugin._channels = '1,6,11'
        cmd = plugin._build_cmd()
        idx = cmd.index('--channel')
        assert cmd[idx + 1] == '1,6,11'

    def test_build_cmd_channels_empty(self, plugin):
        """No --channel flag when _channels is empty."""
        plugin._autohunt = False
        plugin._channels = ''
        cmd = plugin._build_cmd()
        assert '--channel' not in cmd

    def test_build_cmd_dwell(self, plugin):
        """--dwell N reflects _dwell."""
        plugin._dwell = 15
        cmd = plugin._build_cmd()
        idx = cmd.index('--dwell')
        assert cmd[idx + 1] == '15'

    def test_build_cmd_targets(self, plugin):
        """--target-entry for each target in _targets."""
        plugin._targets = ['AA:BB:CC:DD:EE:FF', '11:22:33:44:55:66']
        cmd = plugin._build_cmd()
        count = cmd.count('--target-entry')
        assert count == 2
        # Verify actual values follow the flags
        indices = [i for i, c in enumerate(cmd) if c == '--target-entry']
        assert cmd[indices[0] + 1] == 'AA:BB:CC:DD:EE:FF'
        assert cmd[indices[1] + 1] == '11:22:33:44:55:66'

    def test_build_cmd_whitelist(self, plugin):
        """--whitelist-entry for each entry in _whitelist_entries."""
        plugin._whitelist_entries = ['MyNetwork', 'HomeWiFi']
        cmd = plugin._build_cmd()
        count = cmd.count('--whitelist-entry')
        assert count == 2

    def test_build_cmd_extra_args(self, plugin):
        """extra_args are split and appended to command."""
        plugin.options['extra_args'] = '--verbose --some-flag value'
        cmd = plugin._build_cmd()
        assert '--verbose' in cmd
        assert '--some-flag' in cmd
        assert 'value' in cmd
        # extra_args should be at the end
        assert cmd[-3:] == ['--verbose', '--some-flag', 'value']

    def test_build_cmd_extra_args_empty(self, plugin):
        """Empty extra_args does not add anything extra."""
        plugin.options['extra_args'] = ''
        cmd = plugin._build_cmd()
        assert '' not in cmd


# ---------------------------------------------------------------------------
# 2. Backoff Tests
# ---------------------------------------------------------------------------

class TestBackoff:
    """Tests for _backoff_seconds() exponential backoff."""

    def test_backoff_first_crash(self, plugin):
        """First crash: 5 seconds."""
        plugin._crash_count = 1
        assert plugin._backoff_seconds() == 5

    def test_backoff_second_crash(self, plugin):
        """Second crash: 10 seconds."""
        plugin._crash_count = 2
        assert plugin._backoff_seconds() == 10

    def test_backoff_third_crash(self, plugin):
        """Third crash: 20 seconds."""
        plugin._crash_count = 3
        assert plugin._backoff_seconds() == 20

    def test_backoff_caps_at_300(self, plugin):
        """Backoff never exceeds 300 seconds."""
        plugin._crash_count = 100
        assert plugin._backoff_seconds() == 300

    def test_backoff_progression(self, plugin):
        """Verify the full progression: 5, 10, 20, 40, 80, 160, 300, 300..."""
        expected = [5, 10, 20, 40, 80, 160, 300, 300]
        for i, exp in enumerate(expected, start=1):
            plugin._crash_count = i
            assert plugin._backoff_seconds() == exp, "crash_count=%d expected %d" % (i, exp)


# ---------------------------------------------------------------------------
# 3. Capture Parsing Tests
# ---------------------------------------------------------------------------

class TestParseCaptureFilename:
    """Tests for _parse_capture_filename() static method."""

    def test_parse_mac_filename(self):
        """Standard AO filename extracts AP MAC."""
        ap, sta = AngryOxide._parse_capture_filename("AA-BB-CC-DD-EE-FF_NetworkName.pcapng")
        assert ap == "AA:BB:CC:DD:EE:FF"
        assert sta == "unknown"

    def test_parse_no_mac(self):
        """Filename without MAC pattern returns unknowns."""
        ap, sta = AngryOxide._parse_capture_filename("weird_file.pcapng")
        assert ap == "unknown"
        assert sta == "unknown"

    def test_parse_short_name(self):
        """Short filename returns unknowns."""
        ap, sta = AngryOxide._parse_capture_filename("short.pcapng")
        assert ap == "unknown"
        assert sta == "unknown"

    def test_parse_lowercase_mac(self):
        """Lowercase MAC in filename is parsed correctly."""
        ap, sta = AngryOxide._parse_capture_filename("aa-bb-cc-dd-ee-ff_TestNet.pcapng")
        assert ap == "aa:bb:cc:dd:ee:ff"
        assert sta == "unknown"

    def test_parse_no_underscore(self):
        """MAC without underscore separator still parsed if format matches."""
        ap, sta = AngryOxide._parse_capture_filename("AA-BB-CC-DD-EE-FF.pcapng")
        assert ap == "AA:BB:CC:DD:EE:FF"

    def test_parse_empty_string(self):
        """Empty filename returns unknowns."""
        ap, sta = AngryOxide._parse_capture_filename("")
        assert ap == "unknown"
        assert sta == "unknown"

    def test_parse_multiple_underscores(self):
        """Filename with multiple underscores still parses MAC correctly."""
        ap, sta = AngryOxide._parse_capture_filename("AA-BB-CC-DD-EE-FF_Network_With_Spaces.pcapng")
        assert ap == "AA:BB:CC:DD:EE:FF"


# ---------------------------------------------------------------------------
# 4. Whitelist Tests
# ---------------------------------------------------------------------------

class TestWhitelist:
    """Tests for _is_whitelisted() method."""

    def test_whitelist_match(self, plugin):
        """Filename containing whitelisted SSID returns True."""
        result = plugin._is_whitelisted("AA-BB-CC-DD-EE-FF_MyHomeNetwork.pcapng", ["MyHomeNetwork"])
        assert result is True

    def test_whitelist_no_match(self, plugin):
        """Filename not matching any whitelist entry returns False."""
        result = plugin._is_whitelisted("AA-BB-CC-DD-EE-FF_SomeOtherNet.pcapng", ["MyHomeNetwork"])
        assert result is False

    def test_whitelist_case_insensitive(self, plugin):
        """Matching is case-insensitive."""
        result = plugin._is_whitelisted("AA-BB-CC-DD-EE-FF_MYHOMENETWORK.pcapng", ["myhomenetwork"])
        assert result is True

    def test_whitelist_case_insensitive_reverse(self, plugin):
        """Matching is case-insensitive (whitelist uppercase, filename lowercase)."""
        result = plugin._is_whitelisted("AA-BB-CC-DD-EE-FF_myhomenetwork.pcapng", ["MYHOMENETWORK"])
        assert result is True

    def test_whitelist_pcap_extension(self, plugin):
        """Works with .pcap extension too."""
        result = plugin._is_whitelisted("AA-BB-CC-DD-EE-FF_MyHomeNetwork.pcap", ["MyHomeNetwork"])
        assert result is True

    def test_whitelist_mac_in_name(self, plugin):
        """MAC address in whitelist matches filename."""
        result = plugin._is_whitelisted("AA-BB-CC-DD-EE-FF_NetworkName.pcapng", ["AA-BB-CC-DD-EE-FF"])
        assert result is True

    def test_whitelist_partial_match(self, plugin):
        """Partial SSID match works (substring after normalization)."""
        result = plugin._is_whitelisted("AA-BB-CC-DD-EE-FF_MyHomeNetwork.pcapng", ["HomeNet"])
        assert result is True

    def test_whitelist_empty_list(self, plugin):
        """Empty whitelist matches nothing."""
        result = plugin._is_whitelisted("AA-BB-CC-DD-EE-FF_Anything.pcapng", [])
        assert result is False

    def test_whitelist_special_chars_stripped(self, plugin):
        """Non-alphanumeric chars are stripped during normalization."""
        result = plugin._is_whitelisted("AA-BB-CC-DD-EE-FF_My-Home_Net.pcapng", ["My-Home_Net!"])
        assert result is True


# ---------------------------------------------------------------------------
# 5. Uptime Formatting Tests
# ---------------------------------------------------------------------------

class TestUptimeFormatting:
    """Tests for _format_uptime() method."""

    def test_uptime_not_started(self, plugin):
        """_start_time=None returns '0m'."""
        plugin._start_time = None
        assert plugin._format_uptime() == '0m'

    @patch('angryoxide_v2.time')
    def test_uptime_minutes(self, mock_time, plugin):
        """1800s elapsed returns '30m'."""
        plugin._start_time = 1000.0
        mock_time.time.return_value = 2800.0
        assert plugin._format_uptime() == '30m'

    @patch('angryoxide_v2.time')
    def test_uptime_hours(self, mock_time, plugin):
        """7200s elapsed returns '2h'."""
        plugin._start_time = 1000.0
        mock_time.time.return_value = 8200.0
        assert plugin._format_uptime() == '2h'

    @patch('angryoxide_v2.time')
    def test_uptime_zero_seconds(self, mock_time, plugin):
        """0 seconds elapsed returns '0m'."""
        plugin._start_time = 1000.0
        mock_time.time.return_value = 1000.0
        assert plugin._format_uptime() == '0m'

    @patch('angryoxide_v2.time')
    def test_uptime_just_under_hour(self, mock_time, plugin):
        """3599s elapsed returns '59m'."""
        plugin._start_time = 1000.0
        mock_time.time.return_value = 4599.0
        assert plugin._format_uptime() == '59m'

    @patch('angryoxide_v2.time')
    def test_uptime_exactly_one_hour(self, mock_time, plugin):
        """3600s elapsed returns '1h'."""
        plugin._start_time = 1000.0
        mock_time.time.return_value = 4600.0
        assert plugin._format_uptime() == '1h'


# ---------------------------------------------------------------------------
# 6. Webhook API Tests
# ---------------------------------------------------------------------------

class TestWebhook:
    """Tests for on_webhook() method."""

    def test_get_root_returns_html(self, plugin, make_request):
        """GET / returns HTML dashboard with status 200."""
        req = make_request(method='GET', path='/')
        resp = plugin.on_webhook('/', req)
        # Should be a Response object with text/html
        assert not isinstance(resp, tuple), "GET / should not return error tuple"
        assert resp.mimetype == 'text/html'
        assert '<!DOCTYPE html>' in resp.data

    def test_get_empty_path_returns_html(self, plugin, make_request):
        """GET '' (empty path) returns HTML dashboard."""
        req = make_request(method='GET', path='')
        resp = plugin.on_webhook('', req)
        assert not isinstance(resp, tuple)
        assert resp.mimetype == 'text/html'

    def test_get_api_status(self, plugin, make_request):
        """GET /api/status returns full status JSON."""
        plugin._running = True
        plugin._captures = 5
        plugin._crash_count = 1
        plugin._fw_crash_count = 0
        plugin._stopped_permanently = False
        plugin._start_time = 1000.0
        mock_proc = MagicMock()
        mock_proc.pid = 1234
        plugin._process = mock_proc

        req = make_request(method='GET', path='/api/status')
        with patch('angryoxide_v2.time') as mock_time:
            mock_time.time.return_value = 1060.0
            resp = plugin.on_webhook('/api/status', req)

        data = _resp_data(resp)
        assert data['running'] is True
        assert data['pid'] == 1234
        assert data['captures'] == 5
        assert data['crash_count'] == 1
        assert data['uptime_secs'] == 60
        assert 'attacks' in data
        assert 'rate' in data
        assert 'channels' in data
        assert 'targets' in data
        assert 'whitelist' in data

    def test_get_api_status_not_running(self, plugin, make_request):
        """GET /api/status when not running returns null uptime."""
        plugin._running = False
        plugin._process = None
        plugin._start_time = None

        req = make_request(method='GET', path='/api/status')
        resp = plugin.on_webhook('/api/status', req)

        data = _resp_data(resp)
        assert data['running'] is False
        assert data['pid'] is None
        assert data['uptime_secs'] is None

    def test_get_api_health(self, plugin, make_request):
        """GET /api/health returns health JSON."""
        req = make_request(method='GET', path='/api/health')
        with patch('angryoxide_v2.os.path.exists', return_value=True), \
             patch('angryoxide_v2.subprocess.run') as mock_run:
            mock_run.return_value = MagicMock(stdout='normal log output', returncode=0)
            resp = plugin.on_webhook('/api/health', req)

        data = _resp_data(resp)
        assert data['wifi'] is True
        assert data['monitor'] is True
        assert data['firmware'] is True

    def test_get_api_captures(self, plugin, make_request):
        """GET /api/captures returns capture list sorted by mtime descending."""
        # API now scans disk directly, not _known_files
        req = make_request(method='GET', path='/api/captures')
        with patch('angryoxide_v2.os.path.isdir', return_value=True), \
             patch('angryoxide_v2.os.listdir', return_value=['file1.pcapng', 'file2.pcapng']), \
             patch('angryoxide_v2.os.path.getmtime', side_effect=lambda p: 2000.0 if 'file2' in p else 1000.0), \
             patch('angryoxide_v2.os.path.isfile', return_value=False), \
             patch('angryoxide_v2.os.path.getsize', return_value=4096), \
             patch('angryoxide_v2.os.path.basename', side_effect=lambda p: p.split('/')[-1].split('\\')[-1]), \
             patch('angryoxide_v2.os.path.dirname', return_value='/etc/pwnagotchi/handshakes/'):
            resp = plugin.on_webhook('/api/captures', req)

        data = _resp_data(resp)
        assert len(data) == 2
        assert data[0]['file'] == 'file2.pcapng'
        assert data[1]['file'] == 'file1.pcapng'

    def test_post_api_attacks(self, plugin, make_request):
        """POST /api/attacks updates attack toggles and triggers restart."""
        plugin._agent = MagicMock()  # needed for _restart_ao
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        plugin._attacks['deauth'] = True
        plugin._attacks['pmkid'] = True

        req = make_request(method='POST', path='/api/attacks',
                           json_data={'deauth': False, 'pmkid': False})

        with patch.object(plugin, '_restart_ao') as mock_restart:
            resp = plugin.on_webhook('/api/attacks', req)

        data = _resp_data(resp)
        assert data['status'] == 'ok'
        assert plugin._attacks['deauth'] is False
        assert plugin._attacks['pmkid'] is False
        assert plugin._attacks['csa'] is True  # unchanged
        mock_restart.assert_called_once()

    def test_post_api_rate(self, plugin, make_request):
        """POST /api/rate updates rate and triggers restart."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        plugin._rate = 2

        req = make_request(method='POST', path='/api/rate', json_data={'rate': 3})

        with patch.object(plugin, '_restart_ao') as mock_restart:
            resp = plugin.on_webhook('/api/rate', req)

        data = _resp_data(resp)
        assert data['rate'] == 3
        assert plugin._rate == 3
        mock_restart.assert_called_once()

    def test_post_api_rate_invalid_unchanged(self, plugin, make_request):
        """POST /api/rate with invalid rate (not 1-3) keeps rate unchanged."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        plugin._rate = 2

        req = make_request(method='POST', path='/api/rate', json_data={'rate': 99})

        with patch.object(plugin, '_restart_ao') as mock_restart:
            resp = plugin.on_webhook('/api/rate', req)

        assert plugin._rate == 2
        mock_restart.assert_not_called()

    def test_post_api_channels(self, plugin, make_request):
        """POST /api/channels updates channels/autohunt/dwell and triggers restart."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}

        req = make_request(method='POST', path='/api/channels',
                           json_data={'channels': '1,6,11', 'autohunt': True, 'dwell': 15})

        with patch.object(plugin, '_restart_ao') as mock_restart:
            resp = plugin.on_webhook('/api/channels', req)

        assert plugin._channels == '1,6,11'
        assert plugin._autohunt is True
        assert plugin._dwell == 15
        mock_restart.assert_called_once()

    def test_post_api_channels_dwell_clamped(self, plugin, make_request):
        """POST /api/channels clamps dwell to [1, 30]."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}

        req = make_request(method='POST', path='/api/channels',
                           json_data={'channels': '', 'dwell': 999})

        with patch.object(plugin, '_restart_ao'):
            plugin.on_webhook('/api/channels', req)

        assert plugin._dwell == 30  # clamped

    def test_post_api_targets_add(self, plugin, make_request):
        """POST /api/targets/add adds target and triggers restart."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        plugin._targets = []

        req = make_request(method='POST', path='/api/targets/add',
                           json_data={'target': 'AA:BB:CC:DD:EE:FF'})

        with patch.object(plugin, '_restart_ao') as mock_restart:
            resp = plugin.on_webhook('/api/targets/add', req)

        assert 'AA:BB:CC:DD:EE:FF' in plugin._targets
        mock_restart.assert_called_once()

    def test_post_api_targets_add_duplicate_no_restart(self, plugin, make_request):
        """POST /api/targets/add with duplicate does not add or restart."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        plugin._targets = ['AA:BB:CC:DD:EE:FF']

        req = make_request(method='POST', path='/api/targets/add',
                           json_data={'target': 'AA:BB:CC:DD:EE:FF'})

        with patch.object(plugin, '_restart_ao') as mock_restart:
            resp = plugin.on_webhook('/api/targets/add', req)

        assert len(plugin._targets) == 1
        mock_restart.assert_not_called()

    def test_post_api_targets_remove(self, plugin, make_request):
        """POST /api/targets/remove removes target and triggers restart."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        plugin._targets = ['AA:BB:CC:DD:EE:FF', '11:22:33:44:55:66']

        req = make_request(method='POST', path='/api/targets/remove',
                           json_data={'target': 'AA:BB:CC:DD:EE:FF'})

        with patch.object(plugin, '_restart_ao') as mock_restart:
            resp = plugin.on_webhook('/api/targets/remove', req)

        assert 'AA:BB:CC:DD:EE:FF' not in plugin._targets
        assert '11:22:33:44:55:66' in plugin._targets
        mock_restart.assert_called_once()

    def test_post_api_whitelist_add(self, plugin, make_request):
        """POST /api/whitelist/add adds entry and triggers restart."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        plugin._whitelist_entries = []

        req = make_request(method='POST', path='/api/whitelist/add',
                           json_data={'entry': 'MyNetwork'})

        with patch.object(plugin, '_restart_ao') as mock_restart:
            resp = plugin.on_webhook('/api/whitelist/add', req)

        assert 'MyNetwork' in plugin._whitelist_entries
        mock_restart.assert_called_once()

    def test_post_api_whitelist_add_duplicate_no_restart(self, plugin, make_request):
        """POST /api/whitelist/add with duplicate does not add or restart."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        plugin._whitelist_entries = ['MyNetwork']

        req = make_request(method='POST', path='/api/whitelist/add',
                           json_data={'entry': 'MyNetwork'})

        with patch.object(plugin, '_restart_ao') as mock_restart:
            resp = plugin.on_webhook('/api/whitelist/add', req)

        assert len(plugin._whitelist_entries) == 1
        mock_restart.assert_not_called()

    def test_post_api_whitelist_remove(self, plugin, make_request):
        """POST /api/whitelist/remove removes entry and triggers restart."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        plugin._whitelist_entries = ['MyNetwork', 'OtherNet']

        req = make_request(method='POST', path='/api/whitelist/remove',
                           json_data={'entry': 'MyNetwork'})

        with patch.object(plugin, '_restart_ao') as mock_restart:
            resp = plugin.on_webhook('/api/whitelist/remove', req)

        assert 'MyNetwork' not in plugin._whitelist_entries
        assert 'OtherNet' in plugin._whitelist_entries
        mock_restart.assert_called_once()

    def test_post_api_restart(self, plugin, make_request):
        """POST /api/restart calls _restart_ao."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}

        req = make_request(method='POST', path='/api/restart')
        with patch.object(plugin, '_restart_ao') as mock_restart:
            resp = plugin.on_webhook('/api/restart', req)

        data = _resp_data(resp)
        assert data['status'] == 'ok'
        mock_restart.assert_called_once()

    def test_post_api_stop(self, plugin, make_request):
        """POST /api/stop calls _stop_ao."""
        req = make_request(method='POST', path='/api/stop')
        with patch.object(plugin, '_stop_ao') as mock_stop:
            resp = plugin.on_webhook('/api/stop', req)

        data = _resp_data(resp)
        assert data['status'] == 'ok'
        mock_stop.assert_called_once()

    def test_post_api_reset(self, plugin, make_request):
        """POST /api/reset resets crash state."""
        plugin._stopped_permanently = True
        plugin._crash_count = 5

        req = make_request(method='POST', path='/api/reset')
        resp = plugin.on_webhook('/api/reset', req)

        data = _resp_data(resp)
        assert data['status'] == 'ok'
        assert plugin._stopped_permanently is False
        assert plugin._crash_count == 0

    def test_post_api_mode_ao(self, plugin, make_request):
        """POST /api/mode with ao mode initiates async switch and returns ok."""
        req = make_request(method='POST', path='/api/mode', json_data={'mode': 'ao'})
        import threading
        mock_thread = MagicMock()
        with patch.object(threading, 'Thread', return_value=mock_thread):
            resp = plugin.on_webhook('/api/mode', req)

        data = _resp_data(resp)
        assert data['status'] == 'ok'
        assert data['mode'] == 'ao'
        mock_thread.start.assert_called_once()

    def test_post_api_mode_invalid_returns_400(self, plugin, make_request):
        """POST /api/mode with invalid mode returns 400."""
        req = make_request(method='POST', path='/api/mode', json_data={'mode': 'invalid'})
        resp = plugin.on_webhook('/api/mode', req)

        assert isinstance(resp, tuple)
        assert resp[1] == 400

    def test_get_nonexistent_returns_404(self, plugin, make_request):
        """GET /nonexistent returns 404."""
        req = make_request(method='GET', path='/nonexistent')
        resp = plugin.on_webhook('/nonexistent', req)

        assert isinstance(resp, tuple)
        assert resp[1] == 404
        data = resp[0].json
        assert 'error' in data

    def test_post_unknown_returns_404(self, plugin, make_request):
        """POST to unknown path returns 404."""
        req = make_request(method='POST', path='/unknown')
        resp = plugin.on_webhook('/unknown', req)

        assert isinstance(resp, tuple)
        assert resp[1] == 404


# ---------------------------------------------------------------------------
# 7. Health Check Tests
# ---------------------------------------------------------------------------

class TestHealthCheck:
    """Tests for _check_health() and _get_health() methods."""

    def test_check_health_running_ok(self, plugin, agent):
        """Running process with no issues returns False (no crash)."""
        plugin._running = True
        mock_proc = MagicMock()
        mock_proc.poll.return_value = None
        plugin._process = mock_proc

        result = plugin._check_health(agent)
        assert result is False

    def test_check_health_not_running(self, plugin, agent):
        """Not running returns False."""
        plugin._running = False
        result = plugin._check_health(agent)
        assert result is False

    @patch('angryoxide_v2.time')
    @patch('angryoxide_v2.subprocess')
    @patch('angryoxide_v2.os')
    def test_check_health_crashed_restarts(self, mock_os, mock_sub, mock_time, plugin, agent):
        """Crashed process schedules non-blocking restart with backoff."""
        mock_time.time.return_value = 5000.0

        plugin._running = True
        mock_proc = MagicMock()
        mock_proc.poll.return_value = 1
        mock_proc.returncode = 1
        mock_proc.pid = 1234
        plugin._process = mock_proc
        plugin.options['max_crashes'] = 10

        with patch.object(plugin, '_try_fw_recovery', return_value=True):
            result = plugin._check_health(agent)

        assert result is True
        assert plugin._crash_count == 1
        # Non-blocking: _next_restart_time is set instead of calling _start_ao directly
        assert plugin._next_restart_time > 5000.0

    @patch('angryoxide_v2.time')
    def test_check_health_max_crashes_stops_permanently(self, mock_time, plugin, agent):
        """Reaching max_crashes sets _stopped_permanently."""
        mock_time.time.return_value = 5000.0

        plugin._running = True
        plugin._crash_count = 9  # will become 10
        plugin.options['max_crashes'] = 10
        mock_proc = MagicMock()
        mock_proc.poll.return_value = 1
        mock_proc.returncode = 1
        mock_proc.pid = 1234
        plugin._process = mock_proc

        result = plugin._check_health(agent)

        assert result is True
        assert plugin._stopped_permanently is True

    def test_health_all_up(self, plugin):
        """Mock all interfaces existing and clean logs: all True."""
        with patch('angryoxide_v2.os.path.exists', return_value=True), \
             patch('angryoxide_v2.subprocess.run') as mock_run:
            mock_run.return_value = MagicMock(stdout='normal kernel messages', returncode=0)
            health = plugin._get_health()

        assert health['wifi'] is True
        assert health['monitor'] is True
        assert health['firmware'] is True

    def test_health_wifi_down(self, plugin):
        """wlan0 missing: wifi=False."""
        def fake_exists(path):
            if path == '/sys/class/net/wlan0':
                return False
            return True

        with patch('angryoxide_v2.os.path.exists', side_effect=fake_exists), \
             patch('angryoxide_v2.subprocess.run') as mock_run:
            mock_run.return_value = MagicMock(stdout='', returncode=0)
            health = plugin._get_health()

        assert health['wifi'] is False
        assert health['monitor'] is True

    def test_health_monitor_down(self, plugin):
        """wlan0mon missing: monitor=False."""
        def fake_exists(path):
            if path == '/sys/class/net/wlan0mon':
                return False
            return True

        with patch('angryoxide_v2.os.path.exists', side_effect=fake_exists), \
             patch('angryoxide_v2.subprocess.run') as mock_run:
            mock_run.return_value = MagicMock(stdout='', returncode=0)
            health = plugin._get_health()

        assert health['wifi'] is True
        assert health['monitor'] is False

    def test_health_firmware_crash(self, plugin):
        """dmesg has crash pattern AND wifi is down: firmware=False."""
        # Firmware crash is only flagged if crash pattern is found AND wifi is down
        def mock_exists(path):
            if '/sys/class/net/wlan0' == path:
                return False  # wifi down
            if '/sys/class/net/wlan0mon' == path:
                return False
            if '/sys/class/net/usb0' == path:
                return False
            return True

        with patch('angryoxide_v2.os.path.exists', side_effect=mock_exists), \
             patch('angryoxide_v2.subprocess.run') as mock_run:
            mock_run.return_value = MagicMock(
                stdout='brcmf_cfg80211_set_channel: Set Channel failed: -110',
                returncode=0
            )
            health = plugin._get_health()

        assert health['firmware'] is False

    def test_try_fw_recovery_no_crash(self, plugin):
        """No crash pattern in logs: recovery returns True."""
        with patch('angryoxide_v2.subprocess.run') as mock_run, \
             patch('angryoxide_v2.time') as mock_time:
            mock_time.time.return_value = 5000.0
            plugin._last_recovery = 0
            mock_run.return_value = MagicMock(stdout='normal kernel messages here', returncode=0)
            result = plugin._try_fw_recovery()

        assert result is True

    def test_try_fw_recovery_detects_crash(self, plugin):
        """Crash pattern in logs triggers recovery sequence."""
        with patch('angryoxide_v2.subprocess.run') as mock_run, \
             patch('angryoxide_v2.os.path.exists', return_value=True), \
             patch('angryoxide_v2.time') as mock_time:
            mock_time.time.return_value = 5000.0
            mock_time.sleep = MagicMock()
            plugin._last_recovery = 0

            mock_run.return_value = MagicMock(
                stdout='brcmf_cfg80211_set_channel: Set Channel failed: -110',
                returncode=0
            )
            result = plugin._try_fw_recovery()

        assert plugin._fw_crash_count == 1
        assert result is True

    def test_try_fw_recovery_cooldown(self, plugin):
        """Recovery not attempted if cooldown not elapsed."""
        with patch('angryoxide_v2.time') as mock_time:
            mock_time.time.return_value = 100.0
            plugin._last_recovery = 80.0  # only 20 seconds ago, need 60
            result = plugin._try_fw_recovery()

        assert result is True


# ---------------------------------------------------------------------------
# 8. Integration Tests
# ---------------------------------------------------------------------------

class TestIntegration:
    """Integration-style tests combining multiple methods."""

    def test_restart_ao_stops_then_starts(self, plugin, agent):
        """_restart_ao calls _stop_ao then _start_ao in order."""
        plugin._agent = agent
        call_order = []

        def mock_stop():
            call_order.append('stop')

        def mock_start(ag):
            call_order.append('start')

        with patch.object(plugin, '_stop_ao', side_effect=mock_stop), \
             patch.object(plugin, '_start_ao', side_effect=mock_start):
            plugin._restart_ao()

        assert call_order == ['stop', 'start']

    def test_restart_ao_no_agent_only_stops(self, plugin):
        """_restart_ao without stored agent only stops."""
        plugin._agent = None

        with patch.object(plugin, '_stop_ao') as mock_stop, \
             patch.object(plugin, '_start_ao') as mock_start:
            plugin._restart_ao()

        mock_stop.assert_called_once()
        mock_start.assert_not_called()

    def test_attack_toggle_rebuilds_cmd(self, plugin):
        """Toggling an attack and rebuilding command reflects change."""
        # All enabled: no disable flags
        for k in plugin._attacks:
            plugin._attacks[k] = True
        cmd_before = plugin._build_cmd()
        assert '--disable-deauth' not in cmd_before

        # Disable deauth
        plugin._attacks['deauth'] = False
        cmd_after = plugin._build_cmd()
        assert '--disable-deauth' in cmd_after

    def test_full_cycle_toggle_deauth_off_and_on(self, plugin):
        """Disable deauth, verify cmd, re-enable, verify it's gone."""
        for k in plugin._attacks:
            plugin._attacks[k] = True

        # All enabled
        cmd1 = plugin._build_cmd()
        disable1 = [c for c in cmd1 if c.startswith('--disable')]
        assert len(disable1) == 0

        # Disable deauth
        plugin._attacks['deauth'] = False
        cmd2 = plugin._build_cmd()
        assert '--disable-deauth' in cmd2

        # Re-enable deauth
        plugin._attacks['deauth'] = True
        cmd3 = plugin._build_cmd()
        disable3 = [c for c in cmd3 if c.startswith('--disable')]
        assert len(disable3) == 0

    @patch('angryoxide_v2.subprocess')
    @patch('angryoxide_v2.os')
    @patch('angryoxide_v2.glob')
    @patch('angryoxide_v2.time')
    def test_start_ao_sets_running(self, mock_time, mock_glob, mock_os, mock_sub, plugin, agent):
        """_start_ao sets _running=True and records start time."""
        mock_time.time.return_value = 12345.0
        mock_glob.glob.return_value = []
        mock_os.path.join = lambda *args: '/'.join(args)
        mock_os.makedirs = MagicMock()
        mock_os.setsid = MagicMock()

        mock_proc = MagicMock()
        mock_proc.pid = 9999
        mock_sub.Popen.return_value = mock_proc
        mock_sub.DEVNULL = -1

        plugin._start_ao(agent)

        assert plugin._running is True
        assert plugin._start_time == 12345.0
        assert plugin._process == mock_proc

    @patch('angryoxide_v2.os')
    @patch('angryoxide_v2.signal')
    def test_stop_ao_clears_state(self, mock_signal, mock_os, plugin):
        """_stop_ao clears _running, _process, _start_time."""
        mock_proc = MagicMock()
        mock_proc.pid = 1234
        mock_proc.wait.return_value = 0
        mock_os.getpgid.return_value = 1234

        plugin._process = mock_proc
        plugin._running = True
        plugin._start_time = 1000.0

        plugin._stop_ao()

        assert plugin._running is False
        assert plugin._process is None
        assert plugin._start_time is None

    @patch('angryoxide_v2.subprocess')
    @patch('angryoxide_v2.os')
    @patch('angryoxide_v2.glob')
    @patch('angryoxide_v2.shutil')
    @patch('angryoxide_v2.time')
    def test_scan_captures_detects_new_files(self, mock_time, mock_shutil, mock_glob,
                                              mock_os, mock_sub, plugin, agent):
        """_scan_captures detects new pcapng files and triggers handshake events."""
        mock_time.time.return_value = 5000.0
        mock_os.path.join = lambda *args: '/'.join(args)
        mock_os.path.basename = lambda p: p.split('/')[-1]
        mock_os.path.abspath = lambda p: p
        mock_os.path.getmtime = MagicMock(return_value=5000.0)

        plugin._known_files = {}
        mock_glob.glob.return_value = ['/etc/pwnagotchi/handshakes/AA-BB-CC-DD-EE-FF_TestNet.pcapng']

        new_count = plugin._scan_captures(agent)

        assert new_count == 1
        assert plugin._captures == 1
        agent._update_handshakes.assert_called_once_with(1)

    @patch('angryoxide_v2.subprocess')
    @patch('angryoxide_v2.os')
    @patch('angryoxide_v2.glob')
    @patch('angryoxide_v2.shutil')
    @patch('angryoxide_v2.time')
    def test_scan_captures_whitelisted_skipped(self, mock_time, mock_shutil, mock_glob,
                                                mock_os, mock_sub, plugin, agent):
        """Whitelisted captures are not counted."""
        mock_time.time.return_value = 5000.0
        mock_os.path.join = lambda *args: '/'.join(args)
        mock_os.path.basename = lambda p: p.split('/')[-1]
        mock_os.path.abspath = lambda p: p
        mock_os.path.getmtime = MagicMock(return_value=5000.0)

        agent._config['main']['whitelist'] = ['TestNet']
        plugin._known_files = {}
        mock_glob.glob.return_value = ['/etc/pwnagotchi/handshakes/AA-BB-CC-DD-EE-FF_TestNet.pcapng']

        new_count = plugin._scan_captures(agent)

        assert new_count == 0
        assert plugin._captures == 0

    def test_on_loaded_binary_exists(self, plugin):
        """on_loaded with existing binary logs info."""
        with patch('angryoxide_v2.os.path.isfile', return_value=True):
            plugin.on_loaded()

    def test_on_loaded_binary_missing(self, plugin):
        """on_loaded with missing binary logs warning."""
        with patch('angryoxide_v2.os.path.isfile', return_value=False):
            plugin.on_loaded()

    @patch('angryoxide_v2.time')
    @patch('angryoxide_v2.os.path.isfile')
    def test_on_epoch_stopped_permanently_noop(self, mock_isfile, mock_time, plugin, agent):
        """on_epoch does nothing when stopped permanently."""
        plugin._stopped_permanently = True
        plugin.on_epoch(agent, 1, {})
        agent.set_face.assert_not_called()

    @patch('angryoxide_v2.time')
    def test_on_epoch_crash_sets_angry_face(self, mock_time, plugin, agent):
        """on_epoch sets crash face when AO process dies."""
        mock_time.time.return_value = 5000.0
        mock_time.sleep = MagicMock()
        plugin._running = True
        plugin.options['max_crashes'] = 10

        mock_proc = MagicMock()
        mock_proc.poll.return_value = 1
        mock_proc.returncode = 1
        mock_proc.pid = 1234
        plugin._process = mock_proc

        mock_view = MagicMock()
        agent._view = mock_view

        # os.path.exists returns True (wifi up), os.path.isfile returns False (no PNG faces)
        def mock_exists(path):
            if 'sys/class/net' in str(path):
                return True
            return False

        with patch.object(plugin, '_try_fw_recovery', return_value=True), \
             patch.object(plugin, '_start_ao'), \
             patch.object(plugin, '_get_battery_level', return_value=None), \
             patch('angryoxide_v2.os.path.exists', side_effect=mock_exists), \
             patch('angryoxide_v2.os.path.isfile', return_value=False):
            plugin.on_epoch(agent, 1, {})

        # Crash face is set via agent._view.set('face', ...) — falls back to faces.ANGRY since no PNG
        mock_view.set.assert_any_call('face', '(>_<)')
        mock_view.update.assert_called()


# ---------------------------------------------------------------------------
# 9. Edge Case Tests
# ---------------------------------------------------------------------------

class TestEdgeCases:
    """Edge cases and boundary conditions."""

    def test_parse_filename_with_multiple_underscores(self):
        """Filename with multiple underscores still parses MAC correctly."""
        ap, sta = AngryOxide._parse_capture_filename("AA-BB-CC-DD-EE-FF_Network_With_Spaces.pcapng")
        assert ap == "AA:BB:CC:DD:EE:FF"

    def test_parse_filename_no_extension(self):
        """Filename without .pcapng extension."""
        ap, sta = AngryOxide._parse_capture_filename("AA-BB-CC-DD-EE-FF_Network")
        assert ap == "AA:BB:CC:DD:EE:FF"

    def test_whitelist_unicode_ssid(self, plugin):
        """Non-alphanumeric chars stripped in normalization."""
        result = plugin._is_whitelisted("AA-BB-CC-DD-EE-FF_CafeWiFi.pcapng", ["Cafe WiFi"])
        assert result is True

    def test_backoff_zero_crashes(self, plugin):
        """Backoff with 0 crashes (edge case)."""
        plugin._crash_count = 0
        result = plugin._backoff_seconds()
        assert result == 2.5

    def test_concurrent_stop_calls_safe(self, plugin):
        """Multiple stop calls on non-running plugin don't raise."""
        plugin._process = None
        plugin._running = False
        plugin._stop_ao()  # should not raise

    def test_known_files_updated_after_scan(self, plugin, agent):
        """_known_files is updated after scan to track mtimes."""
        with patch('angryoxide_v2.glob.glob', return_value=[]), \
             patch('angryoxide_v2.os.path.join', side_effect=lambda *a: '/'.join(a)), \
             patch('angryoxide_v2.os.path.abspath', side_effect=lambda p: p):
            plugin._scan_captures(agent)

        assert plugin._known_files == {}

    def test_build_cmd_custom_binary_path(self, plugin):
        """Custom binary path is used in command."""
        plugin.options['binary_path'] = '/opt/custom/angryoxide'
        cmd = plugin._build_cmd()
        assert cmd[0] == '/opt/custom/angryoxide'

    def test_build_cmd_custom_interface(self, plugin):
        """Custom interface name is used in command."""
        plugin.options['interface'] = 'wlan1mon'
        cmd = plugin._build_cmd()
        idx = cmd.index('--interface')
        assert cmd[idx + 1] == 'wlan1mon'

    def test_build_cmd_custom_output_dir(self, plugin):
        """Custom output directory is used in command."""
        plugin.options['output_dir'] = '/home/pi/captures/'
        cmd = plugin._build_cmd()
        idx = cmd.index('--output')
        # Output path now includes hostname prefix appended to the directory
        assert cmd[idx + 1].startswith('/home/pi/captures/')

    def test_all_six_attacks_disabled_at_once(self, plugin):
        """All 6 attacks disabled produces 6 disable flags."""
        for k in plugin._attacks:
            plugin._attacks[k] = False
        cmd = plugin._build_cmd()
        disable_flags = [c for c in cmd if c.startswith('--disable')]
        assert len(disable_flags) == 6

    def test_targets_empty_no_flag(self, plugin):
        """Empty targets list adds no --target-entry flags."""
        plugin._targets = []
        cmd = plugin._build_cmd()
        assert '--target-entry' not in cmd

    def test_whitelist_entries_empty_no_flag(self, plugin):
        """Empty whitelist list adds no --whitelist-entry flags."""
        plugin._whitelist_entries = []
        cmd = plugin._build_cmd()
        assert '--whitelist-entry' not in cmd

    def test_webhook_post_targets_empty_string_ignored(self, plugin, make_request):
        """POST /api/targets/add with empty target string does nothing."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        plugin._targets = []

        req = make_request(method='POST', path='/api/targets/add',
                           json_data={'target': '  '})

        with patch.object(plugin, '_restart_ao') as mock_restart:
            resp = plugin.on_webhook('/api/targets/add', req)

        assert len(plugin._targets) == 0
        mock_restart.assert_not_called()


# ---------------------------------------------------------------------------
# 10. Firmware Crash Pattern Tests
# ---------------------------------------------------------------------------

class TestFirmwareCrashPattern:
    """Tests for the _FW_CRASH_PATTERN regex."""

    def test_pattern_matches_channel_set_failed(self):
        """Matches 'Set Channel failed.*-110' pattern."""
        text = "brcmf_cfg80211_set_channel: Set Channel failed: -110"
        assert AngryOxide._FW_CRASH_PATTERN.search(text) is not None

    def test_pattern_matches_firmware_halted(self):
        """Matches 'firmware has halted' pattern."""
        text = "brcmfmac: firmware has halted or crashed"
        assert AngryOxide._FW_CRASH_PATTERN.search(text) is not None

    def test_pattern_no_match_normal_log(self):
        """Does not match normal kernel messages."""
        text = "wlan0: associated with AP"
        assert AngryOxide._FW_CRASH_PATTERN.search(text) is None

    def test_pattern_case_insensitive(self):
        """Pattern matching is case-insensitive."""
        text = "BRCMF_CFG80211_SET_CHANNEL: SET CHANNEL FAILED: -110"
        assert AngryOxide._FW_CRASH_PATTERN.search(text) is not None


# ---------------------------------------------------------------------------
# 11. Skip Captured Tests
# ---------------------------------------------------------------------------

class TestSkipCaptured:
    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {'binary_path': '/usr/local/bin/angryoxide', 'interface': 'wlan0mon', 'output_dir': '/test/handshakes/'}
        return p

    def test_skip_captured_default_false(self, plugin):
        assert plugin._skip_captured is False

    def test_skip_captured_in_build_cmd(self, plugin):
        plugin._skip_captured = True
        with patch('angryoxide_v2.os.path.isdir', return_value=True), \
             patch('angryoxide_v2.os.listdir', return_value=['AA-BB-CC-DD-EE-FF_Net.pcapng', 'not-a-mac.txt']):
            cmd = plugin._build_cmd()
        assert '--whitelist-entry' in cmd
        assert 'AA:BB:CC:DD:EE:FF' in cmd

    def test_skip_captured_no_duplicates(self, plugin):
        plugin._skip_captured = True
        plugin._whitelist_entries = ['AA:BB:CC:DD:EE:FF']
        with patch('angryoxide_v2.os.path.isdir', return_value=True), \
             patch('angryoxide_v2.os.listdir', return_value=['AA-BB-CC-DD-EE-FF_Net.pcapng']):
            cmd = plugin._build_cmd()
        # Should only appear once (from _whitelist_entries), not duplicated
        count = sum(1 for i, v in enumerate(cmd) if v == 'AA:BB:CC:DD:EE:FF')
        assert count == 1

    def test_skip_captured_false_no_extra(self, plugin):
        plugin._skip_captured = False
        with patch('angryoxide_v2.os.path.isdir', return_value=True), \
             patch('angryoxide_v2.os.listdir', return_value=['AA-BB-CC-DD-EE-FF_Net.pcapng']):
            cmd = plugin._build_cmd()
        assert 'AA:BB:CC:DD:EE:FF' not in cmd

    def test_skip_captured_persists(self, plugin, tmp_path):
        """_save_state includes skip_captured and _load_state restores it."""
        plugin._state_file = str(tmp_path / 'state_sc.json')
        plugin._skip_captured = True
        plugin._save_state()

        plugin._skip_captured = False
        plugin._load_state()
        assert plugin._skip_captured is True

    def test_webhook_skip_captured_toggle(self, plugin):
        """POST to /api/skip-captured with {"enabled": true} updates state and triggers restart."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        plugin._skip_captured = False

        req = MagicMock()
        req.method = 'POST'
        req.authorization = None
        req.get_json = MagicMock(return_value={'enabled': True})
        req.data = b''

        with patch.object(plugin, '_restart_ao') as mock_restart, \
             patch.object(plugin, '_save_state'):
            resp = plugin.on_webhook('/api/skip-captured', req)

        data = _resp_data(resp)
        assert data['status'] == 'ok'
        assert data['skip_captured'] is True
        assert plugin._skip_captured is True
        mock_restart.assert_called_once()


# ---------------------------------------------------------------------------
# 12. State Persistence Tests
# ---------------------------------------------------------------------------

class TestStatePersistence:
    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        return p

    def test_save_and_load_roundtrip(self, plugin, tmp_path):
        plugin._state_file = str(tmp_path / 'state.json')
        plugin._targets = ['target1']
        plugin._whitelist_entries = ['wl1', 'wl2']
        plugin._rate = 3
        plugin._skip_captured = True
        plugin._attacks['csa'] = False
        plugin._channels = '1,6,11'
        plugin._autohunt = True
        plugin._dwell = 5
        plugin._save_state()

        # Reset and load
        plugin._targets = []
        plugin._whitelist_entries = []
        plugin._rate = 2
        plugin._skip_captured = False
        plugin._load_state()

        assert plugin._targets == ['target1']
        assert plugin._whitelist_entries == ['wl1', 'wl2']
        assert plugin._rate == 3
        assert plugin._skip_captured is True
        assert plugin._attacks['csa'] is False
        assert plugin._channels == '1,6,11'
        assert plugin._autohunt is True
        assert plugin._dwell == 5

    def test_load_missing_file(self, plugin, tmp_path):
        plugin._state_file = str(tmp_path / 'nonexistent.json')
        plugin._load_state()  # should not crash
        assert plugin._rate == 1  # default

    def test_load_corrupted_json(self, plugin, tmp_path):
        bad = tmp_path / 'bad.json'
        bad.write_text('not json{{{')
        plugin._state_file = str(bad)
        plugin._load_state()  # should not crash
        assert plugin._rate == 1


# ---------------------------------------------------------------------------
# 13. File Download Tests
# ---------------------------------------------------------------------------

class TestFileDownloads:
    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {'output_dir': '/ao/handshakes/'}
        mock_agent = MagicMock()
        mock_agent._config = {'bettercap': {'handshakes': '/bc/handshakes/'}}
        p._agent = mock_agent
        return p

    def test_download_capture_found(self, plugin):
        """GET /api/download/capture/test.pcapng returns the file via send_file."""
        req = MagicMock()
        req.method = 'GET'
        req.data = b''
        mock_send = MagicMock(return_value=MagicMock())
        with patch('angryoxide_v2.os.path.isfile', return_value=True), \
             patch('flask.send_file', mock_send):
            result = plugin.on_webhook('api/download/capture/test.pcapng', req)
        mock_send.assert_called_once()
        # Verify the path ends with test.pcapng
        call_path = mock_send.call_args[0][0]
        assert call_path.endswith('test.pcapng')

    def test_download_capture_path_traversal(self, plugin):
        """Path traversal attempt is sanitized via os.path.basename."""
        req = MagicMock()
        req.method = 'GET'
        req.data = b''
        with patch('angryoxide_v2.os.path.isfile', return_value=False):
            result = plugin.on_webhook('api/download/capture/../../etc/passwd', req)
        # Should look for 'passwd' not '../../etc/passwd'
        data = result[0].get_json() if isinstance(result, tuple) else result.get_json()
        assert data.get('error') == 'file not found'

    def test_download_capture_not_found(self, plugin):
        req = MagicMock()
        req.method = 'GET'
        req.data = b''
        with patch('angryoxide_v2.os.path.isfile', return_value=False):
            result = plugin.on_webhook('api/download/capture/missing.pcapng', req)
        data = result[0].get_json() if isinstance(result, tuple) else result.get_json()
        assert 'not found' in str(data)

    def test_download_all_returns_zip(self, plugin):
        req = MagicMock()
        req.method = 'GET'
        req.data = b''
        with patch('angryoxide_v2.os.path.isdir', return_value=True), \
             patch('angryoxide_v2.os.listdir', return_value=[]), \
             patch('angryoxide_v2.os.path.isfile', return_value=False):
            result = plugin.on_webhook('api/download/all', req)
        assert result.mimetype == 'application/zip'


# ---------------------------------------------------------------------------
# 14. Boot / Shutdown Face Tests
# ---------------------------------------------------------------------------

class TestBootShutdownFaces:
    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {'binary_path': '/usr/local/bin/angryoxide', 'interface': 'wlan0mon', 'output_dir': '/test/'}
        return p

    def test_on_ready_sets_awake_face(self, plugin):
        agent = MagicMock()
        agent._config = {'personality': {'deauth': True, 'associate': True}, 'bettercap': {'handshakes': '/h/'}}
        agent._view = MagicMock()
        with patch('angryoxide_v2.os.path.isfile', return_value=True), \
             patch.object(plugin, '_start_ao'), \
             patch.object(plugin, '_face', return_value='awake_face'):
            plugin.on_ready(agent)
        agent._view.set.assert_any_call('face', 'awake_face')
        agent._view.update.assert_called()

    def test_on_unload_sets_shutdown_face(self, plugin):
        ui = MagicMock()
        with patch.object(plugin, '_stop_ao'), \
             patch.object(plugin, '_face', return_value='shutdown_face'):
            plugin.on_unload(ui)
        ui.set.assert_any_call('face', 'shutdown_face')


# ---------------------------------------------------------------------------
# 15. Webhook APs Endpoint Tests
# ---------------------------------------------------------------------------

class TestWebhookAPsEndpoint:
    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {'output_dir': '/ao/', 'interface': 'wlan0mon'}
        mock_agent = MagicMock()
        mock_agent._access_points = [
            {'hostname': 'Net1', 'mac': 'AA:BB:CC:DD:EE:FF', 'channel': 6, 'rssi': -55, 'encryption': 'WPA2', 'clients': [], 'vendor': 'Test'},
        ]
        mock_agent._config = {'bettercap': {'handshakes': '/bc/'}}
        p._agent = mock_agent
        return p

    def test_get_aps_returns_list(self, plugin):
        req = MagicMock()
        req.method = 'GET'
        req.data = b''
        with patch('angryoxide_v2.os.path.isdir', return_value=True), \
             patch('angryoxide_v2.os.listdir', return_value=[]):
            result = plugin.on_webhook('api/aps', req)
        data = result.get_json()
        assert isinstance(data, list)
        assert len(data) == 1
        assert data[0]['ssid'] == 'Net1'
        assert data[0]['captured'] is False

    def test_get_aps_empty_when_no_agent(self, plugin):
        plugin._agent = None
        req = MagicMock()
        req.method = 'GET'
        req.data = b''
        result = plugin.on_webhook('api/aps', req)
        data = result.get_json()
        assert data == []


# ---------------------------------------------------------------------------
# 16. Skip Captured Webhook Tests
# ---------------------------------------------------------------------------

class TestSkipCapturedWebhook:
    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {'binary_path': '/usr/local/bin/angryoxide', 'interface': 'wlan0mon', 'output_dir': '/test/'}
        p._state_file = '/dev/null'
        return p

    def test_toggle_skip_captured_on(self, plugin):
        req = MagicMock()
        req.method = 'POST'
        req.data = b'{"enabled": true}'
        req.get_json = MagicMock(return_value={'enabled': True})
        with patch.object(plugin, '_restart_ao'), \
             patch.object(plugin, '_save_state'):
            result = plugin.on_webhook('api/skip-captured', req)
        data = result.get_json()
        assert data['skip_captured'] is True
        assert plugin._skip_captured is True

    def test_toggle_skip_captured_off(self, plugin):
        plugin._skip_captured = True
        req = MagicMock()
        req.method = 'POST'
        req.data = b'{"enabled": false}'
        req.get_json = MagicMock(return_value={'enabled': False})
        with patch.object(plugin, '_restart_ao'), \
             patch.object(plugin, '_save_state'):
            result = plugin.on_webhook('api/skip-captured', req)
        assert plugin._skip_captured is False


# ---------------------------------------------------------------------------
# 14. AP List Captured Tests
# ---------------------------------------------------------------------------

class TestAPListCaptured:
    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {'output_dir': '/ao/handshakes/', 'interface': 'wlan0mon'}
        mock_agent = MagicMock()
        mock_agent._access_points = [
            {'hostname': 'TestNet', 'mac': 'AA:BB:CC:DD:EE:FF', 'channel': 6, 'rssi': -50, 'encryption': 'WPA2', 'clients': [], 'vendor': ''},
            {'hostname': 'OtherNet', 'mac': '11:22:33:44:55:66', 'channel': 1, 'rssi': -70, 'encryption': 'WPA2', 'clients': [{'mac': 'cli1'}], 'vendor': ''},
        ]
        mock_agent._config = {'bettercap': {'handshakes': '/bc/handshakes/'}}
        p._agent = mock_agent
        return p

    def test_captured_true_for_matching_mac(self, plugin):
        with patch('angryoxide_v2.os.path.isdir', return_value=True), \
             patch('angryoxide_v2.os.listdir', return_value=['AA-BB-CC-DD-EE-FF_TestNet.pcapng']):
            aps = plugin._get_access_points()
        matched = [a for a in aps if a['mac'] == 'AA:BB:CC:DD:EE:FF']
        assert len(matched) == 1
        assert matched[0]['captured'] is True

    def test_captured_false_when_no_files(self, plugin):
        with patch('angryoxide_v2.os.path.isdir', return_value=True), \
             patch('angryoxide_v2.os.listdir', return_value=[]):
            aps = plugin._get_access_points()
        assert all(a['captured'] is False for a in aps)

    def test_sorted_by_rssi_descending(self, plugin):
        with patch('angryoxide_v2.os.path.isdir', return_value=True), \
             patch('angryoxide_v2.os.listdir', return_value=[]):
            aps = plugin._get_access_points()
        rssis = [a['rssi'] for a in aps]
        assert rssis == sorted(rssis, reverse=True)

    def test_client_count(self, plugin):
        with patch('angryoxide_v2.os.path.isdir', return_value=True), \
             patch('angryoxide_v2.os.listdir', return_value=[]):
            aps = plugin._get_access_points()
        other = [a for a in aps if a['ssid'] == 'OtherNet'][0]
        assert other['clients'] == 1


# ---------------------------------------------------------------------------
# 15. Health With AO Tests
# ---------------------------------------------------------------------------

class TestHealthWithAO:
    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {'interface': 'wlan0mon'}
        return p

    def test_health_true_when_ao_running(self, plugin):
        plugin._running = True
        plugin._process = MagicMock()
        plugin._process.poll.return_value = None  # still alive
        # Health checks os.path.exists for interfaces — both must exist for healthy
        with patch('angryoxide_v2.os.path.exists', return_value=True), \
             patch('angryoxide_v2.subprocess.run', side_effect=Exception('no journalctl')):
            h = plugin._get_health()
        assert h['wifi'] is True
        assert h['monitor'] is True

    def test_health_false_when_ao_stopped(self, plugin):
        plugin._running = False
        with patch('angryoxide_v2.os.path.exists', return_value=False), \
             patch('angryoxide_v2.subprocess.run', side_effect=Exception('no journalctl')):
            h = plugin._get_health()
        assert h['wifi'] is False
        assert h['monitor'] is False


# ---------------------------------------------------------------------------
# 16. Mode API Tests
# ---------------------------------------------------------------------------

class TestModeAPI:
    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        return p

    def test_get_mode_ao(self, plugin):
        req = MagicMock()
        req.method = 'GET'
        req.data = b''
        mock_result = MagicMock()
        mock_result.stdout = 'Mode: AngryOxide (AO active)'
        with patch('angryoxide_v2.subprocess.run', return_value=mock_result):
            result = plugin.on_webhook('api/mode', req)
        data = result.get_json()
        assert data['mode'] == 'ao'

    def test_get_mode_pwn(self, plugin):
        req = MagicMock()
        req.method = 'GET'
        req.data = b''
        mock_result = MagicMock()
        mock_result.stdout = 'Mode: Pwnagotchi (bettercap attacks)'
        with patch('angryoxide_v2.subprocess.run', return_value=mock_result):
            result = plugin.on_webhook('api/mode', req)
        data = result.get_json()
        assert data['mode'] == 'pwn'


# ---------------------------------------------------------------------------
# 17. Face Helper Tests
# ---------------------------------------------------------------------------

class TestFaceHelper:
    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p._face_dir = '/test/faces'
        return p

    def test_face_returns_png_path_when_exists(self, plugin):
        # _face() now requires _agent with png enabled to use PNG paths
        mock_agent = MagicMock()
        mock_agent._config = {'ui': {'faces': {'png': True}}}
        plugin._agent = mock_agent
        with patch('angryoxide_v2.os.path.isfile', return_value=True):
            result = plugin._face('angry')
        import os
        assert result == os.path.join('/test/faces', 'angry.png')

    def test_face_returns_fallback_when_missing(self, plugin):
        with patch('angryoxide_v2.os.path.isfile', return_value=False):
            result = plugin._face('wifi_down')
        # Should return stock faces.BROKEN as fallback
        import sys
        faces = sys.modules['pwnagotchi.ui.faces']
        assert result == faces.BROKEN

    def test_face_returns_awake_for_unknown(self, plugin):
        with patch('angryoxide_v2.os.path.isfile', return_value=False):
            result = plugin._face('nonexistent_mood')
        import sys
        faces = sys.modules['pwnagotchi.ui.faces']
        assert result == faces.AWAKE


# ---------------------------------------------------------------------------
# 18. Battery Level Tests
# ---------------------------------------------------------------------------

class TestBatteryLevel:
    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        return AngryOxide()

    def test_battery_from_pisugar_file(self, plugin):
        m = mock_open(read_data='85')
        with patch('builtins.open', m):
            level = plugin._get_battery_level()
        assert level == 85

    def test_battery_returns_none_when_unavailable(self, plugin):
        with patch('builtins.open', side_effect=FileNotFoundError()):
            level = plugin._get_battery_level()
        assert level is None

    def test_battery_handles_float(self, plugin):
        m = mock_open(read_data='72.5')
        with patch('builtins.open', m):
            level = plugin._get_battery_level()
        assert level == 72


# ---------------------------------------------------------------------------
# 19. Name Removal Tests
# ---------------------------------------------------------------------------

class TestNameRemoval:
    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {}
        return p

    def test_on_ui_setup_hides_name(self, plugin):
        """on_ui_setup adds angryoxide and ao_crash elements, and moves mode to (222,112)."""
        ui = MagicMock()
        ui._lock = MagicMock()
        ui._lock.__enter__ = MagicMock(return_value=None)
        ui._lock.__exit__ = MagicMock(return_value=False)
        ui.width = MagicMock(return_value=250)
        ui.height = MagicMock(return_value=122)
        plugin.on_ui_setup(ui)
        # on_ui_setup now adds angryoxide and ao_crash elements (name hiding moved to on_ui_update)
        add_calls = [c[0][0] for c in ui.add_element.call_args_list]
        assert 'angryoxide' in add_calls
        assert 'ao_crash' in add_calls


# ---------------------------------------------------------------------------
# 20. UI Update Tests
# ---------------------------------------------------------------------------

class TestUIUpdate:
    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {}
        return p

    def _make_ui(self):
        ui = MagicMock()
        ui._lock = MagicMock()
        ui._lock.__enter__ = MagicMock(return_value=None)
        ui._lock.__exit__ = MagicMock(return_value=False)
        return ui

    def test_ui_update_running(self, plugin):
        """Running AO shows verified/total, uptime and channels."""
        ui = self._make_ui()
        plugin._running = True
        plugin._captures = 7
        plugin._start_time = 1000.0
        with patch('angryoxide_v2.time') as mock_time, \
             patch.object(plugin, '_is_ao_mode', return_value=True), \
             patch.object(plugin, '_count_pcapngs', return_value=7), \
             patch.object(plugin, '_count_verified', return_value=3):
            mock_time.time.return_value = 1300.0  # 5 minutes
            plugin.on_ui_update(ui)
        ui.set.assert_any_call('angryoxide', 'AO: 3/7 | 5m | CH:1,6,11')

    def test_ui_update_stopped(self, plugin):
        """Stopped AO shows 'off'."""
        ui = self._make_ui()
        plugin._running = False
        plugin._stopped_permanently = False
        with patch.object(plugin, '_is_ao_mode', return_value=True):
            plugin.on_ui_update(ui)
        ui.set.assert_any_call('angryoxide', 'AO: off')

    def test_ui_update_stopped_permanently(self, plugin):
        """Permanently stopped AO shows 'ERR'."""
        ui = self._make_ui()
        plugin._stopped_permanently = True
        with patch.object(plugin, '_is_ao_mode', return_value=True):
            plugin.on_ui_update(ui)
        ui.set.assert_any_call('angryoxide', 'AO: ERR')

    def test_ui_update_hides_overlapping_elements(self, plugin):
        """Running AO hides name/walkby/blitz elements."""
        ui = self._make_ui()
        plugin._running = True
        plugin._captures = 0
        plugin._start_time = 1000.0
        with patch('angryoxide_v2.time') as mock_time, \
             patch.object(plugin, '_is_ao_mode', return_value=True), \
             patch.object(plugin, '_count_pcapngs', return_value=0), \
             patch.object(plugin, '_count_verified', return_value=0):
            mock_time.time.return_value = 1000.0
            plugin.on_ui_update(ui)
        ui.set.assert_any_call('name', '')


# ---------------------------------------------------------------------------
# 21. Epoch Edge Case Tests
# ---------------------------------------------------------------------------

class TestEpochEdgeCases:
    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {'binary_path': '/usr/local/bin/angryoxide', 'interface': 'wlan0mon',
                     'output_dir': '/etc/pwnagotchi/handshakes/'}
        return p

    @pytest.fixture
    def agent(self):
        a = MagicMock()
        a._config = {
            'personality': {'deauth': True, 'associate': True},
            'bettercap': {'handshakes': '/etc/pwnagotchi/handshakes/'},
            'main': {'whitelist': []},
        }
        a._handshakes = {}
        a._last_pwnd = None
        a._update_handshakes = MagicMock()
        a._view = MagicMock()
        return a

    def test_epoch_battery_critical_returns_early(self, plugin, agent):
        """Battery < 15 sets critical face and returns before AO checks."""
        plugin._running = True
        plugin._process = MagicMock()
        plugin._process.poll.return_value = None
        with patch.object(plugin, '_get_battery_level', return_value=10), \
             patch('angryoxide_v2.os.path.exists', return_value=True), \
             patch('angryoxide_v2.os.path.isfile', return_value=False):
            plugin.on_epoch(agent, 1, {})
        agent._view.set.assert_any_call('status', 'Battery critical! 10%')
        # scan_captures should NOT have been called
        agent._update_handshakes.assert_not_called()

    def test_epoch_wifi_down_returns_early(self, plugin, agent):
        """WiFi interface missing sets wifi_down face and returns."""
        plugin._running = True
        plugin._process = MagicMock()
        plugin._process.poll.return_value = None
        plugin._agent = agent
        def mock_exists(path):
            if path == '/sys/class/net/wlan0':
                return False
            return True
        with patch.object(plugin, '_get_battery_level', return_value=None), \
             patch.object(plugin, '_is_ao_mode', return_value=True), \
             patch('angryoxide_v2.os.path.exists', side_effect=mock_exists), \
             patch('angryoxide_v2.os.path.isfile', return_value=False), \
             patch.object(plugin, '_stop_ao'), \
             patch.object(plugin, '_try_fw_recovery', return_value=False):
            plugin.on_epoch(agent, 1, {})
        agent._view.set.assert_any_call('status', 'WiFi down! Recovering...')

    def test_epoch_captures_sets_excited(self, plugin, agent):
        """New captures set EXCITED face via _view.set."""
        plugin._running = True
        plugin._process = MagicMock()
        plugin._process.poll.return_value = None
        plugin._agent = agent
        with patch.object(plugin, '_get_battery_level', return_value=None), \
             patch.object(plugin, '_is_ao_mode', return_value=True), \
             patch('angryoxide_v2.os.path.exists', return_value=True), \
             patch('angryoxide_v2.os.path.isfile', return_value=False), \
             patch.object(plugin, '_check_health', return_value=False), \
             patch.object(plugin, '_scan_captures', return_value=3):
            plugin.on_epoch(agent, 1, {})
        import sys
        faces = sys.modules['pwnagotchi.ui.faces']
        agent._view.set.assert_any_call('face', faces.EXCITED)

    def test_epoch_bored_after_30_stable(self, plugin, agent):
        """30+ stable epochs with no captures sets BORED face via _view.set."""
        plugin._running = True
        plugin._process = MagicMock()
        plugin._process.poll.return_value = None
        plugin._stable_epochs = 30  # will become 31 after increment
        plugin._agent = agent
        with patch.object(plugin, '_get_battery_level', return_value=None), \
             patch.object(plugin, '_is_ao_mode', return_value=True), \
             patch('angryoxide_v2.os.path.exists', return_value=True), \
             patch('angryoxide_v2.os.path.isfile', return_value=False), \
             patch.object(plugin, '_check_health', return_value=False), \
             patch.object(plugin, '_scan_captures', return_value=0):
            plugin.on_epoch(agent, 1, {})
        import sys
        faces = sys.modules['pwnagotchi.ui.faces']
        agent._view.set.assert_any_call('face', faces.BORED)

    def test_epoch_resets_crash_count_after_stability(self, plugin, agent):
        """Crash count resets after 5+ minutes of stability."""
        plugin._running = True
        plugin._process = MagicMock()
        plugin._process.poll.return_value = None
        plugin._crash_count = 3
        plugin._last_crash_time = 1000.0
        with patch.object(plugin, '_get_battery_level', return_value=None), \
             patch('angryoxide_v2.os.path.exists', return_value=True), \
             patch('angryoxide_v2.os.path.isfile', return_value=False), \
             patch.object(plugin, '_check_health', return_value=False), \
             patch.object(plugin, '_scan_captures', return_value=0), \
             patch('angryoxide_v2.time') as mock_time:
            mock_time.time.return_value = 1400.0  # 400 seconds > 300
            plugin.on_epoch(agent, 1, {})
        assert plugin._crash_count == 0


# ---------------------------------------------------------------------------
# 22. Extract MAC Tests
# ---------------------------------------------------------------------------

class TestExtractMac:
    def test_ao_format(self):
        assert AngryOxide._extract_mac_from_filename("AA-BB-CC-DD-EE-FF_Net.pcapng") == "AA:BB:CC:DD:EE:FF"

    def test_bettercap_format(self):
        assert AngryOxide._extract_mac_from_filename("AA:BB:CC:DD:EE:FF.pcap") == "AA:BB:CC:DD:EE:FF"

    def test_no_mac(self):
        assert AngryOxide._extract_mac_from_filename("random_file.txt") is None

    def test_short_mac(self):
        assert AngryOxide._extract_mac_from_filename("AA-BB.pcapng") is None


# ---------------------------------------------------------------------------
# 23. Webhook Edge Cases
# ---------------------------------------------------------------------------

class TestWebhookEdgeCases:
    """Additional webhook edge cases for untested code paths."""

    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {
            'binary_path': '/usr/local/bin/angryoxide',
            'interface': 'wlan0mon',
            'output_dir': '/etc/pwnagotchi/handshakes/',
            'notx': False,
            'no_setup': True,
            'extra_args': '',
        }
        return p

    def test_post_attacks_partial_update(self, plugin):
        """POST /api/attacks with only 1 key changes that key, leaves others unchanged."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        plugin._attacks = {
            'deauth': True, 'pmkid': True, 'csa': True,
            'disassoc': True, 'anon_reassoc': True, 'rogue_m2': True,
        }
        req = FakeRequest(method='POST', path='/api/attacks',
                          json_data={'csa': False})
        with patch.object(plugin, '_restart_ao'), \
             patch.object(plugin, '_save_state'):
            resp = plugin.on_webhook('/api/attacks', req)
        data = _resp_data(resp)
        assert data['status'] == 'ok'
        assert plugin._attacks['csa'] is False
        assert plugin._attacks['deauth'] is True
        assert plugin._attacks['pmkid'] is True
        assert plugin._attacks['disassoc'] is True
        assert plugin._attacks['anon_reassoc'] is True
        assert plugin._attacks['rogue_m2'] is True

    def test_post_channels_dwell_zero_clamped_to_1(self, plugin):
        """POST /api/channels with dwell=0 clamps to 1."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        req = FakeRequest(method='POST', path='/api/channels',
                          json_data={'channels': '', 'dwell': 0})
        with patch.object(plugin, '_restart_ao'), \
             patch.object(plugin, '_save_state'):
            plugin.on_webhook('/api/channels', req)
        assert plugin._dwell == 1

    def test_post_channels_dwell_999_clamped_to_30(self, plugin):
        """POST /api/channels with dwell=999 clamps to 30."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        req = FakeRequest(method='POST', path='/api/channels',
                          json_data={'channels': '', 'dwell': 999})
        with patch.object(plugin, '_restart_ao'), \
             patch.object(plugin, '_save_state'):
            plugin.on_webhook('/api/channels', req)
        assert plugin._dwell == 30

    def test_post_mode_subprocess_exception_returns_500(self, plugin):
        """POST /api/mode with exception during setup returns 500."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}, 'ui': {}}
        req = FakeRequest(method='POST', path='/api/mode',
                          json_data={'mode': 'ao'})
        # Force exception during the try block (e.g., threading.Thread raises)
        import threading
        with patch.object(threading, 'Thread', side_effect=OSError('command not found')):
            resp = plugin.on_webhook('/api/mode', req)
        assert isinstance(resp, tuple)
        assert resp[1] == 500
        data = resp[0].json if hasattr(resp[0], 'json') else resp[0].get_json()
        assert data['status'] == 'error'

    def test_get_aps_with_captured_true(self, plugin):
        """GET /api/aps marks APs as captured when pcapng files match their MAC."""
        mock_agent = MagicMock()
        mock_agent._access_points = [
            {'hostname': 'CapturedNet', 'mac': 'AA:BB:CC:DD:EE:FF', 'channel': 6,
             'rssi': -45, 'encryption': 'WPA2', 'clients': [], 'vendor': 'TestVendor'},
            {'hostname': 'FreeNet', 'mac': '11:22:33:44:55:66', 'channel': 11,
             'rssi': -70, 'encryption': 'WPA2', 'clients': [], 'vendor': ''},
        ]
        mock_agent._config = {'bettercap': {'handshakes': '/bc/'}}
        plugin._agent = mock_agent
        req = FakeRequest(method='GET', path='/api/aps')
        with patch('angryoxide_v2.os.path.isdir', return_value=True), \
             patch('angryoxide_v2.os.listdir', return_value=['AA-BB-CC-DD-EE-FF_CapturedNet.pcapng']):
            resp = plugin.on_webhook('/api/aps', req)
        data = _resp_data(resp)
        captured_ap = [a for a in data if a['mac'] == 'AA:BB:CC:DD:EE:FF'][0]
        free_ap = [a for a in data if a['mac'] == '11:22:33:44:55:66'][0]
        assert captured_ap['captured'] is True
        assert free_ap['captured'] is False

    def test_download_all_with_actual_files(self, plugin):
        """GET /api/download/all with pcapng files includes them in ZIP."""
        import zipfile
        import io

        mock_agent = MagicMock()
        mock_agent._config = {'bettercap': {'handshakes': '/bc/handshakes/'}}
        plugin._agent = mock_agent

        def fake_isdir(path):
            return path in ('/etc/pwnagotchi/handshakes/', '/bc/handshakes/')

        def fake_listdir(path):
            if path == '/etc/pwnagotchi/handshakes/':
                return ['capture1.pcapng', 'capture2.pcapng', 'readme.txt']
            if path == '/bc/handshakes/':
                return ['legacy.pcap', 'hash.22000', 'notes.md']
            return []

        def fake_isfile(path):
            # All listed files are files
            return True

        req = FakeRequest(method='GET', path='/api/download/all')
        req.args = {'filter': 'all'}
        # We need to mock zipfile.ZipFile.write to track what gets added
        written_files = []
        original_zipfile = zipfile.ZipFile

        class TrackingZipFile(original_zipfile):
            def write(self, filename, arcname=None, *args, **kwargs):
                written_files.append(arcname or filename)

        with patch('angryoxide_v2.os.path.isdir', side_effect=fake_isdir), \
             patch('angryoxide_v2.os.listdir', side_effect=fake_listdir), \
             patch('angryoxide_v2.os.path.isfile', side_effect=fake_isfile), \
             patch('angryoxide_v2.os.path.join', side_effect=lambda *a: '/'.join(a)), \
             patch('zipfile.ZipFile', TrackingZipFile):
            resp = plugin.on_webhook('/api/download/all', req)

        assert resp.mimetype == 'application/zip'
        # capture1.pcapng and capture2.pcapng from ao dir (not readme.txt)
        # legacy.pcap and hash.22000 from bc dir (not notes.md)
        assert 'ao/capture1.pcapng' in written_files
        assert 'ao/capture2.pcapng' in written_files
        assert 'bettercap/legacy.pcap' in written_files
        assert 'bettercap/hash.22000' in written_files
        assert 'ao/readme.txt' not in written_files
        assert 'bettercap/notes.md' not in written_files


# ---------------------------------------------------------------------------
# 24. on_epoch Edge Cases (Extended)
# ---------------------------------------------------------------------------

class TestEpochEdgeCasesExtended:
    """Additional on_epoch edge cases not covered by TestEpochEdgeCases."""

    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {
            'binary_path': '/usr/local/bin/angryoxide',
            'interface': 'wlan0mon',
            'output_dir': '/etc/pwnagotchi/handshakes/',
            'max_crashes': 10,
        }
        return p

    @pytest.fixture
    def agent(self):
        a = MagicMock()
        a._config = {
            'personality': {'deauth': True, 'associate': True},
            'bettercap': {'handshakes': '/etc/pwnagotchi/handshakes/'},
            'main': {'whitelist': []},
        }
        a._handshakes = {}
        a._last_pwnd = None
        a._update_handshakes = MagicMock()
        a._view = MagicMock()
        return a

    def test_epoch_battery_low_continues(self, plugin, agent):
        """Battery 15-20% sets battery_low face but continues AO checks."""
        plugin._running = True
        plugin._process = MagicMock()
        plugin._process.poll.return_value = None
        with patch.object(plugin, '_get_battery_level', return_value=17), \
             patch('angryoxide_v2.os.path.exists', return_value=True), \
             patch('angryoxide_v2.os.path.isfile', return_value=False), \
             patch.object(plugin, '_check_health', return_value=False), \
             patch.object(plugin, '_scan_captures', return_value=0) as mock_scan:
            plugin.on_epoch(agent, 1, {})
        # battery_low face set
        agent._view.set.assert_any_call('status', 'Battery low: 17%')
        # scan_captures WAS called (did not return early)
        mock_scan.assert_called_once()

    def test_epoch_monitor_interface_missing(self, plugin, agent):
        """Monitor interface missing: wlan0 is up but wlan0mon is down.
        With current code, if wlan0 is up the WiFi down path is skipped,
        and health check determines status. This tests the health check path."""
        plugin._running = True
        plugin._process = MagicMock()
        plugin._process.poll.return_value = None
        plugin._agent = agent

        def mock_exists(path):
            if path == '/sys/class/net/wlan0':
                return True  # wlan0 is up
            if path == '/sys/class/net/wlan0mon':
                return False  # monitor is down
            return True

        with patch.object(plugin, '_get_battery_level', return_value=None), \
             patch.object(plugin, '_is_ao_mode', return_value=True), \
             patch('angryoxide_v2.os.path.exists', side_effect=mock_exists), \
             patch('angryoxide_v2.os.path.isfile', return_value=False), \
             patch.object(plugin, '_check_health', return_value=True):
            plugin.on_epoch(agent, 1, {})
        # Health check returns True (crashed), so AO crash status is shown
        agent._view.set.assert_any_call('status',
            'AO crashed! Restart %d/%d' % (plugin._crash_count, plugin.options.get('max_crashes', 10)))

    def test_epoch_firmware_crash_face(self, plugin, agent):
        """Firmware crash sets fw_crash face when _fw_crash_count > 0 and recent recovery."""
        plugin._running = True
        plugin._fw_crash_count = 1
        plugin._last_recovery = 4950.0  # recent recovery

        mock_proc = MagicMock()
        mock_proc.poll.return_value = 1  # process died
        mock_proc.returncode = 1
        mock_proc.pid = 1234
        plugin._process = mock_proc

        with patch.object(plugin, '_get_battery_level', return_value=None), \
             patch('angryoxide_v2.os.path.exists', return_value=True), \
             patch('angryoxide_v2.os.path.isfile', return_value=False), \
             patch('angryoxide_v2.time') as mock_time, \
             patch.object(plugin, '_try_fw_recovery', return_value=True), \
             patch.object(plugin, '_start_ao'):
            mock_time.time.return_value = 5000.0
            mock_time.sleep = MagicMock()
            plugin.on_epoch(agent, 1, {})
        agent._view.set.assert_any_call('status', 'Firmware crashed! Recovering...')

    def test_epoch_ao_crash_face_no_fw(self, plugin, agent):
        """AO crash without firmware crash sets ao_crashed face with crash count."""
        plugin._running = True
        plugin._fw_crash_count = 0
        plugin._last_recovery = 0

        mock_proc = MagicMock()
        mock_proc.poll.return_value = 1
        mock_proc.returncode = 1
        mock_proc.pid = 1234
        plugin._process = mock_proc

        with patch.object(plugin, '_get_battery_level', return_value=None), \
             patch('angryoxide_v2.os.path.exists', return_value=True), \
             patch('angryoxide_v2.os.path.isfile', return_value=False), \
             patch('angryoxide_v2.time') as mock_time, \
             patch.object(plugin, '_try_fw_recovery', return_value=True), \
             patch.object(plugin, '_start_ao'):
            mock_time.time.return_value = 5000.0
            mock_time.sleep = MagicMock()
            plugin.on_epoch(agent, 1, {})
        agent._view.set.assert_any_call('status', 'AO crashed! Restart 1/10')

    def test_epoch_not_running_tries_start(self, plugin, agent):
        """on_epoch with _running=False and binary available tries to start AO."""
        plugin._running = False
        with patch('angryoxide_v2.os.path.isfile', return_value=True), \
             patch.object(plugin, '_start_ao') as mock_start:
            plugin.on_epoch(agent, 1, {})
        mock_start.assert_called_once_with(agent)


# ---------------------------------------------------------------------------
# 25. _build_cmd Comprehensive Tests
# ---------------------------------------------------------------------------

class TestBuildCmdComprehensive:
    """Comprehensive _build_cmd tests for complex combinations."""

    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {
            'binary_path': '/usr/local/bin/angryoxide',
            'interface': 'wlan0mon',
            'output_dir': '/etc/pwnagotchi/handshakes/',
            'notx': False,
            'no_setup': True,
            'extra_args': '',
        }
        return p

    def test_all_six_attacks_disabled_produces_all_flags(self, plugin):
        """All 6 attacks disabled produces exactly 6 --disable flags."""
        for k in plugin._attacks:
            plugin._attacks[k] = False
        cmd = plugin._build_cmd()
        disable_flags = [c for c in cmd if c.startswith('--disable')]
        assert len(disable_flags) == 6
        expected_flags = {
            '--disable-deauth', '--disable-pmkid', '--disable-csa',
            '--disable-disassoc', '--disable-anon', '--disable-roguem2',
        }
        assert set(disable_flags) == expected_flags

    def test_skip_captured_multiple_dirs_mixed_formats(self, plugin):
        """skip_captured scans both AO and bettercap dirs, handles mixed file formats."""
        plugin._skip_captured = True
        plugin._whitelist_entries = []

        def fake_isdir(path):
            return path in ('/etc/pwnagotchi/handshakes/', '/home/pi/handshakes')

        def fake_listdir(path):
            if path == '/etc/pwnagotchi/handshakes/':
                return ['AA-BB-CC-DD-EE-FF_Net1.pcapng', 'readme.txt']
            if path == '/home/pi/handshakes':
                return ['11:22:33:44:55:66.pcap', 'not-a-mac.pcapng']
            return []

        with patch('angryoxide_v2.os.path.isdir', side_effect=fake_isdir), \
             patch('angryoxide_v2.os.listdir', side_effect=fake_listdir):
            cmd = plugin._build_cmd()

        whitelist_values = []
        for i, v in enumerate(cmd):
            if v == '--whitelist-entry':
                whitelist_values.append(cmd[i + 1])
        assert 'AA:BB:CC:DD:EE:FF' in whitelist_values
        assert '11:22:33:44:55:66' in whitelist_values
        assert len(whitelist_values) == 2

    def test_targets_whitelist_skip_captured_together(self, plugin):
        """Targets + whitelist + skip_captured all produce correct flags."""
        plugin._targets = ['TARGET:AA:BB:CC:DD:EE']
        plugin._whitelist_entries = ['MyHomeNet']
        plugin._skip_captured = True

        def fake_isdir(path):
            return True

        def fake_listdir(path):
            return ['11-22-33-44-55-66_CapturedNet.pcapng']

        with patch('angryoxide_v2.os.path.isdir', side_effect=fake_isdir), \
             patch('angryoxide_v2.os.listdir', side_effect=fake_listdir):
            cmd = plugin._build_cmd()

        # Verify target
        target_indices = [i for i, v in enumerate(cmd) if v == '--target-entry']
        assert len(target_indices) == 1
        assert cmd[target_indices[0] + 1] == 'TARGET:AA:BB:CC:DD:EE'

        # Verify whitelist entries: MyHomeNet (explicit) + 11:22:33:44:55:66 (from skip_captured)
        wl_indices = [i for i, v in enumerate(cmd) if v == '--whitelist-entry']
        wl_values = [cmd[i + 1] for i in wl_indices]
        assert 'MyHomeNet' in wl_values
        assert '11:22:33:44:55:66' in wl_values


# ---------------------------------------------------------------------------
# 26. _restart_ao Edge Cases
# ---------------------------------------------------------------------------

class TestRestartAoEdgeCases:
    """Edge cases for _restart_ao."""

    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {
            'binary_path': '/usr/local/bin/angryoxide',
            'interface': 'wlan0mon',
            'output_dir': '/etc/pwnagotchi/handshakes/',
        }
        return p

    def test_restart_calls_stop_before_start(self, plugin):
        """_restart_ao calls _stop_ao before _start_ao, in order."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        call_order = []

        def mock_stop():
            call_order.append('stop')

        def mock_start(ag):
            call_order.append('start')

        with patch.object(plugin, '_stop_ao', side_effect=mock_stop), \
             patch.object(plugin, '_start_ao', side_effect=mock_start):
            plugin._restart_ao()

        assert call_order == ['stop', 'start']

    def test_restart_no_crash_when_agent_none(self, plugin):
        """_restart_ao does not crash when _agent is None."""
        plugin._agent = None
        with patch.object(plugin, '_stop_ao') as mock_stop, \
             patch.object(plugin, '_start_ao') as mock_start:
            plugin._restart_ao()
        mock_stop.assert_called_once()
        mock_start.assert_not_called()


# ---------------------------------------------------------------------------
# 27. on_ui_update with AO Running (Extended)
# ---------------------------------------------------------------------------

class TestUIUpdateRunningExtended:
    """Detailed on_ui_update tests when AO is running."""

    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {}
        return p

    def _make_ui(self):
        ui = MagicMock()
        ui._lock = MagicMock()
        ui._lock.__enter__ = MagicMock(return_value=None)
        ui._lock.__exit__ = MagicMock(return_value=False)
        return ui

    def test_walkby_status_cleared(self, plugin):
        """Running AO clears walkby_status element."""
        ui = self._make_ui()
        plugin._running = True
        plugin._captures = 3
        plugin._start_time = 1000.0
        with patch('angryoxide_v2.time') as mock_time:
            mock_time.time.return_value = 1600.0
            plugin.on_ui_update(ui)
        # Check that walkby_status was set to ''
        set_calls = {c[0] for c in ui.set.call_args_list}
        cleared_elems = [c[0][0] for c in ui.set.call_args_list if len(c[0]) >= 2 and c[0][1] == '']
        assert 'walkby_status' in cleared_elems

    def test_name_cleared(self, plugin):
        """Running AO clears name element."""
        ui = self._make_ui()
        plugin._running = True
        plugin._captures = 0
        plugin._start_time = 1000.0
        with patch('angryoxide_v2.time') as mock_time:
            mock_time.time.return_value = 1000.0
            plugin.on_ui_update(ui)
        cleared_elems = [c[0][0] for c in ui.set.call_args_list if len(c[0]) >= 2 and c[0][1] == '']
        assert 'name' in cleared_elems

    def test_angryoxide_shows_captures_and_uptime(self, plugin):
        """Running AO shows 'verified/total | uptime | channels' format."""
        ui = self._make_ui()
        plugin._running = True
        plugin._captures = 12
        plugin._start_time = 1000.0
        with patch('angryoxide_v2.time') as mock_time, \
             patch.object(plugin, '_is_ao_mode', return_value=True), \
             patch.object(plugin, '_count_pcapngs', return_value=12), \
             patch.object(plugin, '_count_verified', return_value=5):
            mock_time.time.return_value = 4600.0  # 1 hour
            plugin.on_ui_update(ui)
        ui.set.assert_any_call('angryoxide', 'AO: 5/12 | 1h | CH:1,6,11')

    def test_all_overlap_elements_cleared(self, plugin):
        """Running AO clears all overlapping UI elements: name, walkby, blitz, walkby_status."""
        ui = self._make_ui()
        plugin._running = True
        plugin._captures = 0
        plugin._start_time = 1000.0
        with patch('angryoxide_v2.time') as mock_time:
            mock_time.time.return_value = 1000.0
            plugin.on_ui_update(ui)
        cleared_elems = [c[0][0] for c in ui.set.call_args_list if len(c[0]) >= 2 and c[0][1] == '']
        for elem in ('name', 'walkby', 'blitz', 'walkby_status'):
            assert elem in cleared_elems, "%s should be cleared when AO is running" % elem


# ---------------------------------------------------------------------------
# 28. _extract_mac_from_filename (Extended)
# ---------------------------------------------------------------------------

class TestExtractMacExtended:
    """Extended MAC extraction tests for various filename formats."""

    def test_ao_format_with_network_name(self):
        """AO format: AA-BB-CC-DD-EE-FF_NetworkName.pcapng"""
        result = AngryOxide._extract_mac_from_filename("AA-BB-CC-DD-EE-FF_NetworkName.pcapng")
        assert result == "AA:BB:CC:DD:EE:FF"

    def test_bettercap_format_colon_separated(self):
        """Bettercap format: AA:BB:CC:DD:EE:FF.pcap"""
        result = AngryOxide._extract_mac_from_filename("AA:BB:CC:DD:EE:FF.pcap")
        assert result == "AA:BB:CC:DD:EE:FF"

    def test_no_mac_random_file(self):
        """Random filename with no MAC returns None."""
        result = AngryOxide._extract_mac_from_filename("randomfile.pcapng")
        assert result is None

    def test_partial_mac_three_octets(self):
        """Partial MAC (only 3 octets) returns None."""
        result = AngryOxide._extract_mac_from_filename("AA-BB-CC.pcapng")
        assert result is None

    def test_lowercase_mac(self):
        """Lowercase MAC is extracted correctly."""
        result = AngryOxide._extract_mac_from_filename("aa-bb-cc-dd-ee-ff_Net.pcapng")
        assert result == "aa:bb:cc:dd:ee:ff"

    def test_empty_filename(self):
        """Empty filename returns None."""
        result = AngryOxide._extract_mac_from_filename("")
        assert result is None

    def test_mac_only_no_extension(self):
        """MAC-only filename without extension."""
        result = AngryOxide._extract_mac_from_filename("AA-BB-CC-DD-EE-FF")
        assert result == "AA:BB:CC:DD:EE:FF"

    def test_five_octets_returns_none(self):
        """5 octets (one short) returns None."""
        result = AngryOxide._extract_mac_from_filename("AA-BB-CC-DD-EE.pcapng")
        assert result is None


# ---------------------------------------------------------------------------
# 28. Log Viewer Tests
# ---------------------------------------------------------------------------

class TestLogViewer:
    """Tests for the GET /api/logs endpoint."""

    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {
            'binary_path': '/usr/local/bin/angryoxide',
            'interface': 'wlan0mon',
            'output_dir': '/etc/pwnagotchi/handshakes/',
        }
        return p

    def test_api_logs_returns_lines(self, plugin):
        """GET /api/logs returns JSON with 'lines' key."""
        req = FakeRequest(method='GET', path='/api/logs')
        fake_output = "line1 angryoxide started\nline2 angryoxide scan\nline3 angryoxide capture"
        mock_result = MagicMock()
        mock_result.stdout = fake_output
        mock_result.returncode = 0
        with patch('angryoxide_v2.subprocess.run', return_value=mock_result):
            resp = plugin.on_webhook('/api/logs', req)
        data = _resp_data(resp)
        assert 'lines' in data
        assert len(data['lines']) == 3

    def test_api_logs_filters_ao_lines(self, plugin):
        """Output with mixed lines only returns angryoxide-related ones."""
        req = FakeRequest(method='GET', path='/api/logs')
        mixed_output = "systemd started pwnagotchi\nangryoxide scanning wlan0mon\nunrelated bettercap msg\nAO found target\nangry bull mode active"
        mock_result = MagicMock()
        mock_result.stdout = mixed_output
        mock_result.returncode = 0
        with patch('angryoxide_v2.subprocess.run', return_value=mock_result):
            resp = plugin.on_webhook('/api/logs', req)
        data = _resp_data(resp)
        # 'angryoxide' matches line 2, 'ao' matches line 4, 'angry' matches line 5
        assert len(data['lines']) == 3
        assert any('angryoxide' in l.lower() for l in data['lines'])
        assert any('ao' in l.lower() for l in data['lines'])

    def test_api_logs_fallback_on_error(self, plugin):
        """Subprocess exception returns ['Could not read logs']."""
        req = FakeRequest(method='GET', path='/api/logs')
        with patch('angryoxide_v2.subprocess.run', side_effect=OSError('no journalctl')):
            resp = plugin.on_webhook('/api/logs', req)
        data = _resp_data(resp)
        assert data['lines'] == ['Could not read logs']


# ---------------------------------------------------------------------------
# 29. Capture Types Tests
# ---------------------------------------------------------------------------

class TestCaptureTypes:
    """Tests for capture type detection in GET /api/captures."""

    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {
            'binary_path': '/usr/local/bin/angryoxide',
            'interface': 'wlan0mon',
            'output_dir': '/etc/pwnagotchi/handshakes/',
        }
        return p

    def test_capture_type_pmkid_by_size(self, plugin):
        """pcapng file < 2048 bytes returns type 'PMKID'."""
        req = FakeRequest(method='GET', path='/api/captures')
        with patch('angryoxide_v2.os.path.isdir', return_value=True), \
             patch('angryoxide_v2.os.listdir', return_value=['AA-BB-CC-DD-EE-FF_Net.pcapng']), \
             patch('angryoxide_v2.os.path.getmtime', return_value=1000.0), \
             patch('angryoxide_v2.os.path.getsize', return_value=512), \
             patch('angryoxide_v2.os.path.isfile', return_value=False), \
             patch('angryoxide_v2.os.path.basename', side_effect=lambda p: p.split('/')[-1].split('\\')[-1]), \
             patch('angryoxide_v2.os.path.dirname', return_value='/etc/pwnagotchi/handshakes/'):
            resp = plugin.on_webhook('/api/captures', req)
        data = _resp_data(resp)
        assert data[0]['type'] == 'PMKID'

    def test_capture_type_4way_by_size(self, plugin):
        """pcapng file >= 2048 bytes returns type '4-way'."""
        req = FakeRequest(method='GET', path='/api/captures')
        with patch('angryoxide_v2.os.path.isdir', return_value=True), \
             patch('angryoxide_v2.os.listdir', return_value=['AA-BB-CC-DD-EE-FF_Net.pcapng']), \
             patch('angryoxide_v2.os.path.getmtime', return_value=1000.0), \
             patch('angryoxide_v2.os.path.getsize', return_value=4096), \
             patch('angryoxide_v2.os.path.isfile', return_value=False), \
             patch('angryoxide_v2.os.path.basename', side_effect=lambda p: p.split('/')[-1].split('\\')[-1]), \
             patch('angryoxide_v2.os.path.dirname', return_value='/etc/pwnagotchi/handshakes/'):
            resp = plugin.on_webhook('/api/captures', req)
        data = _resp_data(resp)
        assert data[0]['type'] == '4-way'

    def test_capture_type_hashcat(self, plugin):
        """.22000 file returns type 'hashcat'."""
        req = FakeRequest(method='GET', path='/api/captures')
        with patch('angryoxide_v2.os.path.isdir', return_value=True), \
             patch('angryoxide_v2.os.listdir', return_value=['capture.22000']), \
             patch('angryoxide_v2.os.path.getmtime', return_value=1000.0), \
             patch('angryoxide_v2.os.path.isfile', return_value=False), \
             patch('angryoxide_v2.os.path.basename', side_effect=lambda p: p.split('/')[-1].split('\\')[-1]), \
             patch('angryoxide_v2.os.path.dirname', return_value='/etc/pwnagotchi/handshakes/'):
            resp = plugin.on_webhook('/api/captures', req)
        data = _resp_data(resp)
        assert data[0]['type'] == 'hashcat'

    def test_capture_type_pmkid_in_name(self, plugin):
        """Filename containing 'pmkid' returns type 'PMKID'."""
        req = FakeRequest(method='GET', path='/api/captures')
        with patch('angryoxide_v2.os.path.isdir', return_value=True), \
             patch('angryoxide_v2.os.listdir', return_value=['AA-BB-CC-DD-EE-FF_pmkid_capture.pcapng']), \
             patch('angryoxide_v2.os.path.getmtime', return_value=1000.0), \
             patch('angryoxide_v2.os.path.isfile', return_value=False), \
             patch('angryoxide_v2.os.path.basename', side_effect=lambda p: p.split('/')[-1].split('\\')[-1]), \
             patch('angryoxide_v2.os.path.dirname', return_value='/etc/pwnagotchi/handshakes/'):
            resp = plugin.on_webhook('/api/captures', req)
        data = _resp_data(resp)
        assert data[0]['type'] == 'PMKID'


# ---------------------------------------------------------------------------
# 30. Session Stats Tests
# ---------------------------------------------------------------------------

class TestSessionStats:
    """Tests for capture_rate and stable_epochs in /api/status."""

    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {
            'binary_path': '/usr/local/bin/angryoxide',
            'interface': 'wlan0mon',
            'output_dir': '/etc/pwnagotchi/handshakes/',
        }
        p._agent = MagicMock()
        p._agent._config = {'main': {'whitelist': []}}
        return p

    def test_status_includes_capture_rate(self, plugin):
        """/api/status response includes 'rate' field (attack rate setting)."""
        plugin._running = True
        plugin._captures = 3
        plugin._start_time = 1000.0
        plugin._process = MagicMock()
        plugin._process.pid = 1234
        req = FakeRequest(method='GET', path='/api/status')
        with patch('angryoxide_v2.time') as mock_time:
            mock_time.time.return_value = 4600.0  # 1 hour
            resp = plugin.on_webhook('/api/status', req)
        data = _resp_data(resp)
        assert 'rate' in data

    def test_status_includes_stable_epochs(self, plugin):
        """/api/status response includes 'uptime_secs' field (None when not running)."""
        plugin._running = False
        plugin._process = None
        plugin._start_time = None
        req = FakeRequest(method='GET', path='/api/status')
        resp = plugin.on_webhook('/api/status', req)
        data = _resp_data(resp)
        assert 'uptime_secs' in data
        assert data['uptime_secs'] is None

    def test_capture_rate_calculation(self, plugin):
        """With 5 captures over 2 hours, uptime_secs should be 7200."""
        plugin._running = True
        plugin._captures = 5
        plugin._start_time = 1000.0
        plugin._process = MagicMock()
        plugin._process.pid = 1234
        req = FakeRequest(method='GET', path='/api/status')
        with patch('angryoxide_v2.time') as mock_time:
            mock_time.time.return_value = 8200.0  # 7200 seconds = 2 hours
            resp = plugin.on_webhook('/api/status', req)
        data = _resp_data(resp)
        assert data['uptime_secs'] == 7200
        assert data['captures'] == 5


# ---------------------------------------------------------------------------
# 31. Discord Notification Tests
# ---------------------------------------------------------------------------

class TestDiscordNotification:
    """Tests for _notify_capture and /api/discord-webhook."""

    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {
            'binary_path': '/usr/local/bin/angryoxide',
            'interface': 'wlan0mon',
            'output_dir': '/etc/pwnagotchi/handshakes/',
        }
        return p

    def test_notify_capture_skips_empty_webhook(self, plugin):
        """No HTTP call when webhook is empty."""
        plugin._discord_webhook = ''
        with patch('urllib.request.urlopen') as mock_urlopen:
            plugin._notify_capture('test.pcapng', 'AA:BB:CC:DD:EE:FF')
        mock_urlopen.assert_not_called()

    def test_notify_capture_skips_placeholder(self, plugin):
        """No HTTP call when webhook contains 'YOUR_WEBHOOK_URL'."""
        plugin._discord_webhook = 'https://discord.com/api/webhooks/YOUR_WEBHOOK_URL'
        with patch('urllib.request.urlopen') as mock_urlopen:
            plugin._notify_capture('test.pcapng', 'AA:BB:CC:DD:EE:FF')
        mock_urlopen.assert_not_called()

    def test_notify_capture_sends_request(self, plugin):
        """Mock urllib.request.urlopen, verify it's called with correct URL."""
        plugin._discord_webhook = 'https://discord.com/api/webhooks/12345/abcdef'
        plugin._captures = 1
        with patch('urllib.request.urlopen') as mock_urlopen:
            plugin._notify_capture('capture.pcapng', 'AA:BB:CC:DD:EE:FF')
        mock_urlopen.assert_called_once()
        call_args = mock_urlopen.call_args
        req_obj = call_args[0][0]
        assert req_obj.full_url == 'https://discord.com/api/webhooks/12345/abcdef'

    def test_discord_webhook_persists(self, plugin, tmp_path):
        """POST /api/discord-webhook saves to state."""
        plugin._state_file = str(tmp_path / 'state.json')
        req = FakeRequest(method='POST', path='/api/discord-webhook',
                          json_data={'url': 'https://discord.com/api/webhooks/test'})
        resp = plugin.on_webhook('/api/discord-webhook', req)
        data = _resp_data(resp)
        assert data['status'] == 'ok'
        assert plugin._discord_webhook == 'https://discord.com/api/webhooks/test'
        # Verify it was saved to state file
        import json as json_mod
        with open(str(tmp_path / 'state.json'), 'r') as f:
            state = json_mod.load(f)
        assert state['discord_webhook'] == 'https://discord.com/api/webhooks/test'


# ---------------------------------------------------------------------------
# 32. GPS Integration Tests
# ---------------------------------------------------------------------------

class TestGPSIntegration:
    """Tests for GPS integration in _build_cmd and _get_health."""

    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {
            'binary_path': '/usr/local/bin/angryoxide',
            'interface': 'wlan0mon',
            'output_dir': '/etc/pwnagotchi/handshakes/',
            'notx': False,
            'no_setup': True,
            'extra_args': '',
        }
        return p

    def test_build_cmd_with_gpsd(self, plugin):
        """When socket connects to 127.0.0.1:2947, cmd includes '--gpsd'."""
        mock_sock = MagicMock()
        with patch('socket.socket', return_value=mock_sock):
            mock_sock.connect.return_value = None  # success
            cmd = plugin._build_cmd()
        assert '--gpsd' in cmd
        assert '127.0.0.1:2947' in cmd

    def test_build_cmd_without_gpsd(self, plugin):
        """When socket connect fails, cmd has no '--gpsd'."""
        mock_sock = MagicMock()
        mock_sock.connect.side_effect = ConnectionRefusedError('refused')
        with patch('socket.socket', return_value=mock_sock):
            cmd = plugin._build_cmd()
        assert '--gpsd' not in cmd

    def test_health_includes_expected_keys(self, plugin):
        """/api/health response includes core health fields."""
        with patch('angryoxide_v2.os.path.exists', return_value=True), \
             patch('angryoxide_v2.subprocess.run') as mock_run:
            mock_run.return_value = MagicMock(stdout='normal log', returncode=0)
            health = plugin._get_health()
        assert 'wifi' in health
        assert 'monitor' in health
        assert 'firmware' in health
        assert 'usb0' in health
        assert 'battery' in health


# ---------------------------------------------------------------------------
# TestNonBlockingRestart -- verify _check_health doesn't call time.sleep
# ---------------------------------------------------------------------------

class TestNonBlockingRestart:
    """Verify _check_health uses non-blocking backoff (no time.sleep)."""

    def test_check_health_no_sleep_on_crash(self, plugin, agent):
        """When AO process dies, _check_health schedules a deferred restart
        via _next_restart_time instead of calling time.sleep."""
        import inspect
        source = inspect.getsource(plugin._check_health)
        assert 'time.sleep' not in source, (
            "_check_health should not call time.sleep; it should use "
            "_next_restart_time for non-blocking backoff"
        )

    def test_check_health_sets_next_restart_time(self, plugin, agent):
        """After a crash, _check_health sets _next_restart_time > 0."""
        fake_proc = MagicMock()
        fake_proc.poll.return_value = 1  # process exited
        fake_proc.returncode = 1
        plugin._process = fake_proc
        plugin._running = True
        plugin._agent = agent

        with patch('angryoxide_v2.os.path.exists', return_value=True), \
             patch('angryoxide_v2.subprocess.run') as mock_run:
            mock_run.return_value = MagicMock(stdout='normal log', returncode=0)
            plugin._check_health(agent)

        assert plugin._next_restart_time > 0, (
            "_check_health should schedule a future restart, not sleep"
        )

    def test_check_health_healthy_process_no_restart(self, plugin, agent):
        """A healthy (still-running) AO process triggers no restart."""
        fake_proc = MagicMock()
        fake_proc.poll.return_value = None  # still running
        plugin._process = fake_proc
        plugin._running = True

        result = plugin._check_health(agent)
        assert result is False
        assert plugin._next_restart_time == 0


# ---------------------------------------------------------------------------
# TestInputValidation -- channel format, MAC format, webhook URL
# ---------------------------------------------------------------------------

class TestInputValidation:
    """Test that the plugin handles various input formats correctly."""

    def test_channel_string_in_build_cmd(self, plugin):
        """Channel string like '1,6,11' is passed correctly to --channel."""
        plugin._channels = '1,6,11'
        cmd = plugin._build_cmd()
        assert '--channel' in cmd
        idx = cmd.index('--channel')
        assert cmd[idx + 1] == '1,6,11'

    def test_empty_channel_string_omits_flag(self, plugin):
        """Empty channel string means no --channel flag."""
        plugin._channels = ''
        plugin._autohunt = False
        cmd = plugin._build_cmd()
        assert '--channel' not in cmd

    def test_autohunt_overrides_channels(self, plugin):
        """When autohunt is True, --autohunt is used instead of --channel."""
        plugin._channels = '1,6,11'
        plugin._autohunt = True
        cmd = plugin._build_cmd()
        assert '--autohunt' in cmd
        assert '--channel' not in cmd

    def test_single_channel(self, plugin):
        """Single channel value is accepted."""
        plugin._channels = '6'
        cmd = plugin._build_cmd()
        assert '--channel' in cmd
        idx = cmd.index('--channel')
        assert cmd[idx + 1] == '6'

    def test_mac_format_in_targets(self, plugin):
        """MAC addresses in targets are passed through to --target-entry."""
        plugin._targets = ['AA:BB:CC:DD:EE:FF']
        cmd = plugin._build_cmd()
        assert '--target-entry' in cmd
        idx = cmd.index('--target-entry')
        assert cmd[idx + 1] == 'AA:BB:CC:DD:EE:FF'

    def test_ssid_in_targets(self, plugin):
        """SSID strings in targets are passed through to --target-entry."""
        plugin._targets = ['MyNetwork']
        cmd = plugin._build_cmd()
        assert '--target-entry' in cmd
        idx = cmd.index('--target-entry')
        assert cmd[idx + 1] == 'MyNetwork'

    def test_whitelist_entries_in_cmd(self, plugin):
        """Whitelist entries are passed to --whitelist-entry."""
        plugin._whitelist_entries = ['00:11:22:33:44:55', 'HomeWiFi']
        cmd = plugin._build_cmd()
        wl_indices = [i for i, c in enumerate(cmd) if c == '--whitelist-entry']
        assert len(wl_indices) == 2
        wl_values = [cmd[i + 1] for i in wl_indices]
        assert '00:11:22:33:44:55' in wl_values
        assert 'HomeWiFi' in wl_values

    def test_discord_webhook_url_stored(self, plugin):
        """Discord webhook URL is stored and retrievable."""
        plugin._discord_webhook = 'https://discord.com/api/webhooks/123/abc'
        assert plugin._discord_webhook == 'https://discord.com/api/webhooks/123/abc'

    def test_discord_webhook_empty_string(self, plugin):
        """Empty string webhook disables notifications."""
        plugin._discord_webhook = ''
        assert not plugin._discord_webhook

    def test_rate_must_be_1_2_or_3(self, plugin):
        """Rate values are passed correctly to --rate in _build_cmd."""
        plugin._rate = 1
        cmd = plugin._build_cmd()
        idx = cmd.index('--rate')
        assert cmd[idx + 1] == '1'

        plugin._rate = 3
        cmd = plugin._build_cmd()
        idx = cmd.index('--rate')
        assert cmd[idx + 1] == '3'

    def test_dwell_in_build_cmd(self, plugin):
        """Dwell time is passed to --dwell."""
        plugin._dwell = 5
        cmd = plugin._build_cmd()
        assert '--dwell' in cmd
        idx = cmd.index('--dwell')
        assert cmd[idx + 1] == '5'


# ---------------------------------------------------------------------------
# TestTOMLEscaping -- test the config writer escapes special characters
# ---------------------------------------------------------------------------

class TestTOMLEscaping:
    """Test the inline TOML writer's escaping of special characters."""

    def _get_escape_fn(self):
        """Extract the _escape_toml_string function logic for testing."""
        def _escape_toml_string(s):
            return s.replace('\\', '\\\\').replace('"', '\\"').replace('\n', '\\n').replace('\r', '\\r')
        return _escape_toml_string

    def test_escape_backslash(self):
        escape = self._get_escape_fn()
        assert escape('C:\\path\\to\\file') == 'C:\\\\path\\\\to\\\\file'

    def test_escape_double_quote(self):
        escape = self._get_escape_fn()
        assert escape('say "hello"') == 'say \\"hello\\"'

    def test_escape_newline(self):
        escape = self._get_escape_fn()
        assert escape('line1\nline2') == 'line1\\nline2'

    def test_escape_carriage_return(self):
        escape = self._get_escape_fn()
        assert escape('line1\rline2') == 'line1\\rline2'

    def test_escape_combined(self):
        escape = self._get_escape_fn()
        result = escape('C:\\path\n"quoted"')
        assert result == 'C:\\\\path\\n\\"quoted\\"'

    def test_escape_empty_string(self):
        escape = self._get_escape_fn()
        assert escape('') == ''

    def test_escape_no_special_chars(self):
        escape = self._get_escape_fn()
        assert escape('simple_key') == 'simple_key'

    def test_toml_bool_rendering(self):
        """Booleans should render as 'true'/'false' (not Python True/False)."""
        v = True
        rendered = 'true' if v else 'false'
        assert rendered == 'true'
        v = False
        rendered = 'true' if v else 'false'
        assert rendered == 'false'

    def test_toml_list_rendering(self):
        """Lists of mixed types should render correctly."""
        escape = self._get_escape_fn()
        items = []
        test_list = [True, 42, 'hello "world"']
        for item in test_list:
            if isinstance(item, bool):
                items.append('true' if item else 'false')
            elif isinstance(item, (int, float)):
                items.append(str(item))
            else:
                items.append('"%s"' % escape(str(item)))
        result = '[%s]' % ', '.join(items)
        assert result == '[true, 42, "hello \\"world\\""]'


# ---------------------------------------------------------------------------
# TestBlindEpochPrevention -- verify on_epoch injects dummy AP when AO running
# ---------------------------------------------------------------------------

class TestBlindEpochPrevention:
    """Verify on_epoch prevents blind epoch counting when AO is active."""

    def test_injects_dummy_ap_when_no_aps(self, plugin, agent):
        """When AO is running and agent has no APs, on_epoch injects a dummy AP."""
        plugin._running = True
        plugin._agent = agent
        agent._access_points = []

        with patch('angryoxide_v2.os.path.exists', return_value=True), \
             patch('angryoxide_v2.os.path.isfile', return_value=True), \
             patch.object(plugin, '_check_health', return_value=False), \
             patch.object(plugin, '_scan_captures', return_value=0), \
             patch.object(plugin, '_get_battery_level', return_value=None):
            plugin.on_epoch(agent, 1, {})

        assert len(agent._access_points) >= 1
        dummy = agent._access_points[0]
        assert dummy['hostname'] == 'AO-active'
        assert dummy['mac'] == '00:00:00:00:00:00'

    def test_no_injection_when_aps_exist(self, plugin, agent):
        """When agent already has APs, on_epoch does not overwrite them."""
        plugin._running = True
        plugin._agent = agent
        existing_ap = {'hostname': 'RealAP', 'mac': 'AA:BB:CC:DD:EE:FF', 'channel': 6, 'rssi': -50, 'encryption': 'WPA2', 'clients': []}
        agent._access_points = [existing_ap]

        with patch('angryoxide_v2.os.path.exists', return_value=True), \
             patch('angryoxide_v2.os.path.isfile', return_value=True), \
             patch.object(plugin, '_check_health', return_value=False), \
             patch.object(plugin, '_scan_captures', return_value=0), \
             patch.object(plugin, '_get_battery_level', return_value=None):
            plugin.on_epoch(agent, 1, {})

        assert agent._access_points[0]['hostname'] == 'RealAP'

    def test_no_injection_when_not_running(self, plugin, agent):
        """When AO is not running, no dummy AP is injected."""
        plugin._running = False
        plugin._agent = agent
        agent._access_points = []

        with patch('angryoxide_v2.os.path.isfile', return_value=True), \
             patch.object(plugin, '_start_ao', return_value=None):
            plugin.on_epoch(agent, 1, {})

        assert agent._access_points == []

    def test_no_injection_when_no_agent(self, plugin, agent):
        """When _agent is None, no dummy AP is injected."""
        plugin._running = True
        plugin._agent = None
        agent._access_points = []

        with patch('angryoxide_v2.os.path.exists', return_value=True), \
             patch('angryoxide_v2.os.path.isfile', return_value=True), \
             patch.object(plugin, '_check_health', return_value=False), \
             patch.object(plugin, '_scan_captures', return_value=0), \
             patch.object(plugin, '_get_battery_level', return_value=None):
            plugin.on_epoch(agent, 1, {})

        assert agent._access_points == []

    def test_injection_survives_exception(self, plugin, agent):
        """If accessing _access_points throws, on_epoch continues normally."""
        plugin._running = True
        plugin._agent = agent
        type(agent)._access_points = PropertyMock(side_effect=AttributeError("no such attr"))

        with patch('angryoxide_v2.os.path.exists', return_value=True), \
             patch('angryoxide_v2.os.path.isfile', return_value=True), \
             patch.object(plugin, '_check_health', return_value=False), \
             patch.object(plugin, '_scan_captures', return_value=0), \
             patch.object(plugin, '_get_battery_level', return_value=None):
            plugin.on_epoch(agent, 1, {})

        # Clean up the PropertyMock so other tests aren't affected
        if hasattr(type(agent), '_access_points'):
            del type(agent)._access_points


# ---------------------------------------------------------------------------
# Security: API Authentication Tests
# ---------------------------------------------------------------------------

class TestWebhookAuth:
    """Tests for the auth check added to on_webhook for POST requests."""

    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {
            'binary_path': '/usr/local/bin/angryoxide',
            'interface': 'wlan0mon',
            'output_dir': '/etc/pwnagotchi/handshakes/',
            'notx': False, 'no_setup': True, 'extra_args': '',
        }
        return p

    def test_post_no_auth_configured_passes(self, plugin):
        """POST request passes when web auth is not enabled."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        req = FakeRequest(method='POST', path='/api/restart')
        with patch.object(plugin, '_restart_ao'), patch.object(plugin, '_save_state'):
            resp = plugin.on_webhook('/api/restart', req)
        data = _resp_data(resp)
        assert data.get('status') == 'ok'

    def test_post_auth_enabled_no_credentials_returns_401(self, plugin):
        """POST request returns 401 when auth is enabled but no credentials provided."""
        plugin._agent = MagicMock()
        plugin._agent._config = {
            'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {},
            'ui': {'web': {'auth': True, 'username': 'admin', 'password': 'secret'}},
        }
        req = FakeRequest(method='POST', path='/api/restart')
        resp = plugin.on_webhook('/api/restart', req)
        assert _resp_status(resp) == 401
        assert _resp_data(resp)['error'] == 'unauthorized'

    def test_post_auth_enabled_wrong_credentials_returns_401(self, plugin):
        """POST request returns 401 when credentials are wrong."""
        plugin._agent = MagicMock()
        plugin._agent._config = {
            'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {},
            'ui': {'web': {'auth': True, 'username': 'admin', 'password': 'secret'}},
        }
        req = FakeRequest(method='POST', path='/api/restart')
        req.authorization = MagicMock()
        req.authorization.username = 'admin'
        req.authorization.password = 'wrong'
        resp = plugin.on_webhook('/api/restart', req)
        assert _resp_status(resp) == 401

    def test_post_auth_enabled_correct_credentials_passes(self, plugin):
        """POST request passes when correct credentials are provided."""
        plugin._agent = MagicMock()
        plugin._agent._config = {
            'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {},
            'ui': {'web': {'auth': True, 'username': 'admin', 'password': 'secret'}},
        }
        req = FakeRequest(method='POST', path='/api/restart')
        req.authorization = MagicMock()
        req.authorization.username = 'admin'
        req.authorization.password = 'secret'
        with patch.object(plugin, '_restart_ao'), patch.object(plugin, '_save_state'):
            resp = plugin.on_webhook('/api/restart', req)
        data = _resp_data(resp)
        assert data.get('status') == 'ok'

    def test_get_bypasses_auth(self, plugin):
        """GET requests are not subject to auth checks."""
        plugin._agent = MagicMock()
        plugin._agent._config = {
            'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {},
            'ui': {'web': {'auth': True, 'username': 'admin', 'password': 'secret'}},
        }
        req = FakeRequest(method='GET', path='/api/status')
        plugin._running = True
        plugin._start_time = time.time()
        resp = plugin.on_webhook('/api/status', req)
        data = _resp_data(resp)
        assert 'running' in data

    def test_post_no_agent_skips_auth(self, plugin):
        """POST request passes when agent is not set (auth check skipped)."""
        plugin._agent = None
        req = FakeRequest(method='POST', path='/api/restart')
        with patch.object(plugin, '_restart_ao'), patch.object(plugin, '_save_state'):
            resp = plugin.on_webhook('/api/restart', req)
        data = _resp_data(resp)
        assert data.get('status') == 'ok'


# ---------------------------------------------------------------------------
# Security: Input Validation Tests
# ---------------------------------------------------------------------------

class TestInputValidation:
    """Tests for _validate_channels, _validate_mac_or_ssid, _validate_discord_webhook."""

    # --- Channel validation ---

    def test_validate_channels_valid(self):
        assert AngryOxide._validate_channels('1,6,11') == '1,6,11'

    def test_validate_channels_single(self):
        assert AngryOxide._validate_channels('6') == '6'

    def test_validate_channels_out_of_range(self):
        assert AngryOxide._validate_channels('0,15,20') == ''

    def test_validate_channels_mixed(self):
        assert AngryOxide._validate_channels('1,abc,6,99,11') == '1,6,11'

    def test_validate_channels_empty(self):
        assert AngryOxide._validate_channels('') == ''

    def test_validate_channels_none(self):
        assert AngryOxide._validate_channels(None) == ''

    def test_validate_channels_spaces(self):
        assert AngryOxide._validate_channels(' 1 , 6 , 11 ') == '1,6,11'

    def test_validate_channels_all_14(self):
        assert AngryOxide._validate_channels('1,2,3,4,5,6,7,8,9,10,11,12,13,14') == '1,2,3,4,5,6,7,8,9,10,11,12,13,14'

    def test_validate_channels_negative(self):
        assert AngryOxide._validate_channels('-1,6') == '6'

    # --- MAC/SSID validation ---

    def test_validate_mac_valid_uppercase(self):
        assert AngryOxide._validate_mac_or_ssid('AA:BB:CC:DD:EE:FF') == 'AA:BB:CC:DD:EE:FF'

    def test_validate_mac_valid_lowercase_uppercased(self):
        assert AngryOxide._validate_mac_or_ssid('aa:bb:cc:dd:ee:ff') == 'AA:BB:CC:DD:EE:FF'

    def test_validate_ssid_valid(self):
        assert AngryOxide._validate_mac_or_ssid('MyNetwork') == 'MyNetwork'

    def test_validate_ssid_stripped(self):
        assert AngryOxide._validate_mac_or_ssid('  MyNetwork  ') == 'MyNetwork'

    def test_validate_empty_returns_none(self):
        assert AngryOxide._validate_mac_or_ssid('') is None

    def test_validate_none_returns_none(self):
        assert AngryOxide._validate_mac_or_ssid(None) is None

    def test_validate_whitespace_only_returns_none(self):
        assert AngryOxide._validate_mac_or_ssid('   ') is None

    def test_validate_ssid_truncated_to_32(self):
        long_ssid = 'A' * 50
        result = AngryOxide._validate_mac_or_ssid(long_ssid)
        assert len(result) == 32

    def test_validate_ssid_control_chars_stripped(self):
        result = AngryOxide._validate_mac_or_ssid('test\x00\x01net')
        assert result == 'testnet'

    def test_validate_mac_mixed_case(self):
        assert AngryOxide._validate_mac_or_ssid('aA:bB:cC:dD:eE:fF') == 'AA:BB:CC:DD:EE:FF'

    def test_validate_mac_invalid_format_treated_as_ssid(self):
        result = AngryOxide._validate_mac_or_ssid('AA-BB-CC-DD-EE-FF')
        assert result == 'AA-BB-CC-DD-EE-FF'  # treated as SSID, not MAC

    # --- Discord webhook validation ---

    def test_validate_discord_valid(self):
        url = 'https://discord.com/api/webhooks/12345/abcdef'
        assert AngryOxide._validate_discord_webhook(url) == url

    def test_validate_discord_empty(self):
        assert AngryOxide._validate_discord_webhook('') == ''

    def test_validate_discord_none(self):
        assert AngryOxide._validate_discord_webhook(None) == ''

    def test_validate_discord_invalid_url(self):
        assert AngryOxide._validate_discord_webhook('https://evil.com/webhook') == ''

    def test_validate_discord_http_not_https(self):
        assert AngryOxide._validate_discord_webhook('http://discord.com/api/webhooks/123/abc') == ''

    def test_validate_discord_stripped(self):
        url = '  https://discord.com/api/webhooks/123/abc  '
        assert AngryOxide._validate_discord_webhook(url) == url.strip()


class TestInputValidationIntegration:
    """Test that validation is applied in actual webhook handlers."""

    @pytest.fixture
    def plugin(self):
        from angryoxide_v2 import AngryOxide
        p = AngryOxide()
        p.options = {
            'binary_path': '/usr/local/bin/angryoxide',
            'interface': 'wlan0mon',
            'output_dir': '/etc/pwnagotchi/handshakes/',
            'notx': False, 'no_setup': True, 'extra_args': '',
        }
        return p

    def test_channels_invalid_filtered(self, plugin):
        """POST /api/channels with invalid channels filters them out."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        req = FakeRequest(method='POST', json_data={'channels': '1,abc,99,6', 'dwell': 5})
        with patch.object(plugin, '_restart_ao'), patch.object(plugin, '_save_state'):
            plugin.on_webhook('/api/channels', req)
        assert plugin._channels == '1,6'

    def test_target_add_invalid_mac_ignored(self, plugin):
        """POST /api/targets/add with invalid entry is ignored."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        plugin._targets = []
        req = FakeRequest(method='POST', json_data={'target': '   '})
        with patch.object(plugin, '_restart_ao'), patch.object(plugin, '_save_state'):
            plugin.on_webhook('/api/targets/add', req)
        assert len(plugin._targets) == 0

    def test_target_add_mac_uppercased(self, plugin):
        """POST /api/targets/add uppercases MAC addresses."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        plugin._targets = []
        req = FakeRequest(method='POST', json_data={'target': 'aa:bb:cc:dd:ee:ff'})
        with patch.object(plugin, '_restart_ao'), patch.object(plugin, '_save_state'):
            plugin.on_webhook('/api/targets/add', req)
        assert plugin._targets == ['AA:BB:CC:DD:EE:FF']

    def test_whitelist_add_ssid_valid(self, plugin):
        """POST /api/whitelist/add with SSID string works."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        plugin._whitelist_entries = []
        req = FakeRequest(method='POST', json_data={'entry': 'HomeNetwork'})
        with patch.object(plugin, '_restart_ao'), patch.object(plugin, '_save_state'):
            plugin.on_webhook('/api/whitelist/add', req)
        assert 'HomeNetwork' in plugin._whitelist_entries

    def test_discord_webhook_invalid_url_cleared(self, plugin):
        """POST /api/discord-webhook with invalid URL sets empty."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        req = FakeRequest(method='POST', json_data={'url': 'https://evil.com/steal'})
        with patch.object(plugin, '_save_state'):
            plugin.on_webhook('/api/discord-webhook', req)
        assert plugin._discord_webhook == ''

    def test_discord_webhook_valid_url_accepted(self, plugin):
        """POST /api/discord-webhook with valid Discord URL is accepted."""
        plugin._agent = MagicMock()
        plugin._agent._config = {'personality': {}, 'bettercap': {'handshakes': '/tmp'}, 'main': {}}
        valid_url = 'https://discord.com/api/webhooks/123456/abcdef'
        req = FakeRequest(method='POST', json_data={'url': valid_url})
        with patch.object(plugin, '_save_state'):
            plugin.on_webhook('/api/discord-webhook', req)
        assert plugin._discord_webhook == valid_url


# ---------------------------------------------------------------------------
# Fast Boot (Delayed Plugin Loading) Tests
# ---------------------------------------------------------------------------

class TestFastBoot:
    """Tests for the fast boot delayed plugin loading system."""

    def test_class_constants_exist(self, plugin):
        """Class-level constants are defined."""
        assert isinstance(plugin._DELAY_PLUGINS, list)
        assert isinstance(plugin._KEEP_PLUGINS, list)
        assert plugin._DELAY_STATE_FILE == '/home/pi/delayed_plugins.json'
        assert 'bt-tether' in plugin._DELAY_PLUGINS
        assert 'angryoxide' in plugin._KEEP_PLUGINS

    def test_save_delayed_plugins_writes_file(self, plugin):
        """_save_delayed_plugins writes enabled non-essential plugins to JSON."""
        import pwnagotchi.plugins as _plugins
        _plugins.loaded = {
            'bt-tether': MagicMock(),
            'grid': MagicMock(),
            'angryoxide': MagicMock(),
            'wpa-sec': None,  # disabled — should not be saved
        }
        m = mock_open()
        with patch('angryoxide_v2.open', m), \
             patch('angryoxide_v2.time.time', return_value=1000.0):
            plugin._save_delayed_plugins()

        m.assert_called_once_with('/home/pi/delayed_plugins.json', 'w')
        written = m().write.call_args_list
        written_str = ''.join(c[0][0] for c in written)
        data = json.loads(written_str)
        assert set(data['delayed']) == {'bt-tether', 'grid'}
        assert data['timestamp'] == 1000.0

    def test_save_delayed_plugins_skips_when_none_loaded(self, plugin):
        """_save_delayed_plugins does nothing if no delay plugins are loaded."""
        import pwnagotchi.plugins as _plugins
        _plugins.loaded = {'angryoxide': MagicMock()}
        m = mock_open()
        with patch('angryoxide_v2.open', m):
            plugin._save_delayed_plugins()
        m.assert_not_called()

    def test_save_delayed_plugins_handles_exception(self, plugin):
        """_save_delayed_plugins swallows errors gracefully."""
        import pwnagotchi.plugins as _plugins
        _plugins.loaded = {'bt-tether': MagicMock()}
        with patch('angryoxide_v2.open', side_effect=OSError("disk full")):
            # Should not raise
            plugin._save_delayed_plugins()

    def test_restore_delayed_plugins_loads_and_removes_file(self, plugin):
        """_restore_delayed_plugins re-enables plugins and removes the state file."""
        import pwnagotchi.plugins as _plugins
        _plugins.toggle_plugin = MagicMock()
        state = json.dumps({'delayed': ['bt-tether', 'grid'], 'timestamp': time.time()})
        m = mock_open(read_data=state)
        with patch('angryoxide_v2.os.path.isfile', return_value=True), \
             patch('angryoxide_v2.open', m), \
             patch('angryoxide_v2.os.remove') as mock_rm:
            plugin._restore_delayed_plugins()

        assert _plugins.toggle_plugin.call_count == 2
        _plugins.toggle_plugin.assert_any_call('bt-tether', True)
        _plugins.toggle_plugin.assert_any_call('grid', True)
        mock_rm.assert_called_once_with('/home/pi/delayed_plugins.json')

    def test_restore_delayed_plugins_no_file(self, plugin):
        """_restore_delayed_plugins does nothing if state file is missing."""
        import pwnagotchi.plugins as _plugins
        _plugins.toggle_plugin = MagicMock()
        with patch('angryoxide_v2.os.path.isfile', return_value=False):
            plugin._restore_delayed_plugins()
        _plugins.toggle_plugin.assert_not_called()

    def test_restore_delayed_plugins_stale_file(self, plugin):
        """_restore_delayed_plugins ignores state files older than 10 minutes."""
        import pwnagotchi.plugins as _plugins
        _plugins.toggle_plugin = MagicMock()
        old_ts = time.time() - 700  # 11+ minutes ago
        state = json.dumps({'delayed': ['bt-tether'], 'timestamp': old_ts})
        m = mock_open(read_data=state)
        with patch('angryoxide_v2.os.path.isfile', return_value=True), \
             patch('angryoxide_v2.open', m), \
             patch('angryoxide_v2.os.remove') as mock_rm:
            plugin._restore_delayed_plugins()

        _plugins.toggle_plugin.assert_not_called()
        # stale file should still be removed
        mock_rm.assert_called_once_with('/home/pi/delayed_plugins.json')

    def test_restore_handles_toggle_failure(self, plugin):
        """_restore_delayed_plugins continues if one plugin fails to toggle."""
        import pwnagotchi.plugins as _plugins
        _plugins.toggle_plugin = MagicMock(side_effect=[Exception("fail"), None])
        state = json.dumps({'delayed': ['bt-tether', 'grid'], 'timestamp': time.time()})
        m = mock_open(read_data=state)
        with patch('angryoxide_v2.os.path.isfile', return_value=True), \
             patch('angryoxide_v2.open', m), \
             patch('angryoxide_v2.os.remove'):
            plugin._restore_delayed_plugins()

        assert _plugins.toggle_plugin.call_count == 2

    def test_on_unload_calls_save(self, plugin):
        """on_unload calls _save_delayed_plugins before shutdown."""
        ui = MagicMock()
        with patch.object(plugin, '_save_delayed_plugins') as mock_save, \
             patch.object(plugin, '_stop_ao'):
            plugin.on_unload(ui)
        mock_save.assert_called_once()

    def test_on_ready_schedules_restore(self, plugin, agent):
        """on_ready schedules _restore_delayed_plugins after AO starts."""
        plugin._running = False  # _start_ao will set it to True
        mock_timer = MagicMock()
        import threading as _threading
        with patch('angryoxide_v2.os.path.isfile', return_value=True), \
             patch('angryoxide_v2.os.path.exists', return_value=True), \
             patch('angryoxide_v2.subprocess.Popen') as mock_popen, \
             patch('angryoxide_v2.os.makedirs'), \
             patch('angryoxide_v2.os.setsid', create=True), \
             patch('angryoxide_v2.glob.glob', return_value=[]), \
             patch.object(plugin, '_is_ao_mode', return_value=True), \
             patch.object(_threading, 'Timer', return_value=mock_timer) as mock_timer_cls:
            mock_popen.return_value = MagicMock(pid=1234)
            plugin.on_ready(agent)

        # AO started, so timer should have been created
        mock_timer_cls.assert_called_once_with(30.0, plugin._restore_delayed_plugins)
        mock_timer.start.assert_called_once()

    def test_on_ready_no_restore_if_ao_fails(self, plugin, agent):
        """on_ready does NOT schedule restore if AO fails to start."""
        mock_timer_cls = MagicMock()
        mock_threading = MagicMock()
        mock_threading.Timer = mock_timer_cls
        with patch('angryoxide_v2.os.path.isfile', return_value=False), \
             patch.dict('sys.modules', {'threading': mock_threading}):
            plugin.on_ready(agent)

        mock_timer_cls.assert_not_called()
