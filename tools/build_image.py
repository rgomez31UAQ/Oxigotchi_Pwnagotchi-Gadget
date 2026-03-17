#!/usr/bin/env python3
"""
build_image.py — Create a shippable Oxigotchi SD card image from a live Pi.

Steps:
  1. SSH to Pi, stop services gracefully
  2. Strip personal data (SSH keys, API keys, bash history, WiFi creds, etc.)
  3. Zero free space for better compression
  4. Stream dd of the block device over SSH → local .img file
  5. Shrink with PiShrink (if available on MSYS2/WSL)
  6. Compress to .img.gz

Usage:
    python build_image.py                         # full pipeline
    python build_image.py --clean-only            # just strip data, no image
    python build_image.py --skip-clean            # image without stripping
    python build_image.py --host 10.0.0.2         # custom Pi address
    python build_image.py --output oxigotchi.img  # custom output name
"""

import argparse
import os
import sys
import time
import subprocess
import gzip
import shutil

try:
    import paramiko
except ImportError:
    print("ERROR: paramiko required — pip install paramiko")
    sys.exit(1)

PI_HOST = "10.0.0.2"
PI_USER = "pi"
PI_PASS = "raspberry"
SD_DEVICE = "/dev/mmcblk0"
OUTPUT_DIR = os.path.expanduser("~/oxigotchi-images")

# Files and directories to clean on the Pi before imaging
PERSONAL_DATA = {
    "files_to_delete": [
        "/home/pi/.bash_history",
        "/home/pi/.python_history",
        "/home/pi/.lesshst",
        "/home/pi/.viminfo",
        "/home/pi/.wget-hsts",
        "/home/pi/.nano/search_history",
        "/home/pi/last_session_cache.json",
        "/root/.bash_history",
        "/root/.python_history",
        "/root/.lesshst",
        "/root/.viminfo",
        "/tmp/pi_helper.log",
        "/var/log/auth.log",
        "/var/log/daemon.log",
        "/var/log/syslog",
        "/var/log/messages",
        "/var/log/kern.log",
    ],
    "dirs_to_clean": [
        "/home/pi/.ssh/known_hosts",
        "/home/pi/.cache/",
        "/root/.cache/",
        "/var/log/journal/",
    ],
    "files_to_truncate": [
        "/var/log/pwnagotchi.log",
        "/var/log/wtmp",
        "/var/log/btmp",
        "/var/log/lastlog",
    ],
    "patterns_to_scrub": {
        # file → [(pattern, replacement)]
        "/etc/pwnagotchi/conf.d/angryoxide-v5.toml": [
            # Wipe wpa-sec API key
            (r'api_key\s*=\s*"[^"]*"', 'api_key = "YOUR_WPA_SEC_API_KEY"'),
        ],
    },
    "ssh_keys_to_regenerate": True,
    "reset_hostname": "oxigotchi",
    "reset_password": None,  # None = leave default, or set new password
}


def ssh_connect(host, user, password, timeout=30):
    """Create SSH connection with banner suppression."""
    client = paramiko.SSHClient()
    client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
    client.connect(host, username=user, password=password,
                   timeout=timeout, banner_timeout=30)
    return client


def ssh_run(client, cmd, sudo=True, timeout=120):
    """Run command via SSH, return (stdout, stderr, exit_code)."""
    if sudo and not cmd.startswith("sudo "):
        cmd = "sudo " + cmd
    stdin, stdout, stderr = client.exec_command(cmd, timeout=timeout)
    out = stdout.read().decode(errors='replace').strip()
    err = stderr.read().decode(errors='replace').strip()
    code = stdout.channel.recv_exit_status()
    return out, err, code


def log(msg):
    ts = time.strftime("%H:%M:%S")
    print(f"[{ts}] {msg}")


def step_stop_services(client):
    """Gracefully stop services before imaging."""
    log("Stopping services...")
    services = ["pwnagotchi", "bettercap", "bt-keepalive.timer",
                "oxigotchi-splash", "pi_helper"]
    for svc in services:
        ssh_run(client, f"systemctl stop {svc} 2>/dev/null", timeout=30)
    time.sleep(2)
    log("  Services stopped")


