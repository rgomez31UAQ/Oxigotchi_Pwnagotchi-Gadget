#!/bin/bash
echo "=== Oxigotchi Bluetooth Pairing ==="
echo "1. Enable BT tethering/hotspot on your phone"
echo "2. Scanning for devices..."
bluetoothctl power on
bluetoothctl agent on
bluetoothctl default-agent
bluetoothctl scan on &
SCAN_PID=$!
sleep 10
kill $SCAN_PID 2>/dev/null
echo ""
echo "Found devices:"
bluetoothctl devices
echo ""
read -p "Enter phone MAC (e.g. AA:BB:CC:DD:EE:FF): " MAC
read -p "Enter phone name (e.g. Xiaomi 13T): " NAME
echo "Pairing with $MAC..."
bluetoothctl pair "$MAC"
bluetoothctl trust "$MAC"
echo "Creating NM connection..."
nmcli connection add type bluetooth con-name "$NAME Network" bt-type panu ifname bnep0 addr "$MAC"
echo ""
echo "Done! Update /etc/systemd/system/bt-tether.service with new connection name if needed."
echo "Then single-tap PiSugar button to connect."
