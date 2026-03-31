#!/bin/bash
# bake_final.sh — Re-bake oxigotchi-v2.0.img with ALL latest changes
# Run inside WSL: sudo bash /path/to/oxigotchi/tools/bake_final.sh
set -euo pipefail

echo "============================================="
echo "=== Oxigotchi v2.0 Image Bake — FINAL     ==="
echo "============================================="
echo ""

# ─── Setup ───
IMG=/mnt/d/oxigotchi-v2.0.img
REPO=/path/to/oxigotchi
SENTINEL_FILE="/.oxigotchi-baked"

if [ ! -f "$IMG" ]; then
    echo "ERROR: Image not found at $IMG"
    exit 1
fi

# ─── 0a. Cross-compile wlan_keepalive ───
echo "=== 0a. Cross-compile wlan_keepalive ==="
aarch64-linux-gnu-gcc -O2 -static -o /tmp/wlan_keepalive "$REPO/tools/wlan_keepalive.c"
file /tmp/wlan_keepalive
echo "  Cross-compile done"

# ─── 0b. Cleanup previous mounts ───
echo "=== 0b. Cleanup previous mounts ==="
sudo umount /mnt/piroot 2>/dev/null || true
sudo umount /mnt/piboot 2>/dev/null || true
sudo losetup -D 2>/dev/null || true
sleep 1

# ─── 1. Mount image ───
echo "=== 1. Mount image ==="
sudo losetup -fP "$IMG"
LOOPDEV=$(losetup -j "$IMG" | head -1 | cut -d: -f1)
echo "Loop device: $LOOPDEV"
sudo mkdir -p /mnt/piboot /mnt/piroot
sudo mount "${LOOPDEV}p2" /mnt/piroot
sudo mount "${LOOPDEV}p1" /mnt/piboot
PI=/mnt/piroot
echo "Image mounted at $PI"

# ─── 2. Plugins ───
echo ""
echo "=== 2. Copy plugins ==="
sudo mkdir -p $PI/etc/pwnagotchi/custom-plugins
for p in angryoxide.py walkby.py frame_padding.py stub_client.py; do
    sudo cp "$REPO/plugin/$p" "$PI/etc/pwnagotchi/custom-plugins/$p"
    echo "  Copied $p"
done
# stub_client + frame_padding also go to site-packages (imported by patched agent.py)
SITE_PKG="$PI/home/pi/.pwn/lib/python3.13/site-packages/pwnagotchi"
if [ -d "$SITE_PKG" ]; then
    sudo cp "$REPO/plugin/stub_client.py" "$SITE_PKG/stub_client.py"
    sudo cp "$REPO/plugin/frame_padding.py" "$SITE_PKG/frame_padding.py"
    echo "  Copied stub_client.py + frame_padding.py to site-packages"
fi

# ─── 3. Config overlay ───
echo ""
echo "=== 3. Copy config overlay ==="
sudo mkdir -p $PI/etc/pwnagotchi/conf.d
sudo cp "$REPO/config/angryoxide-v5.toml" "$PI/etc/pwnagotchi/conf.d/angryoxide-v5.toml"
echo "  angryoxide-v5.toml installed"

# ─── 4. Bull faces ───
echo ""
echo "=== 4. Copy bull faces ==="
sudo mkdir -p $PI/etc/pwnagotchi/custom-plugins/faces
for f in "$REPO/faces/eink/"*.png; do
    sudo cp "$f" "$PI/etc/pwnagotchi/custom-plugins/faces/$(basename "$f")"
done
echo "  Copied $(ls "$REPO/faces/eink/"*.png | wc -l) face PNGs"

# ─── 5. Tools / scripts ───
echo ""
echo "=== 5. Copy tools ==="
# Core tools
sudo cp "$REPO/tools/apply_patches.sh"     "$PI/usr/local/bin/apply-oxigotchi-patches.sh"
sudo cp "$REPO/tools/wifi-recovery.sh"      "$PI/usr/local/bin/wifi-recovery.sh"
sudo cp "$REPO/tools/oxigotchi-splash.py"   "$PI/usr/local/bin/oxigotchi-splash.py"
sudo cp "$REPO/tools/pwnoxide-mode.sh"      "$PI/usr/local/bin/pwnoxide-mode"

# Toggle scripts (NEWLY ADDED)
sudo cp "$REPO/tools/toggle-bt.sh"          "$PI/usr/local/bin/toggle-bt.sh"
sudo cp "$REPO/tools/toggle-mode.sh"        "$PI/usr/local/bin/toggle-mode.sh"
sudo cp "$REPO/tools/toggle-ao-pwn.sh"      "$PI/usr/local/bin/toggle-ao-pwn.sh"
sudo cp "$REPO/tools/bt-pair.sh"            "$PI/usr/local/bin/bt-pair.sh"

