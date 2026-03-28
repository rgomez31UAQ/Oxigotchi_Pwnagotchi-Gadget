// QPU packet classifier — classifies 802.11 frame entries by type/subtype.
//
// Contains a real VideoCore IV QPU classifier kernel that classifies frames
// using conditional execution (setf + cond) and VPM DMA store, plus a
// CPU-side fallback classifier.

use std::sync::Arc;
use super::ringbuf::FrameEntry;
#[cfg(target_os = "linux")]
use super::mailbox::GpuMem;

/// QPU classifier kernel — classifies a single 802.11 frame by type/subtype.
///
/// Uniforms (set by ARM before launch):
///   u0: frame_type  (0=mgmt, 1=control, 2=data)
///   u1: frame_subtype (0-15)
///   u2: output bus address (where to DMA-store the 1-byte result)
///
/// Classification result (written to output bus address):
///   0=Unknown, 1=Beacon, 2=ProbeReq, 3=ProbeResp, 4=Auth, 5=Deauth,
///   6=AssocReq, 7=AssocResp, 8=Data, 9=Control
///
/// QPU ISA notes:
///   - No branches for classification — uses ALU setf + conditional ldi
///   - VPM DMA store writes the result byte to shared memory
///   - Encoding follows the proven pattern from qpu_hello.h
///   - Each instruction is 64 bits = 2 x u32 [lower, upper]
static QPU_CLASSIFIER_CODE: [u32; 64] = [
    // -- Read uniforms --
    // 0: or ra0, unif, unif    (ra0 = frame_type)
    0x15800D80, 0x10020027,
    // 1: or ra1, unif, unif    (ra1 = frame_subtype)
    0x15800D80, 0x10020067,
    // 2: or ra2, unif, unif    (ra2 = output bus address)
    0x15800D80, 0x100200A7,
    // 3: ldi ra3, 0            (ra3 = result = Unknown)
    0x00000000, 0xE00200E7,

    // -- Management subtype checks (checked first; type overrides below fix non-mgmt) --
    // 4: sub.setf nop, ra1, 0  (Z if subtype==0)
    0x0D040DC0, 0xD01209E7,
    // 5: ldi.ifz ra3, 6        (AssocReq)
    0x00000006, 0xE00400E7,
    // 6: sub.setf nop, ra1, 1
    0x0D041DC0, 0xD01209E7,
    // 7: ldi.ifz ra3, 7        (AssocResp)
    0x00000007, 0xE00400E7,
    // 8: sub.setf nop, ra1, 4
    0x0D044DC0, 0xD01209E7,
    // 9: ldi.ifz ra3, 2        (ProbeReq)
    0x00000002, 0xE00400E7,
    // 10: sub.setf nop, ra1, 5
    0x0D045DC0, 0xD01209E7,
    // 11: ldi.ifz ra3, 3       (ProbeResp)
    0x00000003, 0xE00400E7,
    // 12: sub.setf nop, ra1, 8
    0x0D048DC0, 0xD01209E7,
    // 13: ldi.ifz ra3, 1       (Beacon)
    0x00000001, 0xE00400E7,
    // 14: sub.setf nop, ra1, 11
    0x0D04BDC0, 0xD01209E7,
    // 15: ldi.ifz ra3, 4       (Auth)
    0x00000004, 0xE00400E7,
    // 16: sub.setf nop, ra1, 12
    0x0D04CDC0, 0xD01209E7,
    // 17: ldi.ifz ra3, 5       (Deauth)
    0x00000005, 0xE00400E7,

    // -- Type overrides (overwrite subtype result for non-management frames) --
    // 18: sub.setf nop, ra0, 1 (Z if type==1)
    0x0D001DC0, 0xD01209E7,
    // 19: ldi.ifz ra3, 9       (Control)
    0x00000009, 0xE00400E7,
    // 20: sub.setf nop, ra0, 2 (Z if type==2)
    0x0D002DC0, 0xD01209E7,
    // 21: ldi.ifz ra3, 8       (Data)
    0x00000008, 0xE00400E7,
    // 22: sub.setf nop, ra0, 3 (Z if type==3, reserved)
    0x0D003DC0, 0xD01209E7,
    // 23: ldi.ifz ra3, 0       (Unknown)
    0x00000000, 0xE00400E7,

    // -- VPM DMA store: write result byte to output address --
    // 24: ldi vpmvcd_wr_setup(B), VPW_SETUP_H32 (0x1A00)
    0x00001A00, 0xE0021C67,
    // 25: or vpm, ra3, ra3     (write result to VPM)
    0x150C0D80, 0x10020C27,
    // 26: ldi vpmvcd_wr_setup(B), VDW_SETUP_H32 (0x80904000)
    0x80904000, 0xE0021C67,
    // 27: or vpm_st_addr(B), ra2, ra2  (trigger DMA store)
    0x15080D80, 0x10021CA7,
    // 28: or nop, vpm_st_wait(B), vpm_st_wait(B)  (wait)
    0x15032FC0, 0x100209E7,

    // -- Thread end + 2 mandatory delay-slot nops --
    // 29: thrend
    0x009E7000, 0x300009E7,
    // 30: nop
    0x009E7000, 0x100009E7,
    // 31: nop
    0x009E7000, 0x100009E7,
];

