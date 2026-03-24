#!/bin/bash
# Watchdog: restart pisugar-server when PiSugar MCU wakes up on I2C.
# Runs every 15s via pisugar-watchdog.timer.

PISUGAR_ADDR=0x57

i2c_found() {
    python3 -c "
import fcntl, os
try:
    bus = os.open('/dev/i2c-1', os.O_RDWR)
    fcntl.ioctl(bus, 0x0703, $PISUGAR_ADDR)
    os.read(bus, 1)
    os.close(bus)
    exit(0)
except:
    exit(1)
" 2>/dev/null
}

pisugar_connected() {
    result=$(echo "get battery" | nc -q 1 127.0.0.1 8423 2>/dev/null)
    ! echo "$result" | grep -q "not connected"
}

if i2c_found && ! pisugar_connected; then
    logger "pisugar-watchdog: PiSugar MCU detected on I2C, restarting pisugar-server"
    systemctl restart pisugar-server
fi