# Bootlog (latest version from repo)
sudo cp "$REPO/tools/bootlog.sh"            "$PI/usr/local/bin/bootlog.sh"

# CRLF fix ALL shell scripts in /usr/local/bin
for f in "$PI"/usr/local/bin/*.sh "$PI/usr/local/bin/pwnoxide-mode"; do
    if [ -f "$f" ]; then
        sudo sed -i 's/\r$//' "$f"
        sudo chmod +x "$f"
    fi
done
sudo chmod +x "$PI/usr/local/bin/oxigotchi-splash.py"
echo "  All tools installed + CRLF fixed + chmod +x"

# Install wlan_keepalive binary
sudo cp /tmp/wlan_keepalive "$PI/usr/local/bin/wlan_keepalive"
sudo chmod +x "$PI/usr/local/bin/wlan_keepalive"
echo "  wlan_keepalive binary installed"

# ─── 6. Services ───
echo ""
echo "=== 6. Install services ==="
# Copy service files from repo
sudo cp "$REPO/services/oxigotchi-patches.service" "$PI/etc/systemd/system/oxigotchi-patches.service"
sudo cp "$REPO/services/oxigotchi-patches.path"    "$PI/etc/systemd/system/oxigotchi-patches.path"
sudo cp "$REPO/services/wifi-recovery.service"     "$PI/etc/systemd/system/wifi-recovery.service"
sudo cp "$REPO/services/resize-rootfs.service"     "$PI/etc/systemd/system/resize-rootfs.service"

# wlan-keepalive service — use the native binary version
cat > /tmp/wlan-keepalive.service <<'WKUNIT'
[Unit]
Description=WiFi monitor interface keepalive (brcmfmac SDIO bus)
After=network.target
Before=pwnagotchi.service

[Service]
Type=simple
ExecStart=/usr/local/bin/wlan_keepalive wlan0mon 100
Restart=always
RestartSec=3
Nice=10
StandardOutput=null
StandardError=journal

[Install]
WantedBy=multi-user.target
WKUNIT
sudo cp /tmp/wlan-keepalive.service "$PI/etc/systemd/system/wlan-keepalive.service"

# Splash service (corrected name)
cat > /tmp/oxigotchi-splash.service <<'SPLUNIT'
[Unit]
Description=Oxigotchi Boot Splash
DefaultDependencies=no
Before=pwnagotchi.service
After=local-fs.target sysinit.target

[Service]
Type=oneshot
ExecStart=/home/pi/.pwn/bin/python3 /usr/local/bin/oxigotchi-splash.py /etc/pwnagotchi/custom-plugins/faces/awake.png
ExecStop=/home/pi/.pwn/bin/python3 /usr/local/bin/oxigotchi-splash.py /etc/pwnagotchi/custom-plugins/faces/shutdown.png
RemainAfterExit=yes
TimeoutStartSec=30
TimeoutStopSec=30

[Install]
WantedBy=sysinit.target
SPLUNIT
sudo cp /tmp/oxigotchi-splash.service "$PI/etc/systemd/system/oxigotchi-splash.service"
# Remove old misspelled service
sudo rm -f "$PI/etc/systemd/system/oxagotchi-splash.service" 2>/dev/null
sudo rm -f "$PI/etc/systemd/system/sysinit.target.wants/oxagotchi-splash.service" 2>/dev/null
# Remove stale splash symlink if wrong name
sudo rm -f "$PI/etc/systemd/system/sysinit.target.wants/oxigotchi-splash.service" 2>/dev/null

# Pwnagotchi splash delay drop-in
sudo mkdir -p "$PI/etc/systemd/system/pwnagotchi.service.d"
cat > /tmp/splash-delay.conf <<'DDUNIT'
[Unit]
After=oxigotchi-splash.service

[Service]
ExecStartPre=/bin/sleep 3
DDUNIT
sudo cp /tmp/splash-delay.conf "$PI/etc/systemd/system/pwnagotchi.service.d/pwnagotchi-splash-delay.conf"

# Emergency SSH service
cat > /tmp/emergency-ssh.service <<'ESHUNIT'
[Unit]
Description=Emergency SSH
After=network.target
Wants=network.target
Conflicts=ssh.service

[Service]
Type=simple
ExecStartPre=/bin/bash -c "test -f /etc/ssh/ssh_host_ed25519_key || ssh-keygen -A"
ExecStart=/usr/sbin/sshd -D -p 22 -o PasswordAuthentication=yes -o PermitRootLogin=yes
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
ESHUNIT
sudo cp /tmp/emergency-ssh.service "$PI/etc/systemd/system/emergency-ssh.service"

# Bootlog service
cat > /tmp/bootlog.service <<'BLOGSVC'
[Unit]
Description=Boot diagnostics and self-healing
After=local-fs.target boot-firmware.mount network.target
Wants=local-fs.target

[Service]
Type=oneshot
ExecStart=/usr/local/bin/bootlog.sh
RemainAfterExit=yes
TimeoutStartSec=60

[Install]
WantedBy=multi-user.target
BLOGSVC
sudo cp /tmp/bootlog.service "$PI/etc/systemd/system/bootlog.service"

# USB0 fallback service + script
cat > /tmp/usb0-fallback.sh <<'USBFSH'
#!/bin/bash
# Fallback: if NM hasn't brought up usb0 after 30s, do it manually
sleep 30
if ! ip addr show usb0 | grep -q "10.0.0.2"; then
    ip addr add 10.0.0.2/24 dev usb0 2>/dev/null
    ip link set usb0 up 2>/dev/null
fi
if ! ip addr show usb0 | grep -q "192.168.137.2"; then
    ip addr add 192.168.137.2/24 dev usb0 2>/dev/null
fi
USBFSH
sudo cp /tmp/usb0-fallback.sh "$PI/usr/local/bin/usb0-fallback.sh"
sudo sed -i 's/\r$//' "$PI/usr/local/bin/usb0-fallback.sh"
sudo chmod +x "$PI/usr/local/bin/usb0-fallback.sh"

cat > /tmp/usb0-fallback.service <<'USBFSVC'
[Unit]
Description=USB0 IP fallback
After=NetworkManager.service

[Service]
Type=oneshot
ExecStart=/usr/local/bin/usb0-fallback.sh
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
USBFSVC
sudo cp /tmp/usb0-fallback.service "$PI/etc/systemd/system/usb0-fallback.service"

# Watchdog service
cat > /tmp/watchdog.service <<'WDGSVC'
[Unit]
Description=Hardware Watchdog
After=multi-user.target

[Service]
Type=oneshot
ExecStart=/bin/bash -c 'if [ -e /dev/watchdog ]; then echo "Watchdog active"; fi'
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
WDGSVC
sudo cp /tmp/watchdog.service "$PI/etc/systemd/system/watchdog.service"

# chmod 644 on ALL service files
for svc in oxigotchi-patches oxigotchi-patches.path wifi-recovery resize-rootfs \
           wlan-keepalive oxigotchi-splash emergency-ssh bootlog usb0-fallback watchdog; do
    # Handle both .service and .path extensions
    for ext in service path; do
        if [ -f "$PI/etc/systemd/system/${svc}.${ext}" ]; then
            sudo chmod 644 "$PI/etc/systemd/system/${svc}.${ext}"
        elif [ -f "$PI/etc/systemd/system/${svc}" ]; then
            sudo chmod 644 "$PI/etc/systemd/system/${svc}"
        fi
    done
done
sudo chmod 644 "$PI/etc/systemd/system/pwnagotchi.service.d/pwnagotchi-splash-delay.conf"
echo "  All services installed, chmod 644 applied"

# ─── 7. Fix config.toml ───
echo ""
echo "=== 7. Fix config.toml ==="
CFG="$PI/etc/pwnagotchi/config.toml"
if [ -f "$CFG" ]; then
    # --- FIRST: Remove nested wpa-sec duplicate section ---
    # The config.toml may have a duplicate [main.plugins.wpa-sec] block that
    # creates a nested dict. Use python to cleanly de-duplicate.
    python3 - "$CFG" <<'PYEOF'
import sys, re

cfg_path = sys.argv[1]
with open(cfg_path) as f:
    lines = f.readlines()

# Find all [main.plugins.wpa-sec] section headers and keep only the first
wpa_sec_header = '[main.plugins.wpa-sec]'
found_first = False
in_dup_section = False
cleaned = []
i = 0
while i < len(lines):
    line = lines[i]
    stripped = line.strip()

    # Detect wpa-sec section header
    if stripped == wpa_sec_header:
        if not found_first:
            found_first = True
            cleaned.append(line)
        else:
            # This is a DUPLICATE section — skip it and all its content
            in_dup_section = True
            i += 1
            continue
    elif in_dup_section:
        # Skip lines until we hit the next section header or EOF
        if stripped.startswith('[') and stripped != wpa_sec_header:
            in_dup_section = False
            cleaned.append(line)
        else:
            i += 1
            continue
    else:
        cleaned.append(line)
    i += 1

with open(cfg_path, 'w') as f:
    f.writelines(cleaned)
print("  wpa-sec dedup done")
PYEOF

    # main.name = "oxigotchi"
    sudo sed -i 's/^\(main\.name\s*=\s*\)"[^"]*"/\1"oxigotchi"/' "$CFG"
    sudo sed -i '/^\[main\]/,/^\[/{s/^\(name\s*=\s*\)"[^"]*"/\1"oxigotchi"/}' "$CFG"

    # font = "DejaVuSansMono"
    sudo sed -i 's/^\(ui\.font\.name\s*=\s*\)"[^"]*"/\1"DejaVuSansMono"/' "$CFG"
    sudo sed -i '/^\[ui\.font\]/,/^\[/{s/^\(name\s*=\s*\)"[^"]*"/\1"DejaVuSansMono"/}' "$CFG"

    # AO enabled=true
    sudo sed -i '/^\[main\.plugins\.angryoxide\]/,/^\[/{s/^enabled = false/enabled = true/}' "$CFG"

    # Whitelist
    sudo sed -i 's/^\(main\.whitelist\s*=\s*\).*/\1["YourNetwork", "YourNetwork-5G"]/' "$CFG"
    sudo sed -i '/^\[main\]/,/^\[/{s/^\(whitelist\s*=\s*\).*/\1["YourNetwork", "YourNetwork-5G"]/}' "$CFG"

    # bt-tether disabled
    sudo sed -i '/^\[main\.plugins\.bt-tether\]/,/^\[/{s/^\(enabled\s*=\s*\)true/\1false/}' "$CFG"

    # ui.faces.png = true
    sudo sed -i '/^\[ui\.faces\]/,/^\[/{s/^\(png\s*=\s*\)false/\1true/}' "$CFG"

    echo "  config.toml patched (name, font, whitelist, bt-tether, png, wpa-sec dedup)"