// ---------------------------------------------------------------------------
// FrameClass — classification categories
// ---------------------------------------------------------------------------

/// Frame classification categories.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameClass {
    Unknown = 0,
    Beacon = 1,
    ProbeReq = 2,
    ProbeResp = 3,
    Auth = 4,
    Deauth = 5,
    AssocReq = 6,
    AssocResp = 7,
    Data = 8,
    Control = 9,
}

impl FrameClass {
    /// Classify a frame from its type and subtype fields.
    /// This is the CPU-side fallback classifier (same logic the QPU will execute).
    pub fn classify(frame_type: u8, frame_subtype: u8) -> Self {
        match frame_type {
            0 => match frame_subtype {
                // Management frames
                0 => FrameClass::AssocReq,
                1 => FrameClass::AssocResp,
                4 => FrameClass::ProbeReq,
                5 => FrameClass::ProbeResp,
                8 => FrameClass::Beacon,
                11 => FrameClass::Auth,   // 0x0B
                12 => FrameClass::Deauth, // 0x0C
                _ => FrameClass::Unknown,
            },
            1 => FrameClass::Control,
            2 => FrameClass::Data,
            _ => FrameClass::Unknown,
        }
    }
}

// ---------------------------------------------------------------------------
// Classifier — QPU classifier engine (Linux)
// ---------------------------------------------------------------------------

/// QPU classifier engine — loads the kernel binary into GPU memory,
/// executes it against a ring buffer, and reads back results.
#[cfg(target_os = "linux")]
pub struct Classifier {
    code_mem: GpuMem,     // GPU memory holding the QPU binary
    output_mem: GpuMem,   // GPU memory for classification results
    uniform_mem: GpuMem,  // GPU memory for QPU uniforms (3 x u32)
    output_capacity: u32, // Max frames per batch
}

#[cfg(target_os = "linux")]
impl Classifier {
    /// Create a new classifier, loading the QPU binary into GPU memory.
    /// `output_capacity` is the max frames per classification batch.
    pub fn new(mbox: Arc<super::mailbox::Mailbox>, output_capacity: u32) -> Result<Self, String> {
        // Allocate GPU memory for QPU code
        let code_size = (QPU_CLASSIFIER_CODE.len() * 4) as u32;
        // Round up to page size
        let code_alloc = ((code_size + 4095) / 4096) * 4096;
        let code_mem = GpuMem::alloc(mbox.clone(), code_alloc)?;

        // Copy QPU binary into GPU memory
        unsafe {
            let dst = code_mem.as_ptr() as *mut u32;
            for (i, &word) in QPU_CLASSIFIER_CODE.iter().enumerate() {
                std::ptr::write_volatile(dst.add(i), word);
            }
        }

        // Allocate GPU memory for output (1 byte per frame, page-aligned)
        let output_size = ((output_capacity + 4095) / 4096) * 4096;
        let output_mem = GpuMem::alloc(mbox.clone(), output_size.max(4096))?;

        // Allocate GPU memory for uniforms (3 x u32 = 12 bytes, page-aligned)
        let uniform_mem = GpuMem::alloc(mbox, 4096)?;

        Ok(Classifier {
            code_mem,
            output_mem,
            uniform_mem,
            output_capacity,
        })
    }

