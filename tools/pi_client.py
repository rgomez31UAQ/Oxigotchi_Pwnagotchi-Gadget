#!/usr/bin/env python3
"""
Pi Client — local library for talking to pi_helper daemon.
All firmware scripts import this instead of using paramiko directly.

Usage:
    from pi_client import Pi

    pi = Pi()                    # connects to 10.0.0.2:8888
    pi.status()                  # full system status
    pi.read_mem(addr, 8)         # read 8 bytes of firmware RAM
    pi.write_mem(addr, b'..')    # write bytes to firmware RAM
    pi.read_bulk([(a, n), ...])  # batch read (one TCP round-trip)
    pi.shell("ls /tmp")          # run shell command
    pi.dmesg()                   # recent dmesg
"""
import socket
import json
import struct
import time
import paramiko

PI_HOST = "10.0.0.2"
PI_PORT = 8888
PI_USER = "pi"
PI_PASS = "raspberry"


class Pi:
    def __init__(self, host=PI_HOST, port=PI_PORT, timeout=30):
        self.host = host
        self.port = port
        self.timeout = timeout
        self._sock = None
        self._buf = b""
        self.connect()

    def connect(self):
        if self._sock:
            try:
                self._sock.close()
            except:
                pass
        self._sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._sock.settimeout(self.timeout)
        self._sock.connect((self.host, self.port))
        self._buf = b""

    def _send(self, obj):
        """Send JSON command, receive JSON response."""
        line = json.dumps(obj) + "\n"
        self._sock.sendall(line.encode())
        # Read response
        while b"\n" not in self._buf:
            chunk = self._sock.recv(65536)
            if not chunk:
                raise ConnectionError("Pi helper disconnected")
            self._buf += chunk
        resp_line, self._buf = self._buf.split(b"\n", 1)
        return json.loads(resp_line)

    def close(self):
        if self._sock:
            self._sock.close()
            self._sock = None

    # === High-level API ===

    def ping(self):
        return self._send({"cmd": "ping"})

    def status(self):
        """Full system status: interfaces, bus, pwnagotchi, dmesg errors, firmware size."""
        return self._send({"cmd": "status"})

    def read_mem(self, addr, length=8):
        """Read firmware RAM. Returns bytes or None."""
        r = self._send({"cmd": "read_mem", "addr": addr, "len": length})
        if r.get("ok") and r.get("hex"):
            return bytes.fromhex(r["hex"])
        return None

    def write_mem(self, addr, data):
        """Write bytes to firmware RAM. Returns True/False."""
        r = self._send({"cmd": "write_mem", "addr": addr, "hex": data.hex()})
        return r.get("ok", False)

    def read_bulk(self, regions):
        """Read multiple memory regions in one call.
        regions = [(addr, len), ...]
        Returns list of bytes objects (or None for failures).
        """
        r = self._send({"cmd": "read_mem_bulk",
                        "regions": [[a, l] for a, l in regions]})
        if not r.get("ok"):
            return [None] * len(regions)
        results = []
        for read in r.get("reads", []):
            if read.get("ok") and read.get("hex"):
                results.append(bytes.fromhex(read["hex"]))
            else:
                results.append(None)
        # Pad if bus died mid-read
        while len(results) < len(regions):
            results.append(None)
        return results

    def shell(self, cmd):
        """Run shell command on Pi. Returns (rc, stdout, stderr)."""
        r = self._send({"cmd": "shell", "run": cmd})
        return r.get("rc", -1), r.get("stdout", ""), r.get("stderr", "")

    def dmesg(self, seconds=60):
        """Get recent dmesg lines."""
        r = self._send({"cmd": "dmesg", "seconds": seconds})
        return r.get("lines", [])

    def file_read(self, path):
        """Read a file from Pi. Returns bytes."""
        r = self._send({"cmd": "file", "op": "read", "path": path})
        if r.get("ok"):
            return bytes.fromhex(r["hex"])
        return None

    def file_write(self, path, data):
        """Write a file on Pi."""
        r = self._send({"cmd": "file", "op": "write", "path": path, "hex": data.hex()})
        return r.get("ok", False)

    def file_exists(self, path):
        r = self._send({"cmd": "file", "op": "exists", "path": path})
        return r.get("exists", False)

    # === Convenience ===

    def bus_alive(self):
        s = self.status()
        return s.get("bus", False)

    def wait_for_monitor(self, timeout=180, poll=5):
        """Wait until wlan0mon is up."""
        start = time.time()
        while time.time() - start < timeout:
            s = self.status()
            if s.get("wlan0mon"):
                return True
            time.sleep(poll)
        return False

    def verify_thresholds(self, threshold_config=None):
        """Read all 3 threshold addresses. Returns dict with status.

        threshold_config: list of (addr, name, patched_val, orig_val) tuples.
            Load from firmware config file. If None, returns empty dict.
        """
        if threshold_config is None:
            # Threshold addresses must be loaded from firmware config
            print("WARNING: No threshold config provided. Load addresses from firmware config.")
            return {}

        addrs = threshold_config
        regions = [(a, 8) for a, _, _, _ in addrs]
        reads = self.read_bulk(regions)

        results = {}
        for (addr, name, patched_val, orig_val), data in zip(addrs, reads):
            if data is None:
                results[name] = {"status": "read_fail"}
            elif data[0] == patched_val:
                results[name] = {"status": "patched", "val": data[0], "raw": data[:4].hex()}
            elif data[0] == orig_val:
                results[name] = {"status": "original", "val": data[0], "raw": data[:4].hex()}
            else:
                results[name] = {"status": "unknown", "val": data[0], "raw": data[:4].hex()}
        return results

    def print_status(self):
        """Pretty-print full status."""
        s = self.status()
        print(f"  Bus: {'ALIVE' if s.get('bus') else 'DEAD'}")
        print(f"  wlan0mon: {'UP' if s.get('wlan0mon') else 'DOWN'}")
        print(f"  pwnagotchi: {s.get('pwnagotchi', '?')}")
        print(f"  uptime: {s.get('uptime_secs', 0):.0f}s")
        print(f"  fw_size: {s.get('fw_size', 0)}")
        print(f"  bus_down: {s.get('bus_down_count', '?')}  sdio_err: {s.get('sdio_errors', '?')}")
        if s.get('dmesg_tail'):
            print(f"  dmesg (last relevant):")
            for line in s['dmesg_tail'][-5:]:
                print(f"    {line}")
        return s


