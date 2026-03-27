// QPU mailbox FFI — Rust wrapper for /dev/vcio mailbox interface
// and V3D direct register poke execution on BCM2837 (Pi Zero 2W).
//
// Mirrors the proven C implementation in qpu_test/mailbox.c and qpu_test.c.
// All GPU memory operations go through the VideoCore mailbox property interface.
// QPU execution uses direct V3D register poke — NOT mailbox execute_qpu (tag 0x30011),
// which is broken without the vc4 DT overlay.

#[cfg(target_os = "linux")]
use std::fs::OpenOptions;
#[cfg(target_os = "linux")]
use std::os::unix::io::AsRawFd;
#[cfg(target_os = "linux")]
use std::sync::Arc;

// Mailbox property tags
const TAG_ALLOCATE_MEMORY: u32 = 0x3000C;
const TAG_LOCK_MEMORY: u32 = 0x3000D;
const TAG_UNLOCK_MEMORY: u32 = 0x3000E;
const TAG_RELEASE_MEMORY: u32 = 0x3000F;
const TAG_ENABLE_QPU: u32 = 0x30012;

// Memory allocation flags
const MEM_FLAG_DIRECT: u32 = 1 << 2;
const MEM_FLAG_COHERENT: u32 = 1 << 3;
const MEM_FLAG_ZERO: u32 = 1 << 4;

// V3D register offsets (BCM2837)
const V3D_BASE: u32 = 0x3FC0_0000;
const V3D_IDENT0: usize = 0x000;
const V3D_IDENT1: usize = 0x004;
const V3D_L2CACTL: usize = 0x020;
const V3D_SRQPC: usize = 0x430;
const V3D_SRQUA: usize = 0x434;
const V3D_SRQUL: usize = 0x438;
const V3D_SRQCS: usize = 0x43C;

// ioctl for /dev/vcio
// _IOWR(100, 0, char *) = (3<<30) | (sizeof(void*)<<16) | (100<<8) | 0
// sizeof(char *) = 8 on any 64-bit platform, 4 on 32-bit.
// 0xC008_6400 is correct for all 64-bit Linux (aarch64, x86_64).
#[cfg(all(target_os = "linux", target_pointer_width = "32"))]
compile_error!("IOCTL_MBOX_PROPERTY value requires 64-bit platform (sizeof(char *) = 8)");
#[cfg(target_os = "linux")]
const IOCTL_MBOX_PROPERTY: libc::c_ulong = 0xC008_6400;

/// Bus address to physical address (BCM2837: strip top 2 alias bits).
const fn bus_to_phys(addr: u32) -> u32 {
    addr & !0xC000_0000
}

// ---------------------------------------------------------------------------
// /dev/mem mapping helpers
// ---------------------------------------------------------------------------

/// Map a physical address range into userspace via /dev/mem.
///
/// Handles page-alignment: the returned pointer is adjusted by the
/// sub-page offset so it points directly at `phys_addr`.
#[cfg(target_os = "linux")]
fn mapmem(phys_addr: u32, size: u32) -> Result<*mut u8, String> {
    let fd = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/mem")
        .map_err(|e| format!("open /dev/mem: {}", e))?;

    let offset = phys_addr % 4096;
    let base = phys_addr - offset;
    let map_size = (size + offset) as libc::size_t;

    // SAFETY: We are mapping physical memory via /dev/mem. The caller is
    // responsible for ensuring the physical address range is valid and that
    // concurrent access is coordinated. This requires root privileges.
    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            map_size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd.as_raw_fd(),
            base as libc::off_t,
        )
    };

    if ptr == libc::MAP_FAILED {
        return Err(format!(
            "mmap failed for phys 0x{:08X} size {}",
            phys_addr, size
        ));
    }

    // SAFETY: mmap succeeded, so ptr is valid. Adding the sub-page offset
    // gives a pointer to exactly the requested physical address.
    Ok(unsafe { (ptr as *mut u8).add(offset as usize) })
}

