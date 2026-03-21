#!/bin/bash
# bake_v2.sh — Re-bake oxigotchi-v2.0.img with ALL sprint fixes
# Run inside WSL: sudo bash /mnt/c/msys64/home/user/oxigotchi/tools/bake_v2.sh
set -euo pipefail

echo "============================================="
echo "=== Oxigotchi v2.0 Image Bake — Full Sprint ==="
echo "============================================="
echo ""

# ─── Setup ───
IMG=/mnt/d/oxigotchi-v2.0.img
REPO=/mnt/c/msys64/home/user/oxigotchi

# Cross-compile wlan_keepalive
echo "=== 0a. Cross-compile wlan_keepalive ==="
aarch64-linux-gnu-gcc -O2 -static -o /tmp/wlan_keepalive "$REPO/tools/wlan_keepalive.c"
file /tmp/wlan_keepalive
echo "  Cross-compile done"

# Clean up any previous mounts
echo "=== 0b. Cleanup previous mounts ==="
sudo umount /mnt/piroot 2>/dev/null || true
sudo umount /mnt/piboot 2>/dev/null || true
sudo losetup -D 2>/dev/null || true
sleep 1

# Mount image
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
# stub_client also goes to site-packages (imported by patched agent.py)
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
sudo cp "$REPO/tools/apply_patches.sh" "$PI/usr/local/bin/apply-oxigotchi-patches.sh"
sudo cp "$REPO/tools/wifi-recovery.sh" "$PI/usr/local/bin/wifi-recovery.sh"
sudo cp "$REPO/tools/oxigotchi-splash.py" "$PI/usr/local/bin/oxigotchi-splash.py"
sudo cp "$REPO/tools/pwnoxide-mode.sh" "$PI/usr/local/bin/pwnoxide-mode"
# Fix CRLF on all shell scripts
for f in $PI/usr/local/bin/apply-oxigotchi-patches.sh \
         $PI/usr/local/bin/wifi-recovery.sh \
         $PI/usr/local/bin/pwnoxide-mode; do
    sudo sed -i 's/\r$//' "$f"
    sudo chmod +x "$f"
done
sudo chmod +x "$PI/usr/local/bin/oxigotchi-splash.py"
echo "  All tools installed + CRLF fixed"

# Install wlan_keepalive binary
sudo cp /tmp/wlan_keepalive "$PI/usr/local/bin/wlan_keepalive"
sudo chmod +x "$PI/usr/local/bin/wlan_keepalive"
echo "  wlan_keepalive binary installed"

# ─── 6. Services ───
echo ""
echo "=== 6. Install services ==="
# Copy service files from repo
sudo cp "$REPO/services/oxigotchi-patches.service" "$PI/etc/systemd/system/oxigotchi-patches.service"
sudo cp "$REPO/services/wifi-recovery.service" "$PI/etc/systemd/system/wifi-recovery.service"
sudo cp "$REPO/services/resize-rootfs.service" "$PI/etc/systemd/system/resize-rootfs.service"

# wlan-keepalive service — use the native binary version instead of tcpdump
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

# Splash service (corrected name from oxagotchi to oxigotchi)
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
# Remove old splash symlink if wrong name
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

# Bootlog service + script
cat > /tmp/bootlog.sh <<'BLOGSH'
#!/bin/bash
sleep 3
if mountpoint -q /boot/firmware; then
    LOG=/boot/firmware/bootlog.txt
else
    LOG=/var/log/bootlog.txt
fi
exec >> $LOG 2>&1
echo "=== Boot $(date) ==="
echo "Uptime: $(uptime)"
echo "--- Failed Services ---"
systemctl list-units --failed
echo "--- SSH ---"
systemctl status ssh
echo "--- Emergency SSH ---"
systemctl status emergency-ssh
echo "--- SSH journal ---"
journalctl -u ssh -u emergency-ssh --no-pager -n 30
echo "--- Pwnagotchi ---"
systemctl status pwnagotchi
echo "--- Network ---"
ip addr
echo "--- Listening ports ---"
ss -tlnp
echo "--- Disk ---"
df -h
echo "=== End ==="

# Self-heal SSH
if ! ss -tln | grep -q ":22 "; then
    ssh-keygen -A
    systemctl restart ssh || systemctl restart emergency-ssh
    echo "SSH healed at $(date)"