def deploy_helper(host=PI_HOST, user=PI_USER, password=PI_PASS):
    """Deploy pi_helper.py to the Pi via SSH and start it."""
    import os
    helper_path = os.path.join(os.path.dirname(__file__), "pi_helper.py")

    print(f"Deploying pi_helper to {host}...")
    client = paramiko.SSHClient()
    client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
    client.connect(host, username=user, password=password, timeout=30)

    sftp = client.open_sftp()
    sftp.put(helper_path, "/home/pi/pi_helper.py")
    sftp.close()

    # Kill any existing instance, start new one
    stdin, stdout, stderr = client.exec_command(
        "sudo pkill -f 'python3.*pi_helper' 2>/dev/null; "
        "sleep 0.5; "
        "sudo nohup python3 /home/pi/pi_helper.py > /tmp/pi_helper.log 2>&1 &"
    )
    stdout.read()  # wait for command to complete
    time.sleep(1)

    # Verify it's running
    stdin, stdout, stderr = client.exec_command("pgrep -f 'python3.*pi_helper'")
    pid = stdout.read().decode().strip()
    client.close()

    if pid:
        print(f"  Helper running (PID {pid})")
        return True
    else:
        print("  Failed to start helper!")
        return False


def install_helper_service(host=PI_HOST, user=PI_USER, password=PI_PASS):
    """Install pi_helper as a systemd service so it auto-starts on boot."""
    import os
    helper_path = os.path.join(os.path.dirname(__file__), "pi_helper.py")

    print(f"Installing pi_helper service on {host}...")
    client = paramiko.SSHClient()
    client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
    client.connect(host, username=user, password=password, timeout=30)

    sftp = client.open_sftp()
    sftp.put(helper_path, "/home/pi/pi_helper.py")

    service = """[Unit]
Description=Pi Helper Daemon for firmware development
After=network.target

[Service]
Type=simple
ExecStart=/usr/bin/python3 /home/pi/pi_helper.py
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
"""
    with sftp.file("/tmp/pi-helper.service", 'w') as f:
        f.write(service)
    sftp.close()

    cmds = (
        "sudo cp /tmp/pi-helper.service /etc/systemd/system/ && "
        "sudo systemctl daemon-reload && "
        "sudo systemctl enable pi-helper && "
        "sudo systemctl restart pi-helper && "
        "sleep 1 && "
        "systemctl is-active pi-helper"
    )
    stdin, stdout, stderr = client.exec_command(cmds, timeout=15)
    out = stdout.read().decode().strip()
    client.close()

    if out == "active":
        print("  Service installed and running")
        return True
    else:
        print(f"  Service status: {out}")
        return False


if __name__ == "__main__":
    import sys
    if len(sys.argv) > 1 and sys.argv[1] == "deploy":
        deploy_helper()
    elif len(sys.argv) > 1 and sys.argv[1] == "install":
        install_helper_service()
    else:
        # Quick test
        try:
            pi = Pi()
            print("Connected to Pi helper!")
            pi.print_status()

            print("\nThreshold check:")
            t = pi.verify_thresholds()
            for name, info in t.items():
                print(f"  {name}: {info}")

            pi.close()
        except ConnectionRefusedError:
            print(f"Cannot connect to {PI_HOST}:{PI_PORT}")
            print("Deploy first: python pi_client.py deploy")
        except Exception as e:
            print(f"Error: {e}")
            print("Deploy first: python pi_client.py deploy")