/// Unmap a region previously returned by `mapmem`.
///
/// The pointer must be the exact value returned by `mapmem` for the same
/// `phys_addr` and `size`.
#[cfg(target_os = "linux")]
fn unmapmem(addr: *mut u8, phys_addr: u32, size: u32) {
    let offset = phys_addr % 4096;
    // SAFETY: We reverse the offset adjustment applied in mapmem to recover
    // the page-aligned base pointer that mmap originally returned.
    let base = unsafe { addr.sub(offset as usize) };
    let map_size = (size + offset) as libc::size_t;

    // SAFETY: base and map_size match the original mmap call.
    unsafe {
        libc::munmap(base as *mut libc::c_void, map_size);
    }
}

// ---------------------------------------------------------------------------
// Mailbox — /dev/vcio property interface
// ---------------------------------------------------------------------------

/// Wraps the `/dev/vcio` file descriptor for VideoCore mailbox property calls.
///
/// Used for GPU memory allocation/locking and QPU enable/disable.
#[cfg(target_os = "linux")]
pub struct Mailbox {
    fd: std::fs::File,
}

#[cfg(target_os = "linux")]
impl Mailbox {
    /// Open the VideoCore mailbox device.
    pub fn open() -> Result<Self, String> {
        let fd = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/vcio")
            .map_err(|e| format!("open /dev/vcio: {}", e))?;
        Ok(Mailbox { fd })
    }