fi
BLOGSH
sudo cp /tmp/bootlog.sh "$PI/usr/local/bin/bootlog.sh"
sudo sed -i 's/\r$//' "$PI/usr/local/bin/bootlog.sh"
sudo chmod +x "$PI/usr/local/bin/bootlog.sh"

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
sudo chmod 644 "$PI/etc/systemd/system/oxigotchi-patches.service"
sudo chmod 644 "$PI/etc/systemd/system/wifi-recovery.service"
sudo chmod 644 "$PI/etc/systemd/system/resize-rootfs.service"
sudo chmod 644 "$PI/etc/systemd/system/wlan-keepalive.service"
sudo chmod 644 "$PI/etc/systemd/system/oxigotchi-splash.service"
sudo chmod 644 "$PI/etc/systemd/system/emergency-ssh.service"
sudo chmod 644 "$PI/etc/systemd/system/bootlog.service"
sudo chmod 644 "$PI/etc/systemd/system/usb0-fallback.service"
sudo chmod 644 "$PI/etc/systemd/system/watchdog.service"
sudo chmod 644 "$PI/etc/systemd/system/pwnagotchi.service.d/pwnagotchi-splash-delay.conf"
echo "  All services installed, chmod 644 applied"

# ─── 7. Fix config.toml ───
echo ""
echo "=== 7. Fix config.toml ==="
CFG="$PI/etc/pwnagotchi/config.toml"
if [ -f "$CFG" ]; then
    # main.name = "oxigotchi"
    sudo sed -i 's/^\(main\.name\s*=\s*\)"[^"]*"/\1"oxigotchi"/' "$CFG"
    sudo sed -i '/^\[main\]/,/^\[/{s/^\(name\s*=\s*\)"[^"]*"/\1"oxigotchi"/}' "$CFG"

    # font = "DejaVuSansMono"
    sudo sed -i 's/^\(ui\.font\.name\s*=\s*\)"[^"]*"/\1"DejaVuSansMono"/' "$CFG"
    sudo sed -i '/^\[ui\.font\]/,/^\[/{s/^\(name\s*=\s*\)"[^"]*"/\1"DejaVuSansMono"/}' "$CFG"

    # AO enabled=true
    sudo sed -i '/^\[main\.plugins\.angryoxide\]/,/^\[/{s/^enabled = false/enabled = true/}' "$CFG"

    # Whitelist
    sudo sed -i 's/^\(main\.whitelist\s*=\s*\).*/\1["Alpha", "Alpha 5G"]/' "$CFG"
    sudo sed -i '/^\[main\]/,/^\[/{s/^\(whitelist\s*=\s*\).*/\1["Alpha", "Alpha 5G"]/}' "$CFG"

    # bt-tether disabled
    sudo sed -i '/^\[main\.plugins\.bt-tether\]/,/^\[/{s/^\(enabled\s*=\s*\)true/\1false/}' "$CFG"

    # ui.faces.png = true
    sudo sed -i '/^\[ui\.faces\]/,/^\[/{s/^\(png\s*=\s*\)false/\1true/}' "$CFG"

    echo "  config.toml patched"
else
    echo "  WARNING: config.toml not found at $CFG"
fi