else
    echo "  WARNING: config.toml not found at $CFG"
fi

# ─── 8. Hostname ───
echo ""
echo "=== 8. Fix hostname ==="
echo "oxigotchi" | sudo tee "$PI/etc/hostname" > /dev/null
sudo sed -i '/^127\.0\.1\.1[[:space:]]/s/[[:space:]].*$/\toxigotchi/' "$PI/etc/hosts"
if ! grep -q '127.0.1.1' "$PI/etc/hosts"; then
    echo "127.0.1.1	oxigotchi" | sudo tee -a "$PI/etc/hosts" > /dev/null
fi
echo "  Hostname set to oxigotchi"

# ─── 9. Dual-IP NM config ───
echo ""
echo "=== 9. Dual-IP NetworkManager config ==="
NM_DIR="$PI/etc/NetworkManager/system-connections"
sudo mkdir -p "$NM_DIR"

cat > /tmp/nm-shared.conf <<'NM1'
[connection]
id=USB Gadget (shared)
uuid=69b44b21-30f2-4bbf-a0d9-af5637ef9e25
type=ethernet
autoconnect=true
autoconnect-priority=10
interface-name=usb0

[ethernet]

[ipv4]
address1=10.0.0.2/24
address2=192.168.137.2/24
gateway=10.0.0.1
dns=8.8.8.8;1.1.1.1;
method=manual