def step_clean_personal_data(client):
    """Strip all personal/identifying data from the Pi."""
    log("Stripping personal data...")
    cleaned = 0

    # Delete files
    for f in PERSONAL_DATA["files_to_delete"]:
        out, err, code = ssh_run(client, f"rm -f {f}")
        if code == 0:
            cleaned += 1

    # Clean directories
    for d in PERSONAL_DATA["dirs_to_clean"]:
        ssh_run(client, f"rm -rf {d}")
        cleaned += 1

    # Truncate log files (keep the file, empty contents)
    for f in PERSONAL_DATA["files_to_truncate"]:
        ssh_run(client, f"truncate -s 0 {f}")
        cleaned += 1

    # Scrub patterns in config files
    for filepath, patterns in PERSONAL_DATA["patterns_to_scrub"].items():
        for pattern, replacement in patterns:
            ssh_run(client,
                    f"sed -i 's|{pattern}|{replacement}|g' {filepath} 2>/dev/null")
            cleaned += 1

    # Clear handshake captures (user's data, not ours to ship)
    ssh_run(client, "rm -f /etc/pwnagotchi/handshakes/*.pcap")
    ssh_run(client, "rm -f /etc/pwnagotchi/handshakes/*.pcapng")
    cleaned += 1

    # Clear AO state (targets, whitelist customizations)
    ssh_run(client, "rm -f /etc/pwnagotchi/custom-plugins/angryoxide_state.json")
    cleaned += 1

    # Clear wpa-sec results
    ssh_run(client, "rm -f /etc/pwnagotchi/handshakes/wpa-sec.cracked*")
    cleaned += 1

    # Regenerate SSH host keys (so every image has unique keys)
    if PERSONAL_DATA["ssh_keys_to_regenerate"]:
        log("  Regenerating SSH host keys...")
        ssh_run(client, "rm -f /etc/ssh/ssh_host_*")
        ssh_run(client, "dpkg-reconfigure openssh-server 2>/dev/null",
                timeout=60)
        cleaned += 1

    # Reset hostname
    hostname = PERSONAL_DATA.get("reset_hostname")
    if hostname:
        ssh_run(client, f"hostnamectl set-hostname {hostname}")
        ssh_run(client, f"sed -i 's/127.0.1.1.*/127.0.1.1\\t{hostname}/' /etc/hosts")
        # Also set pwnagotchi name
        ssh_run(client, f"echo '{hostname}' > /etc/pwnagotchi/hostname")
        cleaned += 1

    # Clear machine-id (regenerated on next boot)
    ssh_run(client, "truncate -s 0 /etc/machine-id")
    ssh_run(client, "rm -f /var/lib/dbus/machine-id")
    cleaned += 1

    # Clear systemd journal
    ssh_run(client, "journalctl --vacuum-size=1K 2>/dev/null")
    cleaned += 1

    # Clear apt cache
    ssh_run(client, "apt-get clean 2>/dev/null")
    cleaned += 1

    # Clear pwnagotchi brain/AI data
    ssh_run(client, "rm -f /etc/pwnagotchi/brain.json")
    ssh_run(client, "rm -f /etc/pwnagotchi/brain.nn")
    cleaned += 1

    # Clear grid identity (user gets their own on first boot)
    ssh_run(client, "rm -f /etc/pwnagotchi/id_rsa*")
    cleaned += 1

    log(f"  Cleaned {cleaned} items")


def step_zero_free_space(client):
    """Write zeros to free space for better gzip compression."""
    log("Zeroing free space (this takes a while)...")
    # Write zeros until disk is full, then delete
    ssh_run(client,
            "dd if=/dev/zero of=/home/pi/_zero bs=4M 2>/dev/null; "
            "rm -f /home/pi/_zero; sync",
            timeout=600)
    log("  Free space zeroed")


