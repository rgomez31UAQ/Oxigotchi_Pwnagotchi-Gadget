#!/bin/bash
if systemctl is-active --quiet bt-tether; then
    sudo systemctl stop bt-tether
    echo "BT OFF" > /tmp/.pwnagotchi-button-msg
else
    sudo systemctl start bt-tether
    echo "BT ON" > /tmp/.pwnagotchi-button-msg
fi
sleep 3
rm -f /tmp/.pwnagotchi-button-msg