[ipv6]
addr-gen-mode=default
method=disabled

[proxy]
NM1
sudo cp /tmp/nm-shared.conf "$NM_DIR/USB Gadget (shared).nmconnection"
sudo chmod 600 "$NM_DIR/USB Gadget (shared).nmconnection"

cat > /tmp/nm-client.conf <<'NM2'
[connection]
id=USB Gadget (client)
uuid=0dbeff2f-2c11-4f0a-b1d1-4f7220fceccf
type=ethernet
autoconnect=false
interface-name=usb0

[ethernet]

[ipv4]
method=disabled

[ipv6]
method=disabled

[proxy]
NM2
sudo cp /tmp/nm-client.conf "$NM_DIR/USB Gadget (client).nmconnection"
sudo chmod 600 "$NM_DIR/USB Gadget (client).nmconnection"
echo "  Dual-IP NM configs installed (10.0.0.2 + 192.168.137.2)"

# ─── 10. Rootfs expanded sentinel ───
echo ""
echo "=== 10. Create rootfs-expanded sentinel ==="
sudo touch "$PI/var/lib/.rootfs-expanded"
echo "  /var/lib/.rootfs-expanded created"

# ─── 11. Blacklist camera ───
echo ""
echo "=== 11. Blacklist camera module ==="
sudo mkdir -p "$PI/etc/modprobe.d"
echo "blacklist bcm2835_v4l2" | sudo tee "$PI/etc/modprobe.d/blacklist-camera.conf" > /dev/null
echo "  Camera blacklisted"