    /// Classify frames in the ring buffer using the QPU.
    ///
    /// Launches the QPU once per frame, passing type/subtype via uniforms.
    /// The QPU kernel classifies the frame and writes the result byte to
    /// output_mem via VPM DMA. Falls back to CPU classification on QPU error.
    ///
    /// NOTE: Phase 1 — O(n) QPU launches (one per frame). CPU fallback is faster
    /// at batch sizes > 1. Batch optimization via QPU branch loop is Phase 2.
    ///
    /// Returns a Vec of (FrameClass, FrameEntry) pairs for each classified frame.
    pub fn classify_batch(
        &self,
        ring: &mut super::ringbuf::RingBuf,
        v3d: &super::mailbox::V3dRegs,
    ) -> Result<Vec<(FrameClass, FrameEntry)>, String> {
        let count = ring.available().min(self.output_capacity);
        if count == 0 {
            return Ok(Vec::new());
        }

        let entries = ring.drain(count);
        let mut results = Vec::with_capacity(entries.len());

        for entry in &entries {
            let ft = unsafe { std::ptr::addr_of!(entry.frame_type).read_unaligned() };
            let fst = unsafe { std::ptr::addr_of!(entry.frame_subtype).read_unaligned() };

            // Set up uniforms: [frame_type, frame_subtype, output_bus_addr]
            unsafe {
                let u_ptr = self.uniform_mem.as_ptr() as *mut u32;
                std::ptr::write_volatile(u_ptr.add(0), ft as u32);
                std::ptr::write_volatile(u_ptr.add(1), fst as u32);
                std::ptr::write_volatile(u_ptr.add(2), self.output_mem.bus_addr());

                // Clear output byte
                std::ptr::write_volatile(self.output_mem.as_ptr(), 0xFF);
            }
            std::sync::atomic::fence(std::sync::atomic::Ordering::Release);

            // Execute QPU
            match v3d.execute_qpu(
                self.code_mem.bus_addr(),
                self.uniform_mem.bus_addr(),
                3, // 3 uniforms
                500, // 500ms timeout
            ) {
                Ok(()) => {
                    std::sync::atomic::fence(std::sync::atomic::Ordering::Acquire);
                    let class_byte = unsafe {
                        std::ptr::read_volatile(self.output_mem.as_ptr())
                    };
                    // Sentinel check: 0xFF means DMA store never fired
                    if class_byte == 0xFF {
                        results.push((FrameClass::classify(ft, fst), *entry));
                        continue;
                    }
                    let class = match class_byte {
                        0 => FrameClass::Unknown,
                        1 => FrameClass::Beacon,
                        2 => FrameClass::ProbeReq,
                        3 => FrameClass::ProbeResp,
                        4 => FrameClass::Auth,
                        5 => FrameClass::Deauth,
                        6 => FrameClass::AssocReq,
                        7 => FrameClass::AssocResp,
                        8 => FrameClass::Data,
                        9 => FrameClass::Control,
                        _ => FrameClass::Unknown,
                    };
                    results.push((class, *entry));
                }
                Err(_) => {
                    // QPU failed — fall back to CPU for this frame
                    results.push((FrameClass::classify(ft, fst), *entry));
                }
            }
        }

        Ok(results)
    }

    /// CPU-side batch classification (fallback when QPU is not available).
    /// Takes pre-extracted frame entries and classifies them.
    pub fn classify_cpu(entries: &[FrameEntry]) -> Vec<FrameClass> {
        entries
            .iter()
            .map(|e| {
                // SAFETY: Reading from packed struct fields
                let ft = unsafe { std::ptr::addr_of!(e.frame_type).read_unaligned() };
                let fst = unsafe { std::ptr::addr_of!(e.frame_subtype).read_unaligned() };
                FrameClass::classify(ft, fst)
            })
            .collect()
    }

    /// Get the bus address of the QPU code (for diagnostic purposes).
    pub fn code_bus_addr(&self) -> u32 {
        self.code_mem.bus_addr()
    }

    /// Get the bus address of the output buffer.
    pub fn output_bus_addr(&self) -> u32 {
        self.output_mem.bus_addr()
    }

    /// Get the output capacity.
    pub fn output_capacity(&self) -> u32 {
        self.output_capacity
    }
}

// ---------------------------------------------------------------------------
// Classifier — non-Linux stub
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "linux"))]
pub struct Classifier;

#[cfg(not(target_os = "linux"))]
impl Classifier {
    pub fn new(
        _mbox: Arc<super::mailbox::Mailbox>,
        _output_capacity: u32,
    ) -> Result<Self, String> {
        Err("Classifier requires Linux with QPU access".into())
    }

    pub fn classify_batch(
        &self,
        _ring: &mut super::ringbuf::RingBuf,
        _v3d: &super::mailbox::V3dRegs,
    ) -> Result<Vec<(FrameClass, FrameEntry)>, String> {
        Err("Classifier requires Linux with QPU access".into())
    }

    pub fn classify_cpu(entries: &[FrameEntry]) -> Vec<FrameClass> {
        entries
            .iter()
            .map(|e| {
                let ft = unsafe { std::ptr::addr_of!(e.frame_type).read_unaligned() };
                let fst = unsafe { std::ptr::addr_of!(e.frame_subtype).read_unaligned() };
                FrameClass::classify(ft, fst)
            })
            .collect()
    }

