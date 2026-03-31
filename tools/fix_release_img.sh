#!/bin/bash
# Post-bake fixes for release image — removes personal data and pwnagotchi remnants
# the bake script missed from the base image's multi-user.target.wants
set -euo pipefail

IMG="/mnt/d/oxigotchi-release.img"
PI=/mnt/piroot

# Clean previous mounts
sudo umount "$PI" 2>/dev/null || true
sudo losetup -D 2>/dev/null || true
sleep 1

# Mount
sudo losetup -fP "$IMG"
sleep 1
LOOPDEV=$(losetup -j "$IMG" | head -1 | cut -d: -f1)
echo "Loop: $LOOPDEV"
sudo partprobe "$LOOPDEV"
sleep 1
sudo mkdir -p "$PI"
sudo mount "${LOOPDEV}p2" "$PI"
echo "Mounted $IMG"

echo ""
echo "=== Fix 1: Remove personal NM connections ==="
for f in "$PI"/etc/NetworkManager/system-connections/*.nmconnection; do
    name=$(basename "$f")
    if [ "$name" != "USB Gadget.nmconnection" ]; then
        sudo rm -f "$f"
        echo "  Removed: $name"
    fi
done

echo ""
echo "=== Fix 2: Clean pwnagotchi remnant symlinks from multi-user.target.wants ==="
for svc in oxigotchi-splash pwngrid-peer sshswitch pisugar-server epd-startup; do
    if [ -L "$PI/etc/systemd/system/multi-user.target.wants/${svc}.service" ] || \
       [ -f "$PI/etc/systemd/system/multi-user.target.wants/${svc}.service" ]; then
        sudo rm -f "$PI/etc/systemd/system/multi-user.target.wants/${svc}.service"
        echo "  Removed from wants: $svc"
    fi
done

echo ""
echo "=== Fix 3: Mask additional pwnagotchi services ==="
for svc in pwngrid-peer sshswitch; do
    sudo ln -sf /dev/null "$PI/etc/systemd/system/${svc}.service" 2>/dev/null || true
    echo "  Masked: $svc"
done

echo ""
echo "=== Verification ==="
echo "--- NM connections ---"
ls "$PI/etc/NetworkManager/system-connections/"
echo "--- multi-user.target.wants ---"
ls "$PI/etc/systemd/system/multi-user.target.wants/" | sort
echo "--- config rage_level ---"
grep rage_level "$PI/etc/oxigotchi/config.toml"
echo "--- masked services ---"
for svc in pwnagotchi bettercap pwngrid-peer sshswitch; do
    target=$(readlink "$PI/etc/systemd/system/${svc}.service" 2>/dev/null || echo "NOT MASKED")
    echo "  $svc -> $target"
done

sync
sudo umount "$PI"
sudo losetup -D
echo ""
echo "=== Image patched and unmounted ==="