# ─── 12. Consolidate handshake dirs ───
echo ""
echo "=== 12. Consolidate handshake directories ==="
HSDIR="$PI/etc/pwnagotchi/handshakes"
AODIR="$PI/root/handshakes"
sudo mkdir -p "$HSDIR"
if [ -d "$AODIR" ] && [ ! -L "$AODIR" ]; then
    sudo cp -n "$AODIR"/* "$HSDIR/" 2>/dev/null || true
    sudo rm -rf "$AODIR"
fi
if [ -L "$AODIR" ]; then
    sudo rm -f "$AODIR"
fi
sudo ln -sf /etc/pwnagotchi/handshakes "$AODIR"
echo "  /root/handshakes -> /etc/pwnagotchi/handshakes (symlink)"

# ─── 13. Disable unwanted services ───
echo ""
echo "=== 13. Disable unwanted services ==="
for svc in ModemManager systemd-networkd usb0-ip rpi-usb-gadget-ics userconfig pi-helper rpi-eeprom-update; do
    sudo rm -f "$PI/etc/systemd/system/multi-user.target.wants/${svc}.service" 2>/dev/null
    echo "  Disabled: $svc"
done
sudo rm -f "$PI/etc/systemd/network/10-usb0.network" 2>/dev/null

# ─── 14. Enable services ───
echo ""
echo "=== 14. Enable services ==="
sudo mkdir -p "$PI/etc/systemd/system/multi-user.target.wants"
for svc in wlan-keepalive wifi-recovery bootlog emergency-ssh usb0-fallback resize-rootfs watchdog oxigotchi-patches; do
    sudo ln -sf "/etc/systemd/system/${svc}.service" "$PI/etc/systemd/system/multi-user.target.wants/${svc}.service"
    echo "  Enabled: $svc"
done
# Also enable the .path unit
sudo ln -sf "/etc/systemd/system/oxigotchi-patches.path" "$PI/etc/systemd/system/multi-user.target.wants/oxigotchi-patches.path"
echo "  Enabled: oxigotchi-patches.path"

# sysinit.target.wants for splash
sudo mkdir -p "$PI/etc/systemd/system/sysinit.target.wants"
sudo ln -sf /etc/systemd/system/oxigotchi-splash.service "$PI/etc/systemd/system/sysinit.target.wants/oxigotchi-splash.service"
echo "  Enabled: oxigotchi-splash (sysinit)"

# ─── 15. SSH setup ───
echo ""
echo "=== 15. SSH configuration ==="
sudo mkdir -p "$PI/etc/ssh/sshd_config.d"
printf 'PasswordAuthentication yes\nPermitRootLogin yes\nKbdInteractiveAuthentication yes\nUseDNS no\n' | sudo tee "$PI/etc/ssh/sshd_config.d/99-oxigotchi.conf" > /dev/null

# Generate SSH host keys if missing
if [ ! -f "$PI/etc/ssh/ssh_host_ed25519_key" ]; then
    sudo ssh-keygen -A -f "$PI" 2>&1
fi
sudo chmod 600 $PI/etc/ssh/ssh_host_*_key 2>/dev/null || true
sudo chmod 644 $PI/etc/ssh/ssh_host_*_key.pub 2>/dev/null || true

# Install user SSH authorized keys
sudo mkdir -p "$PI/home/pi/.ssh"
AUTHKEYS="$PI/home/pi/.ssh/authorized_keys"
PUBKEY="ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIANe4Gwnsedd4fjT6CGTqg4KpOh9oHWOiYY8WJxelSLv oxigotchi"
if ! grep -qF "$PUBKEY" "$AUTHKEYS" 2>/dev/null; then
    echo "$PUBKEY" | sudo tee -a "$AUTHKEYS" > /dev/null
fi
sudo chmod 700 "$PI/home/pi/.ssh" 2>/dev/null || true
sudo chmod 600 "$AUTHKEYS" 2>/dev/null || true
sudo chown -R 1000:1000 "$PI/home/pi/.ssh" 2>/dev/null || true
echo "  SSH configured (password auth, host keys, authorized_keys)"

# ─── 16. Disable cloud-init ───
echo ""
echo "=== 16. Disable cloud-init ==="
sudo mkdir -p "$PI/etc/cloud"
sudo touch "$PI/etc/cloud/cloud-init.disabled"
echo "  cloud-init disabled"

# ─── 17. Misc system fixes ───
echo ""
echo "=== 17. Misc system fixes ==="

# Default target: multi-user
sudo ln -sf /lib/systemd/system/multi-user.target "$PI/etc/systemd/system/default.target"
echo "  Default target: multi-user"

# Neutralize rc.local
printf '#!/bin/bash\nexit 0\n' | sudo tee "$PI/etc/rc.local" > /dev/null
sudo chmod +x "$PI/etc/rc.local"
echo "  rc.local neutralized"

# Swap config
printf 'CONF_SWAPSIZE=100\nCONF_SWAPFILE=/var/swap\n' | sudo tee "$PI/etc/dphys-swapfile" > /dev/null
echo "  Swap = 100MB"

# tmpfs for /tmp
if ! grep -q '/tmp' "$PI/etc/fstab"; then
    echo 'tmpfs /tmp tmpfs defaults,noatime,nosuid,size=50m 0 0' | sudo tee -a "$PI/etc/fstab" > /dev/null
fi
echo "  /tmp = tmpfs"

# Disable apt daily timers
sudo rm -f "$PI/etc/systemd/system/timers.target.wants/apt-daily.timer" 2>/dev/null
sudo rm -f "$PI/etc/systemd/system/timers.target.wants/apt-daily-upgrade.timer" 2>/dev/null
echo "  apt daily timers disabled"

# Timezone
sudo ln -sf /usr/share/zoneinfo/Europe/Helsinki "$PI/etc/localtime" 2>/dev/null
echo "  Timezone: Europe/Helsinki"

# Hardware watchdog in config.txt
BOOT_CFG="$PI/boot/firmware/config.txt"
if [ ! -f "$BOOT_CFG" ]; then
    BOOT_CFG="/mnt/piboot/config.txt"
fi
if [ -f "$BOOT_CFG" ]; then
    if ! grep -q 'dtparam=watchdog=on' "$BOOT_CFG"; then
        echo 'dtparam=watchdog=on' | sudo tee -a "$BOOT_CFG" > /dev/null
        echo "  Hardware watchdog enabled in config.txt"
    else
        echo "  Hardware watchdog already in config.txt"
    fi
fi

# modules.conf — ensure brcmfmac loads
MODULES_CONF="$PI/etc/modules-load.d/modules.conf"
if [ -f "$MODULES_CONF" ]; then
    if ! grep -q 'brcmfmac' "$MODULES_CONF"; then
        echo 'brcmfmac' | sudo tee -a "$MODULES_CONF" > /dev/null
        echo "  Added brcmfmac to modules.conf"
    else
        echo "  brcmfmac already in modules.conf"
    fi
else
    sudo mkdir -p "$(dirname "$MODULES_CONF")"
    echo 'brcmfmac' | sudo tee "$MODULES_CONF" > /dev/null
    echo "  Created modules.conf with brcmfmac"
fi

# ─── 18. Clean disk bloat ───
echo ""
echo "=== 18. Clean disk bloat ==="
sudo rm -rf "$PI/root/.rustup" 2>/dev/null
sudo rm -rf "$PI/root/go" 2>/dev/null
sudo rm -f "$PI/home/pi/swapfile" 2>/dev/null
sudo rm -f "$PI/home/pi/oxigotchi-backup.tar" 2>/dev/null
sudo rm -rf "$PI/home/pi/.vscode-server/data/CachedExtensionVSIXs" 2>/dev/null
sudo rm -f "$PI/etc/pwnagotchi/handshakes/.kismet" 2>/dev/null
sudo rm -rf "$PI/var/log/journal/"* 2>/dev/null
echo "  Bloat cleaned"

# ─── 19. Cleanup stale files ───
echo ""
echo "=== 19. Cleanup stale files ==="
sudo rm -f "$PI/usr/local/bin/oxagotchi-splash.py" 2>/dev/null
echo "  Removed stale oxagotchi-splash.py (typo variant)"

# ─── 20. Final CRLF fix ALL .sh files in /usr/local/bin ───
echo ""
echo "=== 20. CRLF fix all shell scripts ==="
for f in "$PI"/usr/local/bin/*.sh "$PI/usr/local/bin/pwnoxide-mode"; do
    if [ -f "$f" ]; then
        sudo sed -i 's/\r$//' "$f"
    fi
done
echo "  All .sh files in /usr/local/bin CRLF-fixed"

# ─── 21. Create sentinel file ───
echo ""
echo "=== 21. Create sentinel file ==="
echo "oxigotchi v2.0 baked $(date -u '+%Y-%m-%dT%H:%M:%SZ')" | sudo tee "$PI${SENTINEL_FILE}" > /dev/null
echo "  Sentinel: $SENTINEL_FILE"


# ═══════════════════════════════════════════════
# FULL VERIFICATION
# ═══════════════════════════════════════════════
echo ""
echo "============================================="
echo "=== FULL VERIFICATION ==="
echo "============================================="
echo ""
ERRORS=0

verify() {
    local label="$1"
    local path="$2"
    if [ -e "$path" ]; then
        echo "  OK: $label"
    else
        echo "  MISSING: $label ($path)"
        ERRORS=$((ERRORS+1))
    fi
}

echo "--- Hostname ---"
cat "$PI/etc/hostname"
grep '127.0.1.1' "$PI/etc/hosts" || echo "  WARNING: no 127.0.1.1 entry in hosts"

echo ""
echo "--- config.toml key settings ---"
if [ -f "$CFG" ]; then
    grep -i 'name\s*=' "$CFG" | head -5
    grep -i 'font' "$CFG" | head -5
    grep -i 'whitelist' "$CFG" | head -3
    grep -i 'bt-tether' "$CFG" | head -3
    grep -i 'png' "$CFG" | head -3
    # Check for wpa-sec duplicate
    WPA_SEC_COUNT=$(grep -c '^\[main\.plugins\.wpa-sec\]' "$CFG" 2>/dev/null || echo 0)
    if [ "$WPA_SEC_COUNT" -gt 1 ]; then
        echo "  BAD: $WPA_SEC_COUNT wpa-sec sections (should be 1)"
        ERRORS=$((ERRORS+1))
    else
        echo "  OK: wpa-sec section count = $WPA_SEC_COUNT"
    fi
fi

echo ""
echo "--- AO config overlay ---"
ls -la "$PI/etc/pwnagotchi/conf.d/angryoxide-v5.toml" 2>/dev/null || { echo "  MISSING"; ERRORS=$((ERRORS+1)); }
grep 'enabled = true' "$PI/etc/pwnagotchi/conf.d/angryoxide-v5.toml" 2>/dev/null | head -3
grep 'whitelist' "$PI/etc/pwnagotchi/conf.d/angryoxide-v5.toml" 2>/dev/null

echo ""
echo "--- Plugins ---"
verify "angryoxide.py"    "$PI/etc/pwnagotchi/custom-plugins/angryoxide.py"
verify "walkby.py"        "$PI/etc/pwnagotchi/custom-plugins/walkby.py"
verify "frame_padding.py" "$PI/etc/pwnagotchi/custom-plugins/frame_padding.py"
verify "stub_client.py"   "$PI/etc/pwnagotchi/custom-plugins/stub_client.py"

echo ""
echo "--- Plugin features (angryoxide.py) ---"
for marker in '_get_ip_display' '_ap_count' '_peers_patched' 'capture_prefix' 'blind' 'state_restore\|_save_state\|_load_state'; do
    if grep -qE "$marker" "$PI/etc/pwnagotchi/custom-plugins/angryoxide.py" 2>/dev/null; then
        echo "  OK: $marker found"
    else
        echo "  WARNING: $marker NOT found in angryoxide.py"
    fi
done

echo ""
echo "--- Bull faces ---"
echo "  Face count: $(ls "$PI/etc/pwnagotchi/custom-plugins/faces/"*.png 2>/dev/null | wc -l)"

echo ""
echo "--- Tools ---"
verify "apply-oxigotchi-patches.sh" "$PI/usr/local/bin/apply-oxigotchi-patches.sh"
verify "wifi-recovery.sh"           "$PI/usr/local/bin/wifi-recovery.sh"
verify "oxigotchi-splash.py"        "$PI/usr/local/bin/oxigotchi-splash.py"
verify "pwnoxide-mode"              "$PI/usr/local/bin/pwnoxide-mode"
verify "wlan_keepalive"             "$PI/usr/local/bin/wlan_keepalive"
verify "toggle-bt.sh"               "$PI/usr/local/bin/toggle-bt.sh"
verify "toggle-mode.sh"             "$PI/usr/local/bin/toggle-mode.sh"
verify "toggle-ao-pwn.sh"           "$PI/usr/local/bin/toggle-ao-pwn.sh"
verify "bt-pair.sh"                 "$PI/usr/local/bin/bt-pair.sh"
verify "bootlog.sh"                 "$PI/usr/local/bin/bootlog.sh"
file "$PI/usr/local/bin/wlan_keepalive" 2>/dev/null

echo ""
echo "--- CRLF check (should show NO matches) ---"
CRLF_BAD=0
for f in "$PI"/usr/local/bin/*.sh "$PI/usr/local/bin/pwnoxide-mode"; do
    if [ -f "$f" ] && grep -qlP '\r' "$f" 2>/dev/null; then
        echo "  BAD CRLF: $(basename "$f")"
        CRLF_BAD=$((CRLF_BAD+1))
    fi
done
if [ "$CRLF_BAD" -eq 0 ]; then
    echo "  OK: No CRLF found in any scripts"
else
    echo "  BAD: $CRLF_BAD files with CRLF"
    ERRORS=$((ERRORS+CRLF_BAD))
fi

echo ""
echo "--- Services (installed) ---"
for svc in oxigotchi-patches wifi-recovery resize-rootfs wlan-keepalive oxigotchi-splash emergency-ssh bootlog usb0-fallback watchdog; do
    if [ -f "$PI/etc/systemd/system/${svc}.service" ]; then
        PERMS=$(stat -c '%a' "$PI/etc/systemd/system/${svc}.service")
        if [ "$PERMS" = "644" ]; then
            echo "  OK: $svc.service (perms: $PERMS)"
        else
            echo "  BAD: $svc.service (perms: $PERMS, expected 644)"
            ERRORS=$((ERRORS+1))
        fi
    else
        echo "  MISSING: $svc.service"
        ERRORS=$((ERRORS+1))
    fi
done

echo ""
echo "--- Services (enabled via symlinks) ---"
echo "  multi-user.target.wants:"
ls "$PI/etc/systemd/system/multi-user.target.wants/" 2>/dev/null | sort
echo ""
echo "  sysinit.target.wants:"
ls "$PI/etc/systemd/system/sysinit.target.wants/" 2>/dev/null | sort

echo ""
echo "--- Services (disabled - should NOT be in multi-user.target.wants) ---"
for svc in ModemManager systemd-networkd usb0-ip rpi-usb-gadget-ics userconfig pi-helper rpi-eeprom-update; do
    if [ -f "$PI/etc/systemd/system/multi-user.target.wants/${svc}.service" ]; then
        echo "  STILL ENABLED (BAD): $svc"
        ERRORS=$((ERRORS+1))
    else
        echo "  Disabled OK: $svc"
    fi
done

echo ""
echo "--- Pwnagotchi drop-in ---"
verify "splash-delay.conf" "$PI/etc/systemd/system/pwnagotchi.service.d/pwnagotchi-splash-delay.conf"

echo ""
echo "--- NetworkManager connections ---"
ls -la "$NM_DIR/" 2>/dev/null
grep -A2 'ipv4' "$NM_DIR/USB Gadget (shared).nmconnection" 2>/dev/null

echo ""
echo "--- SSH ---"
echo "  SSH host keys: $(ls $PI/etc/ssh/ssh_host_*_key 2>/dev/null | wc -l)"
cat "$PI/etc/ssh/sshd_config.d/99-oxigotchi.conf" 2>/dev/null
echo "  authorized_keys lines: $(wc -l < "$PI/home/pi/.ssh/authorized_keys" 2>/dev/null || echo 0)"

echo ""
echo "--- Cloud-init ---"
test -f "$PI/etc/cloud/cloud-init.disabled" && echo "  Disabled: YES" || echo "  Disabled: NO"

echo ""
echo "--- Handshakes symlink ---"
ls -la "$PI/root/handshakes" 2>/dev/null || echo "  /root/handshakes not present"

echo ""
echo "--- Camera blacklist ---"
cat "$PI/etc/modprobe.d/blacklist-camera.conf" 2>/dev/null || echo "  MISSING"

echo ""
echo "--- Rootfs sentinel ---"
test -f "$PI/var/lib/.rootfs-expanded" && echo "  Present: YES" || echo "  Present: NO"

echo ""
echo "--- Bake sentinel ---"
cat "$PI${SENTINEL_FILE}" 2>/dev/null || echo "  MISSING"

echo ""
echo "--- Default target ---"
readlink "$PI/etc/systemd/system/default.target" 2>/dev/null

echo ""
echo "--- Stale files (should not exist) ---"
test -f "$PI/usr/local/bin/oxagotchi-splash.py" && { echo "  BAD: oxagotchi-splash.py still exists"; ERRORS=$((ERRORS+1)); } || echo "  OK: oxagotchi-splash.py removed"
test -f "$PI/etc/systemd/system/oxagotchi-splash.service" && { echo "  BAD: oxagotchi-splash.service still exists"; ERRORS=$((ERRORS+1)); } || echo "  OK: oxagotchi-splash.service removed"

echo ""
echo "--- site-packages helpers ---"
verify "stub_client.py in site-packages"   "$SITE_PKG/stub_client.py"
verify "frame_padding.py in site-packages" "$SITE_PKG/frame_padding.py"

echo ""
echo "--- Disk usage ---"
sudo du -sh "$PI/" 2>/dev/null

# ─── Unmount ───
echo ""
echo "=== Unmounting ==="
sync
sudo umount /mnt/piboot
sudo umount /mnt/piroot
sudo losetup -D

echo ""
echo "============================================="
if [ "$ERRORS" -gt 0 ]; then
    echo "=== IMAGE BAKED WITH $ERRORS ERRORS ==="
else
    echo "=== IMAGE FULLY BAKED — oxigotchi v2.0 ==="
fi
echo "============================================="
echo ""
echo "Image: $IMG"
echo "Errors: $ERRORS"
