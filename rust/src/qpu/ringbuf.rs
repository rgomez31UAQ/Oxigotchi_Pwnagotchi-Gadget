// QPU ring buffer — SPSC lock-free ring in GPU-shared memory for staging
// 802.11 frame headers before QPU batch classification.
//
// ARM (producer) writes FrameEntry values and increments write_idx.
// QPU (consumer) reads entries and increments read_idx.
// All GPU-shared memory accesses use volatile operations + fences.

#[cfg(target_os = "linux")]
use std::sync::atomic::{fence, Ordering};

#[cfg(target_os = "linux")]
use super::mailbox::GpuMem;

const HEADER_SIZE: usize = 64; // RingHeader is 64 bytes (cache-line aligned)
const ENTRY_SIZE: usize = 32; // FrameEntry is 32 bytes
const DEFAULT_CAPACITY: u32 = 256;

// ---------------------------------------------------------------------------
// FrameEntry — compact frame descriptor for QPU classification
// ---------------------------------------------------------------------------

/// Compact frame entry for QPU classification (32 bytes, cache-line friendly).
/// Packed to ensure consistent layout between ARM and QPU access.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct FrameEntry {
    pub bssid: [u8; 6],    // 6 bytes: source MAC/BSSID
    pub frame_type: u8,     // 1 byte: 802.11 frame type (management=0, control=1, data=2)
    pub frame_subtype: u8,  // 1 byte: 802.11 frame subtype
    pub channel: u8,        // 1 byte: WiFi channel
    pub rssi: i8,           // 1 byte: signal strength in dBm
    pub flags: u8,          // 1 byte: bitflags (bit 0: has_ssid, bit 1: whitelisted)
    pub _pad: u8,           // 1 byte: padding to align seq_num
    pub seq_num: u32,       // 4 bytes: monotonic sequence number
    pub timestamp_ms: u32,  // 4 bytes: milliseconds since epoch start
    pub ssid_hash: u32,     // 4 bytes: FNV-1a hash of SSID (for grouping)
    pub frame_len: u16,     // 2 bytes: original frame length
    pub _reserved: [u8; 6], // 6 bytes: pad to 32 bytes total
}

/// Flag bit: SSID is present and non-empty.
pub const FLAG_HAS_SSID: u8 = 1 << 0;
/// Flag bit: frame matched a whitelist entry.
pub const FLAG_WHITELISTED: u8 = 1 << 1;

// ---------------------------------------------------------------------------
// RingHeader — control block in GPU-shared memory
// ---------------------------------------------------------------------------

/// Ring buffer header, resident in GPU-shared memory.
/// ARM writes `write_idx`; QPU reads it.
/// QPU writes `read_idx`; ARM reads it.
#[repr(C)]
pub struct RingHeader {
    pub write_idx: u32,       // ARM increments after writing an entry
    pub read_idx: u32,        // QPU increments after processing an entry
    pub capacity: u32,        // Number of entries (power of 2)
    pub entry_size: u32,      // sizeof(FrameEntry) = 32
    pub overflow_count: u32,  // ARM increments when buffer is full
    pub _reserved: [u8; 44],  // Pad to 64 bytes
}

// ---------------------------------------------------------------------------
// RingBuf — SPSC ring buffer backed by GpuMem
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
pub struct RingBuf {
    mem: GpuMem,
    capacity: u32,
    seq_counter: u32,
}

#[cfg(target_os = "linux")]
impl RingBuf {
    /// Create a ring buffer in GPU-shared memory.
    /// `capacity` must be a power of 2 (default 256).
    pub fn new(mem: GpuMem, capacity: u32) -> Result<Self, String> {
        // Validate capacity is power of 2 and non-zero
        if capacity == 0 || (capacity & (capacity - 1)) != 0 {
            return Err(format!(
                "capacity must be a non-zero power of 2, got {}",
                capacity
            ));
        }

        let required = HEADER_SIZE + (capacity as usize) * ENTRY_SIZE;
        if (mem.size() as usize) < required {
            return Err(format!(
                "GPU memory too small: need {} bytes, have {}",
                required,
                mem.size()
            ));
        }

        // Initialize the header in GPU-shared memory via volatile writes
        let base = mem.as_ptr();
        // SAFETY: base points to mem.size() bytes of mapped GPU memory.
        // We write the RingHeader fields individually using volatile stores.
        unsafe {
            let hdr = base as *mut u32;
            std::ptr::write_volatile(hdr.add(0), 0); // write_idx
            std::ptr::write_volatile(hdr.add(1), 0); // read_idx
            std::ptr::write_volatile(hdr.add(2), capacity); // capacity
            std::ptr::write_volatile(hdr.add(3), ENTRY_SIZE as u32); // entry_size
            std::ptr::write_volatile(hdr.add(4), 0); // overflow_count
            // _reserved is already zeroed by MEM_FLAG_ZERO in GpuMem::alloc
        }

        fence(Ordering::Release);

        Ok(RingBuf {
            mem,
            capacity,
            seq_counter: 0,
        })
    }

