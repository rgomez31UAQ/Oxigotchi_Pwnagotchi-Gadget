#!/bin/bash
# Show shutdown face on e-ink display, then power off.
# Called by PiSugar soft_poweroff_shell (power button or battery protect).
#
# Boot-loop guard: if battery is low AND charging (USB-C plugged in),
# skip shutdown — the Pi just auto-started to charge, don't kill it.

PISUGAR="127.0.0.1 8423"

BATTERY=$(echo "get battery" | nc -q 1 $PISUGAR 2>/dev/null | grep -oP '[\d.]+' | head -1)
CHARGING=$(echo "get battery_power_plugged" | nc -q 1 $PISUGAR 2>/dev/null)

if echo "$CHARGING" | grep -q "true"; then
    BAT_INT=${BATTERY%.*}
    BAT_INT=${BAT_INT:-0}
    if [ "$BAT_INT" -lt 10 ] 2>/dev/null; then
        echo "Charging at ${BAT_INT}% — skipping shutdown" > /tmp/.pwnagotchi-button-msg
        exit 0
    fi
fi

echo "Shutting down..." > /tmp/.pwnagotchi-button-msg

systemctl stop pwnagotchi 2>/dev/null
systemctl stop rusty-oxigotchi 2>/dev/null
sleep 2

/home/pi/.pwn/bin/python3 /usr/local/bin/epd-shutdown.py 2>/dev/null

shutdown -h now
