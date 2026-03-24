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