def step_sync_and_drop_caches(client):
    """Sync filesystem and drop caches before dd."""
    log("Syncing filesystem...")
    ssh_run(client, "sync && echo 3 > /proc/sys/vm/drop_caches")
    time.sleep(2)


def step_stream_image(client, host, user, password, output_path):
    """Stream dd of the SD card over SSH to a local file."""
    card_size_out, _, _ = ssh_run(client, f"blockdev --getsize64 {SD_DEVICE}")
    try:
        card_bytes = int(card_size_out)
        card_gb = card_bytes / (1024 ** 3)
        log(f"SD card: {card_gb:.1f} GB ({card_bytes} bytes)")
    except ValueError:
        log(f"  WARNING: Could not determine card size, proceeding anyway")
        card_bytes = 0

    log(f"Streaming image to {output_path} ...")
    log(f"  This will take 20-60 minutes over USB networking.")

    # Use subprocess to stream: ssh pi "sudo dd ..." > file
    # This avoids loading the entire image into Python's memory
    ssh_cmd = [
        "ssh",
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "LogLevel=ERROR",
        f"{user}@{host}",
        f"sudo dd if={SD_DEVICE} bs=4M status=none"
    ]

    with open(output_path, "wb") as img_file:
        proc = subprocess.Popen(
            ssh_cmd, stdout=img_file, stderr=subprocess.PIPE
        )

        # Monitor progress
        start = time.time()
        while proc.poll() is None:
            time.sleep(30)
            elapsed = time.time() - start
            if os.path.exists(output_path):
                written = os.path.getsize(output_path)
                written_gb = written / (1024 ** 3)
                rate_mb = (written / (1024 ** 2)) / elapsed if elapsed > 0 else 0
                pct = (written / card_bytes * 100) if card_bytes > 0 else 0
                eta_min = ((card_bytes - written) / (rate_mb * 1024 * 1024) / 60) if rate_mb > 0 else 0
                log(f"  {written_gb:.1f} GB written ({pct:.0f}%) "
                    f"@ {rate_mb:.1f} MB/s, ETA {eta_min:.0f} min")

        rc = proc.returncode
        stderr_out = proc.stderr.read().decode(errors='replace').strip()

    if rc != 0:
        log(f"  ERROR: dd failed (exit {rc}): {stderr_out}")
        return False

    final_size = os.path.getsize(output_path)
    log(f"  Image written: {final_size / (1024**3):.1f} GB")
    return True


def step_try_pishrink(img_path):
    """Try to shrink the image with PiShrink if available."""
    # Check for pishrink.sh in PATH or common locations
    for loc in ["pishrink.sh", "/usr/local/bin/pishrink.sh",
                os.path.expanduser("~/pishrink.sh")]:
        if shutil.which(loc) or os.path.isfile(loc):
            log(f"Running PiShrink ({loc})...")
            result = subprocess.run(
                ["bash", loc, "-s", img_path],
                capture_output=True, text=True, timeout=1800
            )
            if result.returncode == 0:
                new_size = os.path.getsize(img_path)
                log(f"  Shrunk to {new_size / (1024**3):.1f} GB")
                return True
            else:
                log(f"  PiShrink failed: {result.stderr[:200]}")
                return False

    log("  PiShrink not found — skipping (image will be full card size)")
    log("  Install: wget https://raw.githubusercontent.com/Drewsif/PiShrink/master/pishrink.sh")
    return False


def step_compress(img_path):
    """Gzip the image file."""
    gz_path = img_path + ".gz"
    log(f"Compressing to {gz_path} ...")
    start = time.time()

    img_size = os.path.getsize(img_path)

    with open(img_path, "rb") as f_in:
        with gzip.open(gz_path, "wb", compresslevel=6) as f_out:
            copied = 0
            while True:
                chunk = f_in.read(8 * 1024 * 1024)  # 8MB chunks
                if not chunk:
                    break
                f_out.write(chunk)
                copied += len(chunk)
                if copied % (256 * 1024 * 1024) == 0:  # log every 256MB
                    pct = copied / img_size * 100
                    log(f"  Compressed {copied / (1024**3):.1f} / "
                        f"{img_size / (1024**3):.1f} GB ({pct:.0f}%)")

    elapsed = time.time() - start
    gz_size = os.path.getsize(gz_path)
    ratio = gz_size / img_size * 100 if img_size > 0 else 0
    log(f"  Done: {gz_size / (1024**3):.1f} GB "
        f"({ratio:.0f}% of original, {elapsed/60:.0f} min)")

    return gz_path