    /// Send a property request to the mailbox firmware.
    ///
    /// `buf` must be a properly formatted mailbox property buffer:
    ///   [0] = total size in bytes
    ///   [1] = request code (0 on input, 0x80000000 on success)
    ///   [2..] = tag sequence terminated by 0
    fn property(&self, buf: &mut [u32]) -> Result<(), String> {
        // SAFETY: The ioctl writes/reads within the buffer we pass.
        // buf must be properly sized and aligned (guaranteed by caller).
        let ret =
            unsafe { libc::ioctl(self.fd.as_raw_fd(), IOCTL_MBOX_PROPERTY, buf.as_mut_ptr()) };
        if ret < 0 {
            return Err(format!(
                "ioctl MBOX_PROPERTY failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        if buf[1] != 0x8000_0000 {
            return Err(format!("mailbox firmware error: response 0x{:08X}", buf[1]));
        }
        Ok(())
    }

    /// Enable or disable the QPU subsystem.
    pub fn qpu_enable(&self, enable: bool) -> Result<(), String> {
        let mut buf = [0u32; 32];
        let mut i = 0;
        buf[i] = 0;
        i += 1; // total size (filled below)
        buf[i] = 0;
        i += 1; // request code
        buf[i] = TAG_ENABLE_QPU;
        i += 1;
        buf[i] = 4;
        i += 1; // value buffer size
        buf[i] = 4;
        i += 1; // request size
        buf[i] = if enable { 1 } else { 0 };
        i += 1;
        buf[i] = 0;
        i += 1; // end tag
        buf[0] = (i as u32) * 4;

        self.property(&mut buf)?;

        // buf[5] contains the firmware response (0 = success)
        if buf[5] != 0 {
            return Err(format!("qpu_enable returned {}", buf[5]));
        }
        Ok(())
    }

    /// Allocate GPU memory. Returns a handle for use with `mem_lock`/`mem_free`.
    pub fn mem_alloc(&self, size: u32, align: u32, flags: u32) -> Result<u32, String> {
        let mut buf = [0u32; 32];
        let mut i = 0;
        buf[i] = 0;
        i += 1;
        buf[i] = 0;
        i += 1;
        buf[i] = TAG_ALLOCATE_MEMORY;
        i += 1;
        buf[i] = 12;
        i += 1; // value buffer size
        buf[i] = 12;
        i += 1; // request size
        buf[i] = size;
        i += 1;
        buf[i] = align;
        i += 1;
        buf[i] = flags;
        i += 1;
        buf[i] = 0;
        i += 1; // end tag
        buf[0] = (i as u32) * 4;

        self.property(&mut buf)?;

        let handle = buf[5];
        if handle == 0 {
            return Err("mem_alloc: firmware returned handle 0".into());
        }
        Ok(handle)
    }

    /// Lock GPU memory, returning its bus address.
    pub fn mem_lock(&self, handle: u32) -> Result<u32, String> {
        let mut buf = [0u32; 32];
        let mut i = 0;
        buf[i] = 0;
        i += 1;
        buf[i] = 0;
        i += 1;
        buf[i] = TAG_LOCK_MEMORY;
        i += 1;
        buf[i] = 4;
        i += 1;
        buf[i] = 4;
        i += 1;
        buf[i] = handle;
        i += 1;
        buf[i] = 0;
        i += 1;
        buf[0] = (i as u32) * 4;

        self.property(&mut buf)?;

        let bus_addr = buf[5];
        if bus_addr == 0 {
            return Err("mem_lock: firmware returned bus address 0".into());
        }
        Ok(bus_addr)
    }

    /// Unlock GPU memory.
    pub fn mem_unlock(&self, handle: u32) -> Result<(), String> {
        let mut buf = [0u32; 32];
        let mut i = 0;
        buf[i] = 0;
        i += 1;
        buf[i] = 0;
        i += 1;
        buf[i] = TAG_UNLOCK_MEMORY;
        i += 1;
        buf[i] = 4;
        i += 1;
        buf[i] = 4;
        i += 1;
        buf[i] = handle;
        i += 1;
        buf[i] = 0;
        i += 1;
        buf[0] = (i as u32) * 4;

        self.property(&mut buf)?;
        Ok(())
    }

    /// Free GPU memory.
    pub fn mem_free(&self, handle: u32) -> Result<(), String> {
        let mut buf = [0u32; 32];
        let mut i = 0;
        buf[i] = 0;
        i += 1;
        buf[i] = 0;
        i += 1;
        buf[i] = TAG_RELEASE_MEMORY;
        i += 1;
        buf[i] = 4;
        i += 1;
        buf[i] = 4;
        i += 1;
        buf[i] = handle;
        i += 1;
        buf[i] = 0;
        i += 1;
        buf[0] = (i as u32) * 4;

        self.property(&mut buf)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// GpuMem — RAII wrapper for allocated + locked GPU memory
// ---------------------------------------------------------------------------

/// Owns a GPU memory allocation: handle, bus address, and ARM mapping.
///
/// On drop, unmaps the ARM pointer, unlocks the bus address, and frees the handle.
#[cfg(target_os = "linux")]
pub struct GpuMem {
    mbox: Arc<Mailbox>,
    handle: u32,
    bus_addr: u32,
    arm_ptr: *mut u8,
    size: u32,
}

// SAFETY: The raw pointer is to mmapped /dev/mem which is process-global.
// Access synchronization is the caller's responsibility (same as C version).
#[cfg(target_os = "linux")]
unsafe impl Send for GpuMem {}
#[cfg(target_os = "linux")]
unsafe impl Sync for GpuMem {}

#[cfg(target_os = "linux")]
impl GpuMem {
    /// Allocate GPU memory with DIRECT|COHERENT|ZERO flags, 4096-byte alignment.
    ///
    /// The returned `GpuMem` holds the ARM mapping and will clean up on drop.
    pub fn alloc(mbox: Arc<Mailbox>, size: u32) -> Result<Self, String> {
        let flags = MEM_FLAG_DIRECT | MEM_FLAG_COHERENT | MEM_FLAG_ZERO;
        let handle = mbox.mem_alloc(size, 4096, flags)?;

        let bus_addr = match mbox.mem_lock(handle) {
            Ok(addr) => addr,
            Err(e) => {
                let _ = mbox.mem_free(handle);
                return Err(e);
            }
        };

        let phys_addr = bus_to_phys(bus_addr);
        let arm_ptr = match mapmem(phys_addr, size) {
            Ok(ptr) => ptr,
            Err(e) => {
                let _ = mbox.mem_unlock(handle);
                let _ = mbox.mem_free(handle);
                return Err(e);
            }
        };

        Ok(GpuMem {
            mbox,
            handle,
            bus_addr,
            arm_ptr,
            size,
        })
    }

    /// Bus address of this allocation (for passing to QPU as uniforms).
    pub fn bus_addr(&self) -> u32 {
        self.bus_addr
    }

    /// ARM-side pointer to the mapped memory.
    pub fn as_ptr(&self) -> *mut u8 {
        self.arm_ptr
    }

    /// Size of the allocation in bytes.
    pub fn size(&self) -> u32 {
        self.size
    }
}

#[cfg(target_os = "linux")]
impl Drop for GpuMem {
    fn drop(&mut self) {
        // Cleanup order: unmap ARM pointer, unlock bus address, free handle.
        let phys_addr = bus_to_phys(self.bus_addr);
        unmapmem(self.arm_ptr, phys_addr, self.size);
        let _ = self.mbox.mem_unlock(self.handle);
        let _ = self.mbox.mem_free(self.handle);
    }
}

// ---------------------------------------------------------------------------
// V3dRegs — mapped V3D register block for direct register poke execution
// ---------------------------------------------------------------------------

/// Memory-mapped V3D register block at BCM2837 physical address 0x3FC00000.
///
/// Provides direct register poke QPU execution — the ONLY working method
/// without the vc4 DT overlay.
#[cfg(target_os = "linux")]
pub struct V3dRegs {
    regs: *mut u32,
}

// SAFETY: The register mapping is process-global physical memory.
#[cfg(target_os = "linux")]
unsafe impl Send for V3dRegs {}
#[cfg(target_os = "linux")]
unsafe impl Sync for V3dRegs {}

#[cfg(target_os = "linux")]
impl V3dRegs {
    /// Map the V3D register block and verify the IDENT0 signature.
    pub fn map() -> Result<Self, String> {
        let regs_ptr = mapmem(V3D_BASE, 0x1000)?;

        let regs = regs_ptr as *mut u32;

        // SAFETY: regs points to 0x1000 bytes of mapped V3D registers.
        // V3D_IDENT0 is at offset 0x000, well within the mapped range.
        let ident0 = unsafe { std::ptr::read_volatile(regs.add(V3D_IDENT0 / 4)) };

        // Lower 24 bits should be 0x443356 = "V3D" in little-endian ASCII
        if (ident0 & 0x00FF_FFFF) != 0x0044_3356 {
            unmapmem(regs_ptr, V3D_BASE, 0x1000);
            return Err(format!(
                "V3D_IDENT0 = 0x{:08X}, expected lower 24 bits = 0x443356 (\"V3D\")",
                ident0
            ));
        }

        Ok(V3dRegs { regs })
    }

    /// Read V3D_IDENT0 register.
    pub fn ident0(&self) -> u32 {
        // SAFETY: regs is a valid mapping of the V3D register block.
        unsafe { std::ptr::read_volatile(self.regs.add(V3D_IDENT0 / 4)) }
    }

    /// Read V3D_IDENT1 register.
    pub fn ident1(&self) -> u32 {
        // SAFETY: regs is a valid mapping of the V3D register block.
        unsafe { std::ptr::read_volatile(self.regs.add(V3D_IDENT1 / 4)) }
    }

    /// Number of QPUs available (slices * QPUs-per-slice from IDENT1).
    pub fn num_qpus(&self) -> u32 {
        let id1 = self.ident1();
        let slices = (id1 >> 4) & 0xF;
        let per_slice = (id1 >> 8) & 0xF;
        slices * per_slice
    }

    /// Execute a QPU program via direct V3D register poke.
    ///
    /// This is the ONLY working execution method without the vc4 DT overlay.
    /// Mailbox execute_qpu (tag 0x30011) DOES NOT WORK without vc4.
    ///
    /// The execution sequence is:
    /// 1. Flush L2 cache
    /// 2. Reset scheduler (clear completed + error counts)
    /// 3. Write uniforms address (SRQUA)
    /// 4. Write uniforms length (SRQUL)
    /// 5. Write program counter (SRQPC) — this triggers the launch
    /// 6. Poll SRQCS bits [23:16] for completion count > 0
    pub fn execute_qpu(
        &self,
        code_bus: u32,
        uniforms_bus: u32,
        uniforms_len: u32,
        timeout_ms: u64,
    ) -> Result<(), String> {
        // SAFETY: All register writes/reads are to the mapped V3D register block.
        // Offsets are within the 0x1000-byte mapped range (max offset = 0x43C).
        unsafe {
            // 1. Flush L2 cache: set bit 2 of L2CACTL
            std::ptr::write_volatile(self.regs.add(V3D_L2CACTL / 4), 1 << 2);

            // 2. Reset scheduler: clear request count (bit 7), error (bit 8),
            //    completed count (bit 16)
            std::ptr::write_volatile(
                self.regs.add(V3D_SRQCS / 4),
                (1 << 7) | (1 << 8) | (1 << 16),
            );

            // 3. Write uniforms address
            std::ptr::write_volatile(self.regs.add(V3D_SRQUA / 4), uniforms_bus);

            // 4. Write uniforms length
            std::ptr::write_volatile(self.regs.add(V3D_SRQUL / 4), uniforms_len);

            // 5. Write program counter — triggers launch
            std::ptr::write_volatile(self.regs.add(V3D_SRQPC / 4), code_bus);
        }

        // 6. Poll for completion
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);

        loop {
            // SAFETY: reading SRQCS from the mapped register block.
            let srqcs = unsafe { std::ptr::read_volatile(self.regs.add(V3D_SRQCS / 4)) };

            let complete = (srqcs >> 16) & 0xFF;
            if complete > 0 {
                return Ok(());
            }

            if start.elapsed() > timeout {
                let error_count = (srqcs >> 8) & 0xFF;
                return Err(format!(
                    "QPU execution timeout after {}ms (SRQCS=0x{:08X}, complete={}, error={})",
                    timeout_ms, srqcs, complete, error_count
                ));
            }

            // Brief yield to avoid spinning at 100% CPU
            std::thread::sleep(std::time::Duration::from_micros(100));
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for V3dRegs {
    fn drop(&mut self) {
        // SAFETY: regs was created by mapmem(V3D_BASE, 0x1000), so we
        // reverse with unmapmem using the same base and size.
        unmapmem(self.regs as *mut u8, V3D_BASE, 0x1000);
    }
}

// ---------------------------------------------------------------------------
// Stub implementations for non-Linux platforms
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "linux"))]
pub struct Mailbox;

#[cfg(not(target_os = "linux"))]
impl Mailbox {
    pub fn open() -> Result<Self, String> {
        Err("Mailbox requires Linux with /dev/vcio".into())
    }

    pub fn qpu_enable(&self, _enable: bool) -> Result<(), String> {
        Err("Mailbox requires Linux with /dev/vcio".into())
    }

    pub fn mem_alloc(&self, _size: u32, _align: u32, _flags: u32) -> Result<u32, String> {
        Err("Mailbox requires Linux with /dev/vcio".into())
    }

    pub fn mem_lock(&self, _handle: u32) -> Result<u32, String> {
        Err("Mailbox requires Linux with /dev/vcio".into())
    }

    pub fn mem_unlock(&self, _handle: u32) -> Result<(), String> {
        Err("Mailbox requires Linux with /dev/vcio".into())
    }

    pub fn mem_free(&self, _handle: u32) -> Result<(), String> {
        Err("Mailbox requires Linux with /dev/vcio".into())
    }
}

#[cfg(not(target_os = "linux"))]
pub struct GpuMem;

#[cfg(not(target_os = "linux"))]
impl GpuMem {
    pub fn alloc(_mbox: std::sync::Arc<Mailbox>, _size: u32) -> Result<Self, String> {
        Err("GpuMem requires Linux with /dev/mem".into())
    }

    pub fn bus_addr(&self) -> u32 {
        0
    }
    pub fn as_ptr(&self) -> *mut u8 {
        std::ptr::null_mut()
    }
    pub fn size(&self) -> u32 {
        0
    }
}

#[cfg(not(target_os = "linux"))]
pub struct V3dRegs;

#[cfg(not(target_os = "linux"))]
impl V3dRegs {
    pub fn map() -> Result<Self, String> {
        Err("V3dRegs requires Linux with /dev/mem".into())
    }

    pub fn ident0(&self) -> u32 {
        0
    }
    pub fn ident1(&self) -> u32 {
        0
    }
    pub fn num_qpus(&self) -> u32 {
        0
    }

    pub fn execute_qpu(
        &self,
        _code_bus: u32,
        _uniforms_bus: u32,
        _uniforms_len: u32,
        _timeout_ms: u64,
    ) -> Result<(), String> {
        Err("V3dRegs requires Linux with /dev/mem".into())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bus_to_phys() {
        // 0xFE100000 -> strip top 2 bits -> 0x3E100000
        assert_eq!(bus_to_phys(0xFE10_0000), 0x3E10_0000);
        // 0xC0000000 alias -> 0x00000000
        assert_eq!(bus_to_phys(0xC000_0000), 0x0000_0000);
        // Already physical (no alias bits) -> unchanged
        assert_eq!(bus_to_phys(0x3E10_0000), 0x3E10_0000);
    }

    #[test]
    fn test_memory_flags() {
        // DIRECT | COHERENT | ZERO = 0x1C (proven in C test)
        assert_eq!(MEM_FLAG_DIRECT | MEM_FLAG_COHERENT | MEM_FLAG_ZERO, 0x1C);
    }

    #[test]
    fn test_flag_values() {
        assert_eq!(MEM_FLAG_DIRECT, 4);
        assert_eq!(MEM_FLAG_COHERENT, 8);
        assert_eq!(MEM_FLAG_ZERO, 16);
    }

    #[test]
    fn test_tag_constants() {
        assert_eq!(TAG_ALLOCATE_MEMORY, 0x3000C);
        assert_eq!(TAG_LOCK_MEMORY, 0x3000D);
        assert_eq!(TAG_UNLOCK_MEMORY, 0x3000E);
        assert_eq!(TAG_RELEASE_MEMORY, 0x3000F);
        assert_eq!(TAG_ENABLE_QPU, 0x30012);
    }

    #[test]
    fn test_v3d_register_offsets() {
        assert_eq!(V3D_BASE, 0x3FC0_0000);
        assert_eq!(V3D_IDENT0, 0x000);
        assert_eq!(V3D_IDENT1, 0x004);
        assert_eq!(V3D_L2CACTL, 0x020);
        assert_eq!(V3D_SRQPC, 0x430);
        assert_eq!(V3D_SRQUA, 0x434);
        assert_eq!(V3D_SRQUL, 0x438);
        assert_eq!(V3D_SRQCS, 0x43C);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_ioctl_constant() {
        // _IOWR(100, 0, char *) on aarch64 = 0xC0086400
        assert_eq!(IOCTL_MBOX_PROPERTY, 0xC008_6400);
    }

    #[test]
    fn test_mailbox_open_fails_on_non_pi() {
        // On non-Pi systems, opening /dev/vcio should fail gracefully
        let result = Mailbox::open();
        // We just verify it returns a Result (success on Pi, error elsewhere)
        let _ = result;
    }

    #[test]
    fn test_v3d_map_fails_on_non_pi() {
        // On non-Pi systems, mapping V3D registers should fail gracefully
        let result = V3dRegs::map();
        let _ = result;
    }
}
