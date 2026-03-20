#!/bin/bash
set -e

sudo umount /mnt/piroot 2>/dev/null || true
sudo umount /mnt/piboot 2>/dev/null || true
sudo losetup -D 2>/dev/null || true
sudo losetup -fP /mnt/d/oxigotchi-v2.0.img
sudo mkdir -p /mnt/piboot /mnt/piroot
sudo mount /dev/loop0p1 /mnt/piboot
sudo mount /dev/loop0p2 /mnt/piroot
PI=/mnt/piroot

echo "=== 1. multi-user.target as default ==="
sudo ln -sf /lib/systemd/system/multi-user.target $PI/etc/systemd/system/default.target

echo "=== 2. SSH password auth ==="
sudo mkdir -p $PI/etc/ssh/sshd_config.d
printf 'PasswordAuthentication yes\nPermitRootLogin yes\nKbdInteractiveAuthentication yes\nUseDNS no\n' | sudo tee $PI/etc/ssh/sshd_config.d/99-oxigotchi.conf > /dev/null

echo "=== 3. SSH host keys ==="
sudo ssh-keygen -A -f $PI 2>&1
sudo chmod 600 $PI/etc/ssh/ssh_host_*_key
sudo chmod 644 $PI/etc/ssh/ssh_host_*_key.pub

echo "=== 4. Emergency SSH service ==="
cat > /tmp/emergency-ssh.service <<'UNIT'
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
UNIT
sudo cp /tmp/emergency-ssh.service $PI/etc/systemd/system/emergency-ssh.service
sudo ln -sf /etc/systemd/system/emergency-ssh.service $PI/etc/systemd/system/multi-user.target.wants/emergency-ssh.service

echo "=== 5. Disable cloud-init ==="
sudo touch $PI/etc/cloud/cloud-init.disabled

echo "=== 6. Disable unnecessary services ==="
sudo rm -f $PI/etc/systemd/system/multi-user.target.wants/ModemManager.service
sudo rm -f $PI/etc/systemd/system/multi-user.target.wants/systemd-networkd.service
sudo rm -f $PI/etc/systemd/system/multi-user.target.wants/usb0-ip.service
sudo rm -f $PI/etc/systemd/system/multi-user.target.wants/rpi-usb-gadget-ics.service
sudo rm -f $PI/etc/systemd/network/10-usb0.network 2>/dev/null

echo "=== 7. Neutralize rc.local ==="
printf '#!/bin/bash\nexit 0\n' | sudo tee $PI/etc/rc.local > /dev/null
sudo chmod +x $PI/etc/rc.local

echo "=== 7b. Resize rootfs (idempotent) ==="
growpart /dev/loop0 2 2>/dev/null || true
resize2fs /dev/loop0p2 2>/dev/null || true
sudo touch $PI/var/lib/.rootfs-expanded

echo "=== 8. Clean disk bloat ==="
sudo rm -rf $PI/root/.rustup
sudo rm -rf $PI/root/go
sudo rm -f $PI/home/pi/swapfile
sudo rm -f $PI/home/pi/oxigotchi-backup.tar
sudo rm -rf "$PI/home/pi/.vscode-server/data/CachedExtensionVSIXs"
sudo rm -f $PI/etc/pwnagotchi/handshakes/.kismet

echo "=== 9. Fix NM USB Gadget ==="
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
address1=10.0.0.2/24,192.168.137.2/24
gateway=10.0.0.1
dns=8.8.8.8;1.1.1.1;
method=manual

[ipv6]
addr-gen-mode=default
method=disabled

[proxy]
NM1
sudo cp /tmp/nm-shared.conf "$PI/etc/NetworkManager/system-connections/USB Gadget (shared).nmconnection"
sudo chmod 600 "$PI/etc/NetworkManager/system-connections/USB Gadget (shared).nmconnection"

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
sudo cp /tmp/nm-client.conf "$PI/etc/NetworkManager/system-connections/USB Gadget (client).nmconnection"
sudo chmod 600 "$PI/etc/NetworkManager/system-connections/USB Gadget (client).nmconnection"

echo "=== 10. Bootlog script ==="
cat > /tmp/bootlog.sh <<'BLOG'
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
BLOG
sudo cp /tmp/bootlog.sh $PI/usr/local/bin/bootlog.sh
sudo chmod +x $PI/usr/local/bin/bootlog.sh

echo "=== 11. Bootlog service ==="
cat > /tmp/bootlog.service <<'BLS'
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
BLS
sudo cp /tmp/bootlog.service $PI/etc/systemd/system/bootlog.service

echo "=== 12. Fix splash service ==="
cat > /tmp/splash.service <<'SPL'
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
SPL
sudo cp /tmp/splash.service $PI/etc/systemd/system/oxigotchi-splash.service
sudo rm -f $PI/etc/systemd/system/sysinit.target.wants/oxagotchi-splash.service 2>/dev/null
sudo rm -f $PI/etc/systemd/system/sysinit.target.wants/oxigotchi-splash.service 2>/dev/null

echo "=== 13. Pwnagotchi drop-in ==="
cat > /tmp/splash-delay.conf <<'DD'
[Unit]
After=oxigotchi-splash.service