def step_restart_services(client):
    """Restart services after imaging."""
    log("Restarting services...")
    ssh_run(client, "systemctl start pwnagotchi", timeout=30)
    log("  Services restarted")


def main():
    parser = argparse.ArgumentParser(description="Build shippable Oxigotchi SD image")
    parser.add_argument("--host", default=PI_HOST, help="Pi address")
    parser.add_argument("--user", default=PI_USER, help="SSH user")
    parser.add_argument("--password", default=PI_PASS, help="SSH password")
    parser.add_argument("--device", default=SD_DEVICE, help="SD block device")
    parser.add_argument("--output", default=None, help="Output .img filename")
    parser.add_argument("--output-dir", default=OUTPUT_DIR, help="Output directory")
    parser.add_argument("--clean-only", action="store_true",
                        help="Only strip personal data, don't create image")
    parser.add_argument("--skip-clean", action="store_true",
                        help="Skip data stripping")
    parser.add_argument("--skip-zero", action="store_true",
                        help="Skip zeroing free space (faster, bigger image)")
    parser.add_argument("--skip-shrink", action="store_true",
                        help="Skip PiShrink step")
    parser.add_argument("--keep-raw", action="store_true",
                        help="Keep uncompressed .img after gzipping")
    args = parser.parse_args()

    # Output path
    os.makedirs(args.output_dir, exist_ok=True)
    if args.output:
        img_name = args.output
    else:
        ts = time.strftime("%Y%m%d")
        img_name = f"oxigotchi-{ts}.img"
    img_path = os.path.join(args.output_dir, img_name)

    log("=" * 60)
    log("Oxigotchi Image Builder")
    log("=" * 60)
    log(f"Pi: {args.user}@{args.host}")
    log(f"Output: {img_path}")
    log("")

    # Connect
    log("Connecting to Pi...")
    client = ssh_connect(args.host, args.user, args.password)
    log("  Connected")

    try:
        # Phase 1: Stop services
        step_stop_services(client)

        # Phase 2: Clean personal data
        if not args.skip_clean:
            step_clean_personal_data(client)
        else:
            log("Skipping data cleanup (--skip-clean)")

        if args.clean_only:
            log("Clean-only mode — done.")
            step_restart_services(client)
            return

        # Phase 3: Zero free space
        if not args.skip_zero:
            step_zero_free_space(client)
        else:
            log("Skipping zero fill (--skip-zero)")

        # Phase 4: Sync
        step_sync_and_drop_caches(client)

        # Phase 5: Stream image
        ok = step_stream_image(client, args.host, args.user, args.password,
                               img_path)
        if not ok:
            log("FAILED: Image creation failed")
            step_restart_services(client)
            sys.exit(1)

        # Phase 6: Restart services (Pi is usable again)
        step_restart_services(client)

    finally:
        client.close()

    # Phase 7: Shrink (runs locally, Pi no longer needed)
    if not args.skip_shrink:
        step_try_pishrink(img_path)

    # Phase 8: Compress
    gz_path = step_compress(img_path)

    # Phase 9: Cleanup
    if not args.keep_raw:
        log(f"Removing raw image: {img_path}")
        os.remove(img_path)

    log("")
    log("=" * 60)
    log(f"DONE: {gz_path}")
    log(f"Size: {os.path.getsize(gz_path) / (1024**3):.1f} GB")
    log("=" * 60)
    log("")
    log("Next steps:")
    log(f"  1. Test: flash {gz_path} to a fresh SD card")
    log("  2. Boot and verify cold-start sequence")
    log("  3. Upload to Google Drive / Mega for distribution")


if __name__ == "__main__":
    main()
