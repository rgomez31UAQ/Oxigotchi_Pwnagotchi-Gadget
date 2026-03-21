#!/bin/bash
sleep 3
if mountpoint -q /boot/firmware; then
    LOG=/boot/firmware/bootlog.txt
else
    LOG=/var/log/bootlog.txt
fi
exec >> $LOG 2>&1
echo "=== Boot $(date) ==="
echo "Uptime: $(uptime)"
echo "--- Failed Services ---"
systemctl list-units --failed
echo "--- SSH ---"
systemctl status ssh
echo "--- Emergency SSH ---"
systemctl status emergency-ssh
echo "--- SSH journal ---"
journalctl -u ssh -u emergency-ssh --no-pager -n 30
echo "--- Pwnagotchi ---"
systemctl status pwnagotchi
echo "--- Network ---"
ip addr
echo "--- Listening ports ---"
ss -tlnp
echo "--- Disk ---"
df -h
echo "=== End ==="

# Self-heal SSH
if ! ss -tln | grep -q ":22 "; then
    ssh-keygen -A
    systemctl restart ssh || systemctl restart emergency-ssh
    echo "SSH healed at $(date)"
fi
