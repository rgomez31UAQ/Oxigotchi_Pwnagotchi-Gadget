# BT Attacks — Status & Limitations

**Status: Experimental**

BT offensive mode is functional but limited by the BCM43436B0 hardware on the Pi Zero 2W. 5 of 8 attack types work. Manual device targeting is not yet implemented. Unlike WiFi (where deauth frames are unauthenticated and trivially spoofed), Bluetooth connections are authenticated at the link layer — there is no equivalent of WiFi deauth in BT.

## Hardware Constraints

The Pi Zero 2W uses a BCM43436B0 (also identified as BCM43430B0) combo chip sharing a single UART between WiFi and BT. This imposes hard limits:

- **Single radio**: WiFi and BT cannot operate simultaneously. Mode switching (RAGE/BT/SAFE) swaps the radio between WiFi monitor mode and BT HCI.
- **Single BT adapter**: The chip provides one BT controller. Attacks requiring two adapters (relay/MITM) are hardware-impossible.
- **Limited patchram**: The BT firmware has only **1,393 bytes** of contiguous free space at `0x212A77`. Attacks needing large firmware patches (>5KB) cannot fit.

## Attack Status

### Working (5/8)

| Attack | Type | Rage Level | What It Does |
|--------|------|-----------|--------------|
| **SMP Downgrade** | BLE | Low+ | Initiates BLE connection, forces Just Works pairing (NoInputNoOutput IO capability), captures pairing keys as transcripts. Targets devices that accept new pairing requests. |
| **BLE Adv Injection** | BLE | Medium+ | Clones target's BT address via vendor Write_BDADDR command, then broadcasts connectable advertisements impersonating the target. Can trick nearby devices into connecting to the oxigotchi instead. |
| **L2CAP Fuzz** | Classic | Medium+ | Opens L2CAP signaling channel to target, sends 4 malformed packets (oversized echo, invalid info type, reserved PSM, bad MTU). Detects crashes and captures triggering payload. |
| **ATT/GATT Fuzz** | BLE | Medium+ | Opens ATT fixed channel, sends 5 malformed PDUs (invalid handle ranges, empty writes, oversized offsets). Detects crashes and captures trigger. |
| **Vendor Cmd Unlock** | Local | Low+ | Reads the LOCAL controller's firmware state — sends READ_LOCAL_VERSION, READ_VERBOSE_CONFIG, and reads patchram base (0x211700). Diagnostics tool, not an attack on external devices. Requires attack patchram loaded. |

### Broken — Firmware RE Incomplete (1/8)

| Attack | Type | Problem |
|--------|------|---------|
| **KNOB** | Classic | Key Negotiation of Bluetooth attack. Should patch the LMP key-size handler to force `key_size=1`, making encryption trivially crackable. Currently broken: the LMP handler address in firmware has not been reverse-engineered, so `discover_lmp_key_size_addr()` returns `None`. The patch payload bytes are also TBD. Requires attack patchram. |

### Hardware-Impossible on BCM43436B0 (2/8)

| Attack | Type | Problem |
|--------|------|---------|
| **SMP MITM** | BLE | Man-in-the-middle of BLE pairing requires two BT adapters — one to impersonate each endpoint of the pairing. The Pi Zero 2W has a single BCM43436B0 adapter. Cannot be implemented without external USB BT dongle. |
| **BLE Conn Hijack** | BLE | Hijacking an active BLE connection requires Link Layer hooks in firmware, needing >5KB of patchram space. The BCM43436B0 has only 1,393 bytes of contiguous free space. Cannot fit the required patches. |

## Rage Levels

Three discrete levels filter which attacks are permitted:

| Level | Permitted | Description |
|-------|-----------|-------------|
| **Low** | SMP Downgrade, KNOB*, Vendor Cmd Unlock | Passive diagnostics and self-targeted. Minimal risk. |
| **Medium** | All Low + BLE Adv Injection, L2CAP Fuzz, ATT/GATT Fuzz | Active attacks on external devices. |
| **High** | All Medium + SMP MITM*, BLE Conn Hijack* | Full aggression including MITM and hijack. |

*These attacks are toggled on at the rage level but fail due to hardware/RE limitations (see above).

## Target Selection

**Manual targeting is not implemented.** The "Target" button in the web dashboard device table is a UI placeholder — it stores the address but the attack scheduler ignores it.

Targets are selected **automatically** by the `TargetSelector` each BT epoch:

1. Filter out whitelisted devices, weak signals (below `min_rssi`, default -80 dBm), and devices already being attacked or captured
2. Check transport compatibility (BLE attacks for BLE/Dual devices, Classic attacks for Classic/Dual)
3. Score: signal strength (0-127) + novelty bonus (50 for untouched) + named bonus (10 if device has a name)
4. Sort by score, take top N (default `max_concurrent_attacks = 3`)

## BT vs WiFi — Why BT Is Harder

| | WiFi | Bluetooth |
|--|------|-----------|
| **Disconnect attacks** | Trivial — deauth frames are unauthenticated management frames (pre-802.11w) | No equivalent — connections are authenticated at the link layer |
| **Passive capture** | Monitor mode captures all traffic on a channel | Cannot sniff encrypted BT connections without the link key |
| **Attack surface** | Large — 802.11 has many unauthenticated frame types | Small — most BT operations require an established connection |
| **Range** | 50-100m typical | 10-30m typical, often less |
| **Disruption capability** | High — deauth, CSA, rogue AP, disassoc all cause immediate disconnects | Low — must find firmware bugs (fuzzing) or break encryption (KNOB) first |

The WiFi attack surface is fundamentally larger because the 802.11 standard was designed with unauthenticated management frames. BT was designed with authentication from the start. This is why WiFi RAGE mode captures dozens of handshakes while BT mode is more about opportunistic key capture and vulnerability discovery.

## What BT Mode Is Good For

- **Pairing key capture** (SMP Downgrade) — grab keys from devices that accept new pairing
- **Device impersonation** (BLE Adv Injection) — clone a device's address and broadcast as it
- **Vulnerability discovery** (L2CAP/ATT Fuzz) — find crashable devices
- **Firmware diagnostics** (Vendor Cmd Unlock) — inspect the local BT chip state
- **Device enumeration** — the device table shows all nearby BT devices with type, signal, and name

## Future Work

- **KNOB**: Requires BCM43436B0 BT firmware reverse engineering to locate the LMP key-size handler and write the patch payload
- **Manual targeting**: Wire `pending_bt_target` through to `TargetSelector` so the dashboard "Target" button actually works
- **SMP MITM**: Would need an external USB BT adapter (CSR8510 or similar) as a second radio
- **BLE Conn Hijack**: No path forward on BCM43436B0 — patchram space is a hard physical limit
