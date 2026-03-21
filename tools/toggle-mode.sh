#!/bin/bash
if pgrep -f "pwnagotchi --manual" > /dev/null 2>&1; then
    echo "Switching to AUTO..." > /tmp/.pwnagotchi-button-msg
    touch /root/.pwnagotchi-auto
else
    echo "Switching to MANU..." > /tmp/.pwnagotchi-button-msg
    touch /root/.pwnagotchi-manual
fi
sleep 5
rm -f /tmp/.pwnagotchi-button-msg
systemctl restart bettercap
sleep 1
systemctl restart pwnagotchi
