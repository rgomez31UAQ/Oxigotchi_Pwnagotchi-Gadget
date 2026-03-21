#!/bin/bash
STATUS=$(/usr/local/bin/pwnoxide-mode.sh status 2>&1)
if echo "$STATUS" | grep -qi 'AO mode'; then
    /usr/local/bin/pwnoxide-mode.sh pwn
else
    /usr/local/bin/pwnoxide-mode.sh ao
fi