# ─── 8. Hostname ───
echo ""
echo "=== 8. Fix hostname ==="
echo "oxigotchi" | sudo tee "$PI/etc/hostname" > /dev/null
sudo sed -i '/^127\.0\.1\.1[[:space:]]/s/[[:space:]].*$/\toxigotchi/' "$PI/etc/hosts"
# Also add the entry if it doesn't exist
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
# If /root/handshakes exists as a real dir, merge and symlink
if [ -d "$AODIR" ] && [ ! -L "$AODIR" ]; then
    # Move any files from /root/handshakes to canonical location
    sudo cp -n "$AODIR"/* "$HSDIR/" 2>/dev/null || true
    sudo rm -rf "$AODIR"
fi
# Remove existing symlink if wrong target
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
# Also remove systemd-networkd usb0 network config
sudo rm -f "$PI/etc/systemd/network/10-usb0.network" 2>/dev/null

# ─── 14. Enable services ───
echo ""
echo "=== 14. Enable services ==="
# multi-user.target.wants
sudo mkdir -p "$PI/etc/systemd/system/multi-user.target.wants"
for svc in wlan-keepalive wifi-recovery bootlog emergency-ssh usb0-fallback resize-rootfs watchdog; do
    sudo ln -sf "/etc/systemd/system/${svc}.service" "$PI/etc/systemd/system/multi-user.target.wants/${svc}.service"
    echo "  Enabled: $svc"
done

# sysinit.target.wants for splash
sudo mkdir -p "$PI/etc/systemd/system/sysinit.target.wants"
sudo ln -sf /etc/systemd/system/oxigotchi-splash.service "$PI/etc/systemd/system/sysinit.target.wants/oxigotchi-splash.service"
echo "  Enabled: oxigotchi-splash (sysinit)"

# ─── 15. SSH setup ───
echo ""
echo "=== 15. SSH configuration ==="
# SSH password auth config
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
PUBKEY="ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIANe4Gwnsedd4fjT6CGTqg4KpOh9oHWOiYY8WJxelSLv userind@gmail.com"
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

# ─── 20. CRLF fix ALL .sh files in /usr/local/bin ───
echo ""
echo "=== 20. CRLF fix all shell scripts ==="
for f in "$PI"/usr/local/bin/*.sh; do
    if [ -f "$f" ]; then
        sudo sed -i 's/\r$//' "$f"
    fi
done
echo "  All .sh files in /usr/local/bin CRLF-fixed"

# ═══════════════════════════════════════════════
# FULL VERIFICATION
# ═══════════════════════════════════════════════
echo ""
echo "============================================="
echo "=== FULL VERIFICATION ==="
echo "============================================="
echo ""

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
fi

echo ""
echo "--- AO config overlay ---"
ls -la "$PI/etc/pwnagotchi/conf.d/angryoxide-v5.toml" 2>/dev/null || echo "  MISSING"
grep 'enabled = true' "$PI/etc/pwnagotchi/conf.d/angryoxide-v5.toml" 2>/dev/null | head -3
grep 'whitelist' "$PI/etc/pwnagotchi/conf.d/angryoxide-v5.toml" 2>/dev/null
grep 'rate' "$PI/etc/pwnagotchi/conf.d/angryoxide-v5.toml" 2>/dev/null || echo "  (rate not in overlay; default=1 in plugin code)"

echo ""
echo "--- Plugins ---"
ls -la "$PI/etc/pwnagotchi/custom-plugins/"*.py 2>/dev/null

echo ""
echo "--- Bull faces ---"
echo "  Face count: $(ls "$PI/etc/pwnagotchi/custom-plugins/faces/"*.png 2>/dev/null | wc -l)"

echo ""
echo "--- Tools ---"
ls -la "$PI/usr/local/bin/apply-oxigotchi-patches.sh" 2>/dev/null || echo "  MISSING: apply-oxigotchi-patches.sh"
ls -la "$PI/usr/local/bin/wifi-recovery.sh" 2>/dev/null || echo "  MISSING: wifi-recovery.sh"
ls -la "$PI/usr/local/bin/oxigotchi-splash.py" 2>/dev/null || echo "  MISSING: oxigotchi-splash.py"
ls -la "$PI/usr/local/bin/pwnoxide-mode" 2>/dev/null || echo "  MISSING: pwnoxide-mode"
ls -la "$PI/usr/local/bin/wlan_keepalive" 2>/dev/null || echo "  MISSING: wlan_keepalive"
file "$PI/usr/local/bin/wlan_keepalive" 2>/dev/null

echo ""
echo "--- Services (installed) ---"
for svc in oxigotchi-patches wifi-recovery resize-rootfs wlan-keepalive oxigotchi-splash emergency-ssh bootlog usb0-fallback watchdog; do
    if [ -f "$PI/etc/systemd/system/${svc}.service" ]; then
        PERMS=$(stat -c '%a' "$PI/etc/systemd/system/${svc}.service")
        echo "  $svc.service (perms: $PERMS)"
    else
        echo "  MISSING: $svc.service"
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
    else
        echo "  Disabled OK: $svc"
    fi
done

echo ""
echo "--- Pwnagotchi drop-in ---"
ls -la "$PI/etc/systemd/system/pwnagotchi.service.d/" 2>/dev/null
cat "$PI/etc/systemd/system/pwnagotchi.service.d/pwnagotchi-splash-delay.conf" 2>/dev/null

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
echo "--- Default target ---"
readlink "$PI/etc/systemd/system/default.target" 2>/dev/null

echo ""
echo "--- Stale files (should not exist) ---"
test -f "$PI/usr/local/bin/oxagotchi-splash.py" && echo "  BAD: oxagotchi-splash.py still exists" || echo "  OK: oxagotchi-splash.py removed"
test -f "$PI/etc/systemd/system/oxagotchi-splash.service" && echo "  BAD: oxagotchi-splash.service still exists" || echo "  OK: oxagotchi-splash.service removed"

echo ""
echo "--- Disk usage ---"
sudo du -sh "$PI/" 2>/dev/null

echo ""
echo "--- site-packages helpers ---"
ls -la "$SITE_PKG/stub_client.py" 2>/dev/null || echo "  MISSING: stub_client.py in site-packages"
ls -la "$SITE_PKG/frame_padding.py" 2>/dev/null || echo "  MISSING: frame_padding.py in site-packages"

# ─── Unmount ───
echo ""
echo "=== Unmounting ==="
sync
sudo umount /mnt/piboot
sudo umount /mnt/piroot
sudo losetup -D
echo ""
echo "============================================="
echo "=== IMAGE FULLY BAKED — oxigotchi v2.0 ==="
echo "============================================="
