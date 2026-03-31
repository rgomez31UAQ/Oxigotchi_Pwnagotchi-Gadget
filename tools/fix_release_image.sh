#!/bin/bash
# Fix release image after partial bake: replace binary, finish remaining steps
set -euo pipefail
PI=/mnt/piroot
REPO=/mnt/c/msys64/home/user/oxigotchi

# 1. Replace binary with Mooooood build
echo '=== Replacing binary with Mooooood build ==='
sudo cp "$REPO/rust/target/aarch64-unknown-linux-gnu/release/oxigotchi" "$PI/usr/local/bin/rusty-oxigotchi"
sudo chmod +x "$PI/usr/local/bin/rusty-oxigotchi"
file "$PI/usr/local/bin/rusty-oxigotchi"

if strings "$PI/usr/local/bin/rusty-oxigotchi" | grep -q 'Mooooood'; then
    echo '  OK: Mooooood found in binary'
else
    echo '  WARNING: Mooooood NOT found in binary'
fi

# 2. Fix SSH - remove authorized_keys
echo '=== Fixing SSH ==='
sudo rm -rf "$PI/home/pi/.ssh" 2>/dev/null || true
sudo chmod 600 $PI/etc/ssh/ssh_host_*_key 2>/dev/null || true
sudo chmod 644 $PI/etc/ssh/ssh_host_*_key.pub 2>/dev/null || true
echo '  authorized_keys removed, key perms fixed'

# 3. NM config
echo '=== NetworkManager config ==='
NM_DIR="$PI/etc/NetworkManager/system-connections"
sudo mkdir -p "$NM_DIR"
cat > /tmp/nm-usb0.conf <<'NM_USB'
[connection]
id=USB Gadget
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
NM_USB
sudo cp /tmp/nm-usb0.conf "$NM_DIR/USB Gadget.nmconnection"
sudo chmod 600 "$NM_DIR/USB Gadget.nmconnection"
echo '  Dual-IP NM config installed'

# 4. System config
echo '=== System config ==='
sudo ln -sf /lib/systemd/system/multi-user.target "$PI/etc/systemd/system/default.target"
printf '#!/bin/bash\nexit 0\n' | sudo tee "$PI/etc/rc.local" > /dev/null
sudo chmod +x "$PI/etc/rc.local"
printf 'CONF_SWAPSIZE=100\nCONF_SWAPFILE=/var/swap\n' | sudo tee "$PI/etc/dphys-swapfile" > /dev/null
grep -q '/tmp' "$PI/etc/fstab" || echo 'tmpfs /tmp tmpfs defaults,noatime,nosuid,size=50m 0 0' | sudo tee -a "$PI/etc/fstab" > /dev/null
sudo rm -f "$PI/etc/systemd/system/timers.target.wants/apt-daily.timer" 2>/dev/null || true
sudo rm -f "$PI/etc/systemd/system/timers.target.wants/apt-daily-upgrade.timer" 2>/dev/null || true
sudo ln -sf /usr/share/zoneinfo/UTC "$PI/etc/localtime" 2>/dev/null || true

BOOT_CFG="$PI/boot/firmware/config.txt"
[ ! -f "$BOOT_CFG" ] && BOOT_CFG="/mnt/piboot/config.txt"
if [ -f "$BOOT_CFG" ]; then
    grep -q 'dtparam=watchdog=on' "$BOOT_CFG" || echo 'dtparam=watchdog=on' | sudo tee -a "$BOOT_CFG" > /dev/null
fi

sudo mkdir -p "$PI/etc/modprobe.d"
echo 'blacklist bcm2835_v4l2' | sudo tee "$PI/etc/modprobe.d/blacklist-camera.conf" > /dev/null

MODULES_CONF="$PI/etc/modules-load.d/modules.conf"
if [ -f "$MODULES_CONF" ]; then
    grep -q 'brcmfmac' "$MODULES_CONF" || echo 'brcmfmac' | sudo tee -a "$MODULES_CONF" > /dev/null
else
    sudo mkdir -p "$(dirname "$MODULES_CONF")"
    echo 'brcmfmac' | sudo tee "$MODULES_CONF" > /dev/null
fi

sudo mkdir -p "$PI/etc/cloud"
sudo touch "$PI/etc/cloud/cloud-init.disabled"
echo '  System config done'

# 5. Disable unwanted services
echo '=== Disable unwanted services ==='
for svc in ModemManager systemd-networkd usb0-ip rpi-usb-gadget-ics userconfig pi-helper rpi-eeprom-update; do
    sudo rm -f "$PI/etc/systemd/system/multi-user.target.wants/${svc}.service" 2>/dev/null || true
done
sudo rm -f "$PI/etc/systemd/network/10-usb0.network" 2>/dev/null || true
echo '  Done'

# 6. Handshakes dir
sudo mkdir -p "$PI/etc/oxigotchi/handshakes"

# 7. Clean bloat
echo '=== Clean bloat ==='
sudo rm -rf "$PI/root/.rustup" 2>/dev/null || true
sudo rm -rf "$PI/root/go" 2>/dev/null || true
sudo rm -f "$PI/home/pi/swapfile" 2>/dev/null || true
sudo rm -rf "$PI/home/pi/.vscode-server" 2>/dev/null || true
sudo rm -rf "$PI"/var/log/journal/* 2>/dev/null || true
sudo rm -rf "$PI"/var/cache/apt/archives/*.deb 2>/dev/null || true
sudo rm -f "$PI/home/pi/.bash_history" 2>/dev/null || true
sudo rm -f "$PI/root/.bash_history" 2>/dev/null || true
echo '  Done'

# 8. Sentinel
echo "oxigotchi v3.0 release $(date -u '+%Y-%m-%dT%H:%M:%SZ')" | sudo tee "$PI/.oxigotchi-baked" > /dev/null
sudo rm -f "$PI/var/lib/.rootfs-expanded" 2>/dev/null || true
echo '  Sentinel created, rootfs-expanded removed'

# 9. Quick verification
echo ''
echo '=== VERIFICATION ==='
echo "Binary: $(file "$PI/usr/local/bin/rusty-oxigotchi" | grep -o 'aarch64')"
echo "Hostname: $(cat "$PI/etc/hostname")"
echo "Plugins: $(ls "$PI"/etc/oxigotchi/plugins/*.lua 2>/dev/null | wc -l)"
echo "Faces: $(ls "$PI"/etc/oxigotchi/faces/*.png 2>/dev/null | wc -l)"
echo "Host keys: $(ls $PI/etc/ssh/ssh_host_*_key 2>/dev/null | wc -l)"
echo "authorized_keys: $(test -f "$PI/home/pi/.ssh/authorized_keys" && echo 'EXISTS (BAD)' || echo 'NONE (GOOD)')"
echo "resize sentinel: $(test -f "$PI/var/lib/.rootfs-expanded" && echo 'EXISTS (BAD)' || echo 'NONE (will expand)')"
echo "Bake sentinel: $(cat "$PI/.oxigotchi-baked")"
echo "Disk usage: $(sudo du -sh "$PI/" 2>/dev/null | cut -f1)"

echo ''
echo '=== READY TO UNMOUNT, SHRINK, AND ZIP ==='