    pub fn code_bus_addr(&self) -> u32 {
        0
    }
    pub fn output_bus_addr(&self) -> u32 {
        0
    }
    pub fn output_capacity(&self) -> u32 {
        0
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_beacon() {
        assert_eq!(FrameClass::classify(0, 8), FrameClass::Beacon);
    }

    #[test]
    fn test_classify_probe_req() {
        assert_eq!(FrameClass::classify(0, 4), FrameClass::ProbeReq);
    }

    #[test]
    fn test_classify_probe_resp() {
        assert_eq!(FrameClass::classify(0, 5), FrameClass::ProbeResp);
    }

    #[test]
    fn test_classify_auth() {
        assert_eq!(FrameClass::classify(0, 11), FrameClass::Auth);
    }

    #[test]
    fn test_classify_deauth() {
        assert_eq!(FrameClass::classify(0, 12), FrameClass::Deauth);
    }

    #[test]
    fn test_classify_assoc_req() {
        assert_eq!(FrameClass::classify(0, 0), FrameClass::AssocReq);
    }

    #[test]
    fn test_classify_assoc_resp() {
        assert_eq!(FrameClass::classify(0, 1), FrameClass::AssocResp);
    }

    #[test]
    fn test_classify_data() {
        assert_eq!(FrameClass::classify(2, 0), FrameClass::Data);
        assert_eq!(FrameClass::classify(2, 4), FrameClass::Data); // any data subtype
    }

    #[test]
    fn test_classify_control() {
        assert_eq!(FrameClass::classify(1, 0), FrameClass::Control);
        assert_eq!(FrameClass::classify(1, 13), FrameClass::Control); // ACK
    }

    #[test]
    fn test_classify_unknown_type() {
        assert_eq!(FrameClass::classify(3, 0), FrameClass::Unknown);
    }

    #[test]
    fn test_classify_unknown_mgmt_subtype() {
        assert_eq!(FrameClass::classify(0, 15), FrameClass::Unknown);
    }

    #[test]
    fn test_classify_cpu_batch() {
        let entries = vec![
            FrameEntry {
                bssid: [0; 6],
                frame_type: 0,
                frame_subtype: 8,
                channel: 1,
                rssi: -50,
                flags: 0,
                _pad: 0,
                seq_num: 0,
                timestamp_ms: 0,
                ssid_hash: 0,
                frame_len: 100,
                _reserved: [0; 6],
            },
            FrameEntry {
                bssid: [0; 6],
                frame_type: 2,
                frame_subtype: 0,
                channel: 6,
                rssi: -30,
                flags: 0,
                _pad: 0,
                seq_num: 1,
                timestamp_ms: 100,
                ssid_hash: 0,
                frame_len: 200,
                _reserved: [0; 6],
            },
        ];

        let classes = Classifier::classify_cpu(&entries);
        assert_eq!(classes.len(), 2);
        assert_eq!(classes[0], FrameClass::Beacon);
        assert_eq!(classes[1], FrameClass::Data);
    }

    #[test]
    fn test_frame_class_values() {
        assert_eq!(FrameClass::Unknown as u8, 0);
        assert_eq!(FrameClass::Beacon as u8, 1);
        assert_eq!(FrameClass::ProbeReq as u8, 2);
        assert_eq!(FrameClass::ProbeResp as u8, 3);
        assert_eq!(FrameClass::Auth as u8, 4);
        assert_eq!(FrameClass::Deauth as u8, 5);
        assert_eq!(FrameClass::AssocReq as u8, 6);
        assert_eq!(FrameClass::AssocResp as u8, 7);
        assert_eq!(FrameClass::Data as u8, 8);
        assert_eq!(FrameClass::Control as u8, 9);
    }

    #[test]
    fn test_qpu_classifier_code_structure() {
        // Real kernel: 32 instructions = 64 u32 words
        assert_eq!(QPU_CLASSIFIER_CODE.len(), 64);
        // Last 3 instructions: thrend (idx 29) + 2 nop delay slots (idx 30, 31)
        // Each instruction is [lower, upper] so upper word indices are 59, 61, 63
        // thrend: upper word sig=0x3 at index 59
        assert_eq!(QPU_CLASSIFIER_CODE[59] >> 28, 0x3, "instruction 29 must be thrend (sig=3)");
        // nop delay slots: upper word sig=0x1 at indices 61, 63
        assert_eq!(QPU_CLASSIFIER_CODE[61] >> 28, 0x1, "delay slot 1 must be nop (sig=1)");
        assert_eq!(QPU_CLASSIFIER_CODE[63] >> 28, 0x1, "delay slot 2 must be nop (sig=1)");
    }

    #[test]
    fn test_qpu_kernel_vpm_dma_pattern() {
        // Instruction 24: VPM write setup (matches qpu_hello.h)
        assert_eq!(QPU_CLASSIFIER_CODE[48], 0x00001A00); // VPW_SETUP_H32
        assert_eq!(QPU_CLASSIFIER_CODE[49], 0xE0021C67); // ldi to vpmvcd_wr_setup(B)
        // Instruction 26: DMA store setup
        assert_eq!(QPU_CLASSIFIER_CODE[52], 0x80904000); // VDW_SETUP_H32
        assert_eq!(QPU_CLASSIFIER_CODE[53], 0xE0021C67);
    }
}