[Service]
ExecStartPre=/bin/sleep 3
DD
sudo cp /tmp/splash-delay.conf $PI/etc/systemd/system/pwnagotchi.service.d/pwnagotchi-splash-delay.conf

echo "=== 14. Cleanup stale ==="
sudo rm -f $PI/usr/local/bin/oxagotchi-splash.py 2>/dev/null
sudo rm -f $PI/etc/systemd/system/oxagotchi-splash.service 2>/dev/null

echo "=== 15. Swap config ==="
printf 'CONF_SWAPSIZE=100\nCONF_SWAPFILE=/var/swap\n' | sudo tee $PI/etc/dphys-swapfile > /dev/null

echo "=== 16. Fix pi user home ownership ==="
sudo chown -R 1000:1000 $PI/home/pi/.ssh 2>/dev/null

echo "=== 17. Ensure /tmp is tmpfs (prevent SD wear) ==="
if ! grep -q '/tmp' $PI/etc/fstab; then
    echo 'tmpfs /tmp tmpfs defaults,noatime,nosuid,size=50m 0 0' | sudo tee -a $PI/etc/fstab > /dev/null
fi

echo "=== 18. Disable apt daily timers (prevent boot slowdown) ==="
sudo rm -f $PI/etc/systemd/system/timers.target.wants/apt-daily.timer 2>/dev/null
sudo rm -f $PI/etc/systemd/system/timers.target.wants/apt-daily-upgrade.timer 2>/dev/null

echo "=== 19. Set timezone ==="
sudo ln -sf /usr/share/zoneinfo/Europe/Helsinki $PI/etc/localtime 2>/dev/null

echo "=== 20. Disable userconfig service (first-boot wizard, hangs without console) ==="
sudo rm -f $PI/etc/systemd/system/multi-user.target.wants/userconfig.service

echo "=== 21. Disable pi-helper (unnecessary) ==="
sudo rm -f $PI/etc/systemd/system/multi-user.target.wants/pi-helper.service

echo "=== 22a. Enable angryoxide (AO mode default) ==="
sudo sed -i '/^\[main\.plugins\.angryoxide\]/,/^\[/{s/^enabled = false/enabled = true/}' $PI/etc/pwnagotchi/config.toml 2>/dev/null || true

echo "=== 22b. Fix ui.font.name in config.toml ==="
# Handle both flat key (ui.font.name = ...) and section-based (name = ... under [ui.font])
sudo sed -i 's/^\(ui\.font\.name\s*=\s*\)"oxigotchi"/\1"DejaVuSansMono"/' $PI/etc/pwnagotchi/config.toml 2>/dev/null || true
sudo sed -i '/^\[ui\.font\]/,/^\[/{s/^\(name\s*=\s*\)"oxigotchi"/\1"DejaVuSansMono"/}' $PI/etc/pwnagotchi/config.toml 2>/dev/null || true

echo "=== 23. Ensure correct hostname ==="
echo "oxigotchi" | sudo tee $PI/etc/hostname > /dev/null
sudo sed -i '/^127\.0\.1\.1[[:space:]]/s/[[:space:]].*$/\toxigotchi/' $PI/etc/hosts

echo "=== 24. Disable rpi-eeprom-update (not needed on Zero 2W) ==="
sudo rm -f $PI/etc/systemd/system/multi-user.target.wants/rpi-eeprom-update.service

echo "=== 25. Add RNDIS IP auto-set to bootlog self-heal ==="
# Already in bootlog.sh, but also add a simple ifup for usb0 in case NM fails
cat > /tmp/usb0-fallback.sh <<'USBF'
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
USBF
sudo cp /tmp/usb0-fallback.sh $PI/usr/local/bin/usb0-fallback.sh
sudo chmod +x $PI/usr/local/bin/usb0-fallback.sh
cat > /tmp/usb0-fallback.service <<'UFB'
[Unit]
Description=USB0 IP fallback
After=NetworkManager.service

[Service]
Type=oneshot
ExecStart=/usr/local/bin/usb0-fallback.sh
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
UFB
sudo cp /tmp/usb0-fallback.service $PI/etc/systemd/system/usb0-fallback.service
sudo ln -sf /etc/systemd/system/usb0-fallback.service $PI/etc/systemd/system/multi-user.target.wants/usb0-fallback.service

echo "=== 26. Clean journal logs from old boots ==="
sudo rm -rf $PI/var/log/journal/* 2>/dev/null

echo ""
echo "==========================================="
echo "=== VERIFICATION ==="
echo "==========================================="
echo "Default: $(readlink $PI/etc/systemd/system/default.target)"
echo "SSH keys: $(ls $PI/etc/ssh/ssh_host_*_key 2>/dev/null | wc -l)"
cat $PI/etc/ssh/sshd_config.d/99-oxigotchi.conf
echo "Cloud-init disabled: $(test -f $PI/etc/cloud/cloud-init.disabled && echo YES)"
echo ""
echo "Enabled services:"
ls $PI/etc/systemd/system/multi-user.target.wants/
echo ""
echo "Disk:"
sudo du -sh $PI/

sync
sudo umount /mnt/piboot /mnt/piroot
sudo losetup -D
echo ""
echo "=== IMAGE FULLY BAKED ==="
