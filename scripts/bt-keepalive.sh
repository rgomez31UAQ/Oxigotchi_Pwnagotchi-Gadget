#!/bin/bash
# bt-keepalive.sh — Reconnect Bluetooth PAN tethering.
# Reads phone_mac from /etc/oxigotchi/config.toml (no hardcoded devices).
# Called by bt-keepalive.timer every 30s.

CONFIG="/etc/oxigotchi/config.toml"

# Extract phone_mac from config (handles quotes and whitespace)
PHONE_MAC=$(grep -oP 'phone_mac\s*=\s*"\K[^"]+' "$CONFIG" 2>/dev/null)

if [ -z "$PHONE_MAC" ]; then
    exit 0  # No phone configured, nothing to do
fi

# Extract connection name (optional, falls back to phone name)
CONN_NAME=$(grep -oP 'connection_name\s*=\s*"\K[^"]+' "$CONFIG" 2>/dev/null)
if [ -z "$CONN_NAME" ]; then
    CONN_NAME=$(grep -oP 'phone_name\s*=\s*"\K[^"]+' "$CONFIG" 2>/dev/null)
fi

# Only attempt if BT is enabled in config
BT_ENABLED=$(grep -oP 'enabled\s*=\s*\K\w+' "$CONFIG" 2>/dev/null | tail -1)
if [ "$BT_ENABLED" != "true" ]; then
    exit 0
fi

bluetoothctl connect "$PHONE_MAC" 2>/dev/null
sleep 2
if [ -n "$CONN_NAME" ]; then
    nmcli connection up "$CONN_NAME" 2>/dev/null || true
fi