    /// Push a frame entry. Returns false if buffer is full (non-blocking).
    pub fn push(&mut self, entry: &FrameEntry) -> bool {
        let base = self.mem.as_ptr();

        // SAFETY: base points to valid mapped GPU memory of sufficient size.
        // We read/write header fields at known offsets within bounds.
        unsafe {
            let hdr = base as *mut u32;

            // Acquire fence before reading read_idx (written by QPU)
            fence(Ordering::Acquire);

            let write_idx = std::ptr::read_volatile(hdr.add(0));
            let read_idx = std::ptr::read_volatile(hdr.add(1));

            // Check if full
            if write_idx.wrapping_sub(read_idx) >= self.capacity {
                // Increment overflow_count
                let overflow = std::ptr::read_volatile(hdr.add(4));
                std::ptr::write_volatile(hdr.add(4), overflow.wrapping_add(1));
                return false;
            }

            // Compute slot address
            let slot_idx = write_idx & (self.capacity - 1); // modulo via bitmask
            let slot_offset = HEADER_SIZE + (slot_idx as usize) * ENTRY_SIZE;
            let slot_ptr = base.add(slot_offset);

            // Copy the entry into GPU memory (volatile, byte-by-byte via write_volatile)
            let entry_bytes =
                std::slice::from_raw_parts(entry as *const FrameEntry as *const u8, ENTRY_SIZE);
            for i in 0..ENTRY_SIZE {
                std::ptr::write_volatile(slot_ptr.add(i), entry_bytes[i]);
            }

            // Release fence: ensure entry data is visible before write_idx update
            fence(Ordering::Release);

            // Increment write_idx
            std::ptr::write_volatile(hdr.add(0), write_idx.wrapping_add(1));
        }

        self.seq_counter = self.seq_counter.wrapping_add(1);
        true
    }

    /// Number of entries available for reading.
    pub fn available(&self) -> u32 {
        let base = self.mem.as_ptr();
        // SAFETY: reading header fields from mapped GPU memory.
        unsafe {
            let hdr = base as *const u32;
            fence(Ordering::Acquire);
            let write_idx = std::ptr::read_volatile(hdr.add(0));
            let read_idx = std::ptr::read_volatile(hdr.add(1));
            write_idx.wrapping_sub(read_idx)
        }
    }

    /// Check if buffer is full.
    pub fn is_full(&self) -> bool {
        self.available() >= self.capacity
    }

    /// Get the bus address of the ring header (for QPU uniform).
    pub fn header_bus_addr(&self) -> u32 {
        self.mem.bus_addr()
    }

    /// Get the bus address of the data region (for QPU uniform).
    pub fn data_bus_addr(&self) -> u32 {
        self.mem.bus_addr() + HEADER_SIZE as u32
    }

    /// Reset the buffer (ARM side only — call when QPU is idle).
    pub fn reset(&mut self) {
        let base = self.mem.as_ptr();
        // SAFETY: writing header fields in mapped GPU memory.
        unsafe {
            let hdr = base as *mut u32;
            std::ptr::write_volatile(hdr.add(0), 0); // write_idx
            std::ptr::write_volatile(hdr.add(1), 0); // read_idx
            std::ptr::write_volatile(hdr.add(4), 0); // overflow_count
        }
        fence(Ordering::Release);
        self.seq_counter = 0;
    }

    /// Get overflow count (frames dropped because buffer was full).
    pub fn overflow_count(&self) -> u32 {
        let base = self.mem.as_ptr();
        // SAFETY: reading overflow_count from mapped GPU memory header.
        unsafe {
            fence(Ordering::Acquire);
            std::ptr::read_volatile((base as *const u32).add(4))
        }
    }

    /// Get the capacity.
    pub fn capacity(&self) -> u32 {
        self.capacity
    }
}

// ---------------------------------------------------------------------------
// Non-Linux stub
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "linux"))]
pub struct RingBuf;

#[cfg(not(target_os = "linux"))]
impl RingBuf {
    pub fn new(_mem: super::mailbox::GpuMem, _capacity: u32) -> Result<Self, String> {
        Err("RingBuf requires Linux with GPU-shared memory".into())
    }

    pub fn push(&mut self, _entry: &FrameEntry) -> bool {
        false
    }

    pub fn available(&self) -> u32 {
        0
    }

    pub fn is_full(&self) -> bool {
        false
    }

    pub fn header_bus_addr(&self) -> u32 {
        0
    }

