#!/bin/bash
# bake_v3.sh — Bake Rusty Oxigotchi v3.0 image
# Adds the Rust daemon on top of a v2 base image.
# Run inside WSL: sudo bash /path/to/oxigotchi/tools/bake_v3.sh
set -euo pipefail

echo "============================================="
echo "=== Oxigotchi v3.0 Image Bake — Rusty     ==="
echo "============================================="
echo ""

# ─── Setup ───
IMG=/mnt/d/oxigotchi-v3.0.img
REPO=/path/to/oxigotchi
BINARY="$REPO/rust/target/aarch64-unknown-linux-gnu/release/oxigotchi"

if [ ! -f "$IMG" ]; then
    echo "ERROR: Image not found at $IMG"
    exit 1
fi

if [ ! -f "$BINARY" ]; then
    echo "ERROR: Rust binary not found. Cross-compile first:"
    echo "  cd $REPO/rust && cargo build --release --target aarch64-unknown-linux-gnu"
    exit 1
fi

# ─── 0. Run v2 bake first ───
echo "=== 0. Running v2 bake first ==="
# Swap IMG path for v2 bake
sed "s|/mnt/d/oxigotchi-v2.0.img|$IMG|" "$REPO/tools/bake_v2.sh" > /tmp/bake_v2_on_v3.sh
bash /tmp/bake_v2_on_v3.sh || true
echo ""

# ─── Re-mount (v2 bake unmounts at end) ───
echo "=== Re-mounting image ==="
sudo umount /mnt/piroot 2>/dev/null || true
sudo umount /mnt/piboot 2>/dev/null || true
sudo losetup -D 2>/dev/null || true
sleep 1
sudo losetup -fP "$IMG"
LOOPDEV=$(losetup -j "$IMG" | head -1 | cut -d: -f1)
sudo mkdir -p /mnt/piboot /mnt/piroot
sudo mount "${LOOPDEV}p2" /mnt/piroot
sudo mount "${LOOPDEV}p1" /mnt/piboot
PI=/mnt/piroot
echo "Image mounted at $PI"

# ─── V3.1: Install Rust binary ───
echo ""
echo "=== V3.1. Install Rust binary ==="
sudo cp "$BINARY" "$PI/usr/local/bin/rusty-oxigotchi"
sudo chmod +x "$PI/usr/local/bin/rusty-oxigotchi"
echo "  Binary installed ($(ls -lh "$BINARY" | awk '{print $5}'))"

# ─── V3.2: Install Lua plugins ───
echo ""
echo "=== V3.2. Install Lua plugins ==="
sudo mkdir -p "$PI/etc/oxigotchi/plugins"
for p in "$REPO/rust/plugins/"*.lua; do
    sudo cp "$p" "$PI/etc/oxigotchi/plugins/$(basename "$p")"
    echo "  Copied $(basename "$p")"
done

# ─── V3.3: Create oxigotchi config dir ───
echo ""
echo "=== V3.3. Create config directory ==="
sudo mkdir -p "$PI/etc/oxigotchi"
sudo mkdir -p "$PI/var/lib/oxigotchi"
sudo mkdir -p "$PI/tmp/ao_captures"
echo "  /etc/oxigotchi/, /var/lib/oxigotchi/, /tmp/ao_captures/ created"

# ─── V3.4: Install rusty-oxigotchi systemd service ───
echo ""
echo "=== V3.4. Install rusty-oxigotchi service ==="
cat > /tmp/rusty-oxigotchi.service <<'RUSTSVC'
[Unit]
Description=Rusty Oxigotchi - WiFi capture daemon
After=network.target NetworkManager.service
Wants=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/rusty-oxigotchi
Restart=on-failure
RestartSec=5
StartLimitIntervalSec=300
StartLimitBurst=5
Environment=RUST_LOG=info
StandardOutput=journal
StandardError=journal
Nice=-5

[Install]
WantedBy=multi-user.target
RUSTSVC
sudo cp /tmp/rusty-oxigotchi.service "$PI/etc/systemd/system/rusty-oxigotchi.service"
sudo chmod 644 "$PI/etc/systemd/system/rusty-oxigotchi.service"

