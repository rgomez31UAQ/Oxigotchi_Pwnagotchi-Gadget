#!/usr/bin/env python3
"""
Test SDIO RAMRW (0x500) — read/write firmware RAM via nexmon kernel module.

Sends netlink messages directly to the brcmfmac-nexmon DKMS module.
No firmware patching needed — the kernel module handles 0x500 before
it reaches the firmware.

Protocol (from /usr/src/brcmfmac-nexmon-*/core.c):
  GET: set=0, payload = addr(4 LE) + xfer_len(4 LE) → reads from firmware RAM
  SET: set=1, payload = addr(4 LE) + data(N bytes) → writes to firmware RAM

Usage:
  sudo python3 test_sdio_ramrw.py read 0x0113F4 4    # read PSM threshold
  sudo python3 test_sdio_ramrw.py read 0x011430 4    # read DPC threshold
  sudo python3 test_sdio_ramrw.py write 0x0113F4 ff   # write 0xFF to PSM
  sudo python3 test_sdio_ramrw.py test                 # run all tests
"""

import socket
import struct
import sys
import time

NETLINK_NEXMON = 31
NEXMON_CMD_SDIO_RAMRW = 0x500

# Known firmware addresses (from v5 patch)
PSM_THRESHOLD = 0x0113F4
DPC_THRESHOLD = 0x011430
RSSI_THRESHOLD = 0x011460

def build_nexmon_nlmsg(cmd, set_flag, payload):
    """Build a netlink message for the nexmon kernel module."""
    # nexudp_ioctl_header: nex[4] + cmd[4] + set[4] + payload[N]
    nex_magic = b'NEX\x00'
    frame = nex_magic + struct.pack('<II', cmd, set_flag) + payload

    # nlmsghdr: len[4] + type[2] + flags[2] + seq[4] + pid[4]
    nlmsg_len = 16 + len(frame)
    nlmsg = struct.pack('<IHHII', nlmsg_len, 0, 0, 0, 0) + frame
    return nlmsg


def sdio_read(addr, length):
    """Read `length` bytes from firmware RAM at `addr`."""
    payload = struct.pack('<II', addr, length)
    nlmsg = build_nexmon_nlmsg(NEXMON_CMD_SDIO_RAMRW, 0, payload)

    sock = socket.socket(socket.AF_NETLINK, socket.SOCK_RAW, NETLINK_NEXMON)
    try:
        sock.bind((0, 0))
        sock.settimeout(3.0)
        sock.send(nlmsg)
        resp = sock.recv(4096)
        # Response: nlmsghdr(16) + data
        if len(resp) > 16:
            return resp[16:16 + length]
        return None
    except socket.timeout:
        print("ERROR: netlink timeout (module not loaded?)")
        return None
    except OSError as e:
        print(f"ERROR: {e}")
        return None
    finally:
        sock.close()


def sdio_write(addr, data):
    """Write `data` bytes to firmware RAM at `addr`."""
    payload = struct.pack('<I', addr) + data
    nlmsg = build_nexmon_nlmsg(NEXMON_CMD_SDIO_RAMRW, 1, payload)

    sock = socket.socket(socket.AF_NETLINK, socket.SOCK_RAW, NETLINK_NEXMON)
    try:
        sock.bind((0, 0))
        sock.settimeout(3.0)
        sock.send(nlmsg)
        # SET doesn't send a data reply, just check for error
        try:
            resp = sock.recv(4096)
            return True
        except socket.timeout:
            # No response = success (kernel module doesn't always reply to SET)
            return True
    except OSError as e:
        print(f"ERROR: {e}")
        return False
    finally:
        sock.close()


def test_read_thresholds():
    """Test reading PSM/DPC/RSSI threshold values."""
    print("=== Reading firmware thresholds ===")
    for name, addr in [("PSM", PSM_THRESHOLD), ("DPC", DPC_THRESHOLD), ("RSSI", RSSI_THRESHOLD)]:
        data = sdio_read(addr, 4)
        if data:
            # The threshold is in a CMP instruction: 0x2AXX where XX is the threshold
            val = data[0]  # first byte of the 2-byte instruction
            print(f"  {name} at 0x{addr:06X}: {data.hex()} (threshold byte: 0x{val:02X})")
        else:
            print(f"  {name} at 0x{addr:06X}: FAILED")


def test_read_write():
    """Test read-write-verify cycle on a safe address."""
    # Use a RAM address in the data section that we know is safe
    # The fatal_block_mask at ~0x3C094 is our own data, safe to modify
    SAFE_ADDR = 0x0003C094  # block counter from Layer 2

    print("\n=== Read-Write-Verify test ===")
    print(f"  Target: 0x{SAFE_ADDR:06X} (Layer 2 block counter)")

    # Read original
    orig = sdio_read(SAFE_ADDR, 4)
    if not orig:
        print("  Read FAILED")
        return False
    print(f"  Original value: {orig.hex()}")

    # Write test pattern
    test_val = struct.pack('<I', 0xDEADBEEF)
    if not sdio_write(SAFE_ADDR, test_val):
        print("  Write FAILED")
        return False
    print(f"  Wrote: {test_val.hex()}")

    # Read back
    verify = sdio_read(SAFE_ADDR, 4)
    if not verify:
        print("  Verify read FAILED")
        return False
    print(f"  Read back: {verify.hex()}")

    if verify == test_val:
        print("  PASS: Read-write-verify succeeded!")
    else:
        print("  FAIL: Read back doesn't match written value")

    # Restore original
    sdio_write(SAFE_ADDR, orig)
    print(f"  Restored original: {orig.hex()}")
    return verify == test_val


def test_psm_reset():
    """Test resetting PSM watchdog counter."""
    # The PSM counter is at an unknown offset in the wlc struct.
    # For now, just verify we can read the threshold instruction.
    print("\n=== PSM threshold read ===")
    data = sdio_read(PSM_THRESHOLD, 2)
    if data:
        hw = struct.unpack('<H', data)[0]
        if (hw >> 8) == 0x2A:
            imm = hw & 0xFF
            print(f"  CMP R2, #{imm} (0x{imm:02X})")
            if imm == 0xFF:
                print("  PASS: PSM threshold is 0xFF (v5 patch active)")
            else:
                print(f"  WARNING: PSM threshold is {imm}, expected 255")
        else:
            print(f"  Unexpected instruction: 0x{hw:04X}")
    else:
        print("  FAILED to read")


if __name__ == '__main__':
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)

    cmd = sys.argv[1]

    if cmd == 'test':
        test_read_thresholds()
        test_read_write()
        test_psm_reset()

    elif cmd == 'read':
        if len(sys.argv) < 4:
            print("Usage: read <addr> <len>")
            sys.exit(1)
        addr = int(sys.argv[2], 0)
        length = int(sys.argv[3], 0)
        data = sdio_read(addr, length)
        if data:
            print(f"0x{addr:06X}: {data.hex()}")
        else:
            print("FAILED")

    elif cmd == 'write':
        if len(sys.argv) < 4:
            print("Usage: write <addr> <hex_data>")
            sys.exit(1)
        addr = int(sys.argv[2], 0)
        data = bytes.fromhex(sys.argv[3])
        if sdio_write(addr, data):
            print(f"Wrote {len(data)} bytes to 0x{addr:06X}")
        else:
            print("FAILED")

    else:
        print(f"Unknown command: {cmd}")
        print(__doc__)