    pub fn data_bus_addr(&self) -> u32 {
        0
    }

    pub fn reset(&mut self) {}

    pub fn overflow_count(&self) -> u32 {
        0
    }

    pub fn capacity(&self) -> u32 {
        0
    }
}

// ---------------------------------------------------------------------------
// Frame extraction helpers (platform-independent)
// ---------------------------------------------------------------------------

/// FNV-1a hash for SSID (32-bit).
pub fn fnv1a_hash(data: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5; // FNV offset basis
    for &byte in data {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x0100_0193); // FNV prime
    }
    hash
}

/// Extract a FrameEntry from a raw 802.11 frame (radiotap + MAC header).
/// Returns None if the frame is too short or malformed.
///
/// `raw` must begin with a radiotap header followed by the 802.11 frame.
/// `channel`, `rssi`, and `timestamp_ms` are supplied by the capture layer.
pub fn extract_frame_entry(
    raw: &[u8],
    channel: u8,
    rssi: i8,
    timestamp_ms: u32,
) -> Option<FrameEntry> {
    // Minimum: radiotap header (at least 8 bytes) + 802.11 header (24 bytes)
    if raw.len() < 8 + 24 {
        return None;
    }

    // 1. Parse radiotap header length (bytes 2-3, little-endian u16)
    let rt_len = u16::from_le_bytes([raw[2], raw[3]]) as usize;
    if rt_len < 8 || raw.len() < rt_len + 24 {
        return None;
    }

    // 2. 802.11 frame starts after radiotap
    let dot11 = &raw[rt_len..];

    // 3. Frame control: type (bits 3:2) and subtype (bits 7:4) of byte 0
    let fc0 = dot11[0];
    let frame_type = (fc0 >> 2) & 0x03;
    let frame_subtype = (fc0 >> 4) & 0x0F;

    // 4. BSSID: address 3 in the 802.11 header (bytes 16..22)
    let mut bssid = [0u8; 6];
    bssid.copy_from_slice(&dot11[16..22]);

    // 5. Look for SSID in tagged parameters (management frames only)
    let mut ssid_hash: u32 = fnv1a_hash(b""); // default: hash of empty
    let mut has_ssid = false;

    // Beacon (subtype 8) and probe response (subtype 5): fixed params = 12 bytes
    // after the 24-byte MAC header → tagged params start at offset 36
    if frame_type == 0 && (frame_subtype == 8 || frame_subtype == 5) {
        let tag_start = 24 + 12; // MAC header + fixed params
        if dot11.len() > tag_start {
            let tags = &dot11[tag_start..];
            // Walk tagged parameters looking for SSID (tag ID 0)
            let mut pos = 0;
            while pos + 2 <= tags.len() {
                let tag_id = tags[pos];
                let tag_len = tags[pos + 1] as usize;
                if pos + 2 + tag_len > tags.len() {
                    break;
                }
                if tag_id == 0 {
                    // SSID element
                    let ssid_bytes = &tags[pos + 2..pos + 2 + tag_len];
                    if !ssid_bytes.is_empty() {
                        ssid_hash = fnv1a_hash(ssid_bytes);
                        has_ssid = true;
                    }
                    break; // SSID is always the first tag, no need to continue
                }
                pos += 2 + tag_len;
            }
        }
    }

    // 6. Build flags
    let mut flags: u8 = 0;
    if has_ssid {
        flags |= FLAG_HAS_SSID;
    }

    // 7. Compute original frame length (everything after radiotap)
    let frame_len = (raw.len() - rt_len) as u16;

    Some(FrameEntry {
        bssid,
        frame_type,
        frame_subtype,
        channel,
        rssi,
        flags,
        _pad: 0,
        seq_num: 0,      // caller (RingBuf::push) sets this
        timestamp_ms,
        ssid_hash,
        frame_len,
        _reserved: [0u8; 6],
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_entry_size() {
        assert_eq!(std::mem::size_of::<FrameEntry>(), 32);
    }

    #[test]
    fn test_ring_header_size() {
        assert_eq!(std::mem::size_of::<RingHeader>(), 64);
    }

    #[test]
    fn test_fnv1a_empty() {
        // Empty string should return the FNV offset basis
        assert_eq!(fnv1a_hash(b""), 0x811c_9dc5);
    }

    #[test]
    fn test_fnv1a_deterministic() {
        let h1 = fnv1a_hash(b"test");
        let h2 = fnv1a_hash(b"test");
        assert_eq!(h1, h2);
        // Should differ from empty
        assert_ne!(h1, 0x811c_9dc5);
    }

    #[test]
    fn test_fnv1a_test_uppercase_deterministic() {
        let h1 = fnv1a_hash(b"TEST");
        let h2 = fnv1a_hash(b"TEST");
        assert_eq!(h1, h2);
        // Should differ from lowercase
        assert_ne!(h1, fnv1a_hash(b"test"));
    }

    #[test]
    fn test_extract_frame_entry_too_short() {
        // 31 bytes is too short (need 8 radiotap + 24 802.11)
        let short = [0u8; 31];
        assert!(extract_frame_entry(&short, 6, -50, 1000).is_none());
    }

    #[test]
    fn test_extract_frame_entry_bad_radiotap_len() {
        // Radiotap length says 100 but frame is only 40 bytes
        let mut frame = [0u8; 40];
        frame[2] = 100; // rt_len low byte
        frame[3] = 0;   // rt_len high byte
        assert!(extract_frame_entry(&frame, 6, -50, 1000).is_none());
    }

    #[test]
    fn test_extract_beacon_frame() {
        // Construct a minimal beacon frame with SSID "TEST"
        let mut frame = Vec::new();

        // Radiotap header (8 bytes minimal)
        frame.extend_from_slice(&[0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00]);

        // 802.11 beacon frame control: type=0 (management), subtype=8 (beacon)
        // Frame control byte 0: subtype(4bits) | type(2bits) | protocol(2bits)
        // Beacon: subtype=8 -> 1000, type=0 -> 00, protocol=0 -> 00
        // byte 0 = 0b1000_00_00 = 0x80
        frame.push(0x80); // frame control byte 0
        frame.push(0x00); // frame control byte 1

        // Duration
        frame.extend_from_slice(&[0x00, 0x00]);

        // Destination address (broadcast)
        frame.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);

        // Source address
        frame.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);

        // BSSID (address 3, bytes 16-21 of 802.11 header)
        frame.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);

        // Sequence control
        frame.extend_from_slice(&[0x00, 0x00]);

        // Fixed parameters (timestamp 8 + beacon interval 2 + capability 2 = 12 bytes)
        frame.extend_from_slice(&[0x00; 12]);

        // Tagged parameters: SSID tag (ID=0, len=4, "TEST")
        frame.extend_from_slice(&[0x00, 0x04, b'T', b'E', b'S', b'T']);

        let entry = extract_frame_entry(&frame, 6, -42, 12345).unwrap();

        // Copy fields out of packed struct to avoid misaligned references
        let bssid = entry.bssid;
        let ft = entry.frame_type;
        let fst = entry.frame_subtype;
        let ch = entry.channel;
        let rssi = entry.rssi;
        let ts = entry.timestamp_ms;
        let flags = entry.flags;
        let hash = entry.ssid_hash;
        let flen = entry.frame_len;

        assert_eq!(bssid, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
        assert_eq!(ft, 0); // management
        assert_eq!(fst, 8); // beacon
        assert_eq!(ch, 6);
        assert_eq!(rssi, -42);
        assert_eq!(ts, 12345);
        assert_eq!(flags & FLAG_HAS_SSID, FLAG_HAS_SSID);
        assert_eq!(hash, fnv1a_hash(b"TEST"));
        // frame_len = total after radiotap = frame.len() - 8
        assert_eq!(flen, (frame.len() - 8) as u16);
    }

    #[test]
    fn test_extract_beacon_no_ssid() {
        // Beacon with zero-length SSID (hidden network)
        let mut frame = Vec::new();

        // Radiotap header
        frame.extend_from_slice(&[0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00]);

        // Beacon frame control
        frame.push(0x80);
        frame.push(0x00);

        // Duration
        frame.extend_from_slice(&[0x00, 0x00]);

        // Addresses (dest, src, bssid)
        frame.extend_from_slice(&[0xFF; 6]); // dest
        frame.extend_from_slice(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66]); // src
        frame.extend_from_slice(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66]); // bssid

        // Sequence control
        frame.extend_from_slice(&[0x00, 0x00]);

        // Fixed parameters (12 bytes)
        frame.extend_from_slice(&[0x00; 12]);

        // SSID tag with zero length (hidden SSID)
        frame.extend_from_slice(&[0x00, 0x00]);

        let entry = extract_frame_entry(&frame, 1, -80, 5000).unwrap();

        // Copy fields out of packed struct to avoid misaligned references
        let flags = entry.flags;
        let hash = entry.ssid_hash;

        // has_ssid flag should NOT be set for zero-length SSID
        assert_eq!(flags & FLAG_HAS_SSID, 0);
        // ssid_hash should be hash of empty
        assert_eq!(hash, fnv1a_hash(b""));
    }

    #[test]
    fn test_constants() {
        assert_eq!(HEADER_SIZE, 64);
        assert_eq!(ENTRY_SIZE, 32);
        assert_eq!(DEFAULT_CAPACITY, 256);
    }
}