# Enable rusty-oxigotchi
sudo mkdir -p "$PI/etc/systemd/system/multi-user.target.wants"
sudo ln -sf /etc/systemd/system/rusty-oxigotchi.service \
    "$PI/etc/systemd/system/multi-user.target.wants/rusty-oxigotchi.service"
echo "  rusty-oxigotchi.service installed and enabled"

# ─── V3.5: Disable legacy Python services ───
echo ""
echo "=== V3.5. Disable legacy services ==="
# Disable pwnagotchi and bettercap — Rusty replaces both
for svc in pwnagotchi bettercap; do
    sudo rm -f "$PI/etc/systemd/system/multi-user.target.wants/${svc}.service" 2>/dev/null
    echo "  Disabled: $svc"
done
# Also disable services that conflict with Rusty's self-healing
for svc in wifi-recovery wlan-keepalive; do
    sudo rm -f "$PI/etc/systemd/system/multi-user.target.wants/${svc}.service" 2>/dev/null
    echo "  Disabled: $svc (handled by Rusty daemon)"
done

# ─── V3.6: Install v5 firmware ───
echo ""
echo "=== V3.6. Install v5 firmware ==="
FW_SRC="/path/to/brcmfmac43436-sdio.v5.bin"
FW_DST="$PI/lib/firmware/brcm/brcmfmac43436-sdio.bin"
if [ -f "$FW_SRC" ]; then
    # Backup original
    if [ -f "$FW_DST" ] && [ ! -f "${FW_DST}.orig" ]; then
        sudo cp "$FW_DST" "${FW_DST}.orig"
    fi
    sudo cp "$FW_SRC" "$FW_DST"
    echo "  v5 firmware installed ($(md5sum "$FW_SRC" | cut -d' ' -f1))"
else
    echo "  WARNING: v5 firmware not found at $FW_SRC"
fi

# ─── V3.7: Remove first-boot sentinel (trigger migration on first boot) ───
echo ""
echo "=== V3.7. Clean first-boot state ==="
sudo rm -f "$PI/var/lib/.rusty-first-boot" 2>/dev/null
sudo rm -f "$PI/var/lib/oxigotchi/state.json" 2>/dev/null
echo "  First-boot sentinel removed (migration will run on first boot)"

# ─── V3.8: Install test tools ───
echo ""
echo "=== V3.8. Install test tools ==="
sudo cp "$REPO/tools/test_sdio_ramrw.py" "$PI/usr/local/bin/test_sdio_ramrw.py" 2>/dev/null || true
echo "  SDIO RAMRW test tool installed"

# ─── V3.9: Set modprobe options for stability ───
echo ""
echo "=== V3.9. Modprobe options ==="
echo "options brcmfmac roamoff=1" | sudo tee "$PI/etc/modprobe.d/brcmfmac-stable.conf" > /dev/null
echo "  brcmfmac roamoff=1 set"

# ─── Cleanup ───
echo ""
echo "=== Cleanup ==="
# Truncate logs
sudo truncate -s 0 "$PI/var/log/syslog" 2>/dev/null || true
sudo truncate -s 0 "$PI/var/log/daemon.log" 2>/dev/null || true
sudo truncate -s 0 "$PI/var/log/kern.log" 2>/dev/null || true
sudo rm -rf "$PI/var/log/journal/"* 2>/dev/null || true
# Clear tmp
sudo rm -rf "$PI/tmp/"* 2>/dev/null || true
# Clear bash history
sudo truncate -s 0 "$PI/home/pi/.bash_history" 2>/dev/null || true
sudo truncate -s 0 "$PI/root/.bash_history" 2>/dev/null || true
echo "  Logs and history cleared"

# ─── Unmount ───
echo ""
echo "=== Unmounting ==="
sync
sudo umount /mnt/piroot
sudo umount /mnt/piboot
sudo losetup -D
echo "  Image unmounted"

echo ""
echo "============================================="
echo "=== Oxigotchi v3.0 Image Ready!           ==="
echo "=== Flash with: balenaEtcher -> $IMG"
echo "============================================="
