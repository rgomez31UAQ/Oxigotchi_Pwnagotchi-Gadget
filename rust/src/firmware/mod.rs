/// Native Rust client for nexmon SDIO RAMRW (netlink family 31, cmd 0x500).
/// Reads/writes BCM43436B0 firmware RAM without external binaries.

const NETLINK_NEXMON: i32 = 31;
const CMD_SDIO_RAMRW: u32 = 0x500;

/// Build a nexmon netlink frame for SDIO RAMRW.
///
/// Frame layout:
/// [nlmsghdr 16B] [nexudp "NEX\0"+cookie 8B] [ioctl cmd+set 8B] [payload]
fn build_netlink_frame(cmd: u32, set: bool, payload: &[u8]) -> Vec<u8> {
    let total_len: u32 = 16 + 8 + 8 + payload.len() as u32;
    let mut frame = Vec::with_capacity(total_len as usize);

    // nlmsghdr (16 bytes, all little-endian)
    frame.extend_from_slice(&total_len.to_le_bytes()); // nlmsg_len
    frame.extend_from_slice(&0u16.to_le_bytes()); // nlmsg_type
    frame.extend_from_slice(&0u16.to_le_bytes()); // nlmsg_flags
    frame.extend_from_slice(&0u32.to_le_bytes()); // nlmsg_seq
    frame.extend_from_slice(&0u32.to_le_bytes()); // nlmsg_pid

    // nexudp_header (8 bytes)
    frame.extend_from_slice(b"NEX"); // magic
    frame.push(0); // type = NEXUDP_IOCTL
    frame.extend_from_slice(&0u32.to_le_bytes()); // security cookie

    // ioctl_header (8 bytes)
    frame.extend_from_slice(&cmd.to_le_bytes()); // cmd
    frame.extend_from_slice(&(set as u32).to_le_bytes()); // set flag

    // payload
    frame.extend_from_slice(payload);

    frame
}

/// Build payload for a READ operation: addr + length.
fn build_read_payload(addr: u32, length: u32) -> Vec<u8> {
    let mut p = Vec::with_capacity(8);
    p.extend_from_slice(&addr.to_le_bytes());
    p.extend_from_slice(&length.to_le_bytes());
    p
}

/// Build payload for a WRITE operation: addr + data.
fn build_write_payload(addr: u32, data: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(4 + data.len());
    p.extend_from_slice(&addr.to_le_bytes());
    p.extend_from_slice(data);
    p
}

/// Read `length` bytes from firmware RAM at `addr` via SDIO RAMRW netlink.
#[cfg(target_os = "linux")]
pub fn sdio_read(addr: u32, length: u32) -> Result<Vec<u8>, String> {
    let payload = build_read_payload(addr, length);
    let frame = build_netlink_frame(CMD_SDIO_RAMRW, false, &payload);

    unsafe {
        let fd = libc::socket(libc::AF_NETLINK, libc::SOCK_RAW, NETLINK_NEXMON);
        if fd < 0 {
            return Err(format!(
                "netlink socket failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        // Bind
        let mut sa: libc::sockaddr_nl = std::mem::zeroed();
        sa.nl_family = libc::AF_NETLINK as u16;
        if libc::bind(
            fd,
            &sa as *const _ as *const libc::sockaddr,
            std::mem::size_of::<libc::sockaddr_nl>() as u32,
        ) < 0
        {
            libc::close(fd);
            return Err(format!(
                "netlink bind failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        // Set 3s timeout
        let tv = libc::timeval {
            tv_sec: 3,
            tv_usec: 0,
        };
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            &tv as *const _ as *const libc::c_void,
            std::mem::size_of::<libc::timeval>() as u32,
        );

        // Send
        let sent = libc::send(fd, frame.as_ptr() as *const libc::c_void, frame.len(), 0);
        if sent < 0 {
            libc::close(fd);
            return Err(format!(
                "netlink send failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        // Receive
        let mut resp = vec![0u8; 4096];
        let n = libc::recv(fd, resp.as_mut_ptr() as *mut libc::c_void, resp.len(), 0);
        libc::close(fd);

        if n < 0 {
            return Err(format!(
                "netlink recv failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        let n = n as usize;
        if n < 16 + length as usize {
            return Err(format!(
                "short response: got {n} bytes, need {}",
                16 + length
            ));
        }

        Ok(resp[16..16 + length as usize].to_vec())
    }
}

#[cfg(not(target_os = "linux"))]
pub fn sdio_read(_addr: u32, length: u32) -> Result<Vec<u8>, String> {
    Ok(vec![0u8; length as usize])
}

/// Write `data` to firmware RAM at `addr` via SDIO RAMRW netlink.
#[cfg(target_os = "linux")]
pub fn sdio_write(addr: u32, data: &[u8]) -> Result<(), String> {
    let payload = build_write_payload(addr, data);
    let frame = build_netlink_frame(CMD_SDIO_RAMRW, true, &payload);

    unsafe {
        let fd = libc::socket(libc::AF_NETLINK, libc::SOCK_RAW, NETLINK_NEXMON);
        if fd < 0 {
            return Err(format!(
                "netlink socket failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        let mut sa: libc::sockaddr_nl = std::mem::zeroed();
        sa.nl_family = libc::AF_NETLINK as u16;
        if libc::bind(
            fd,
            &sa as *const _ as *const libc::sockaddr,
            std::mem::size_of::<libc::sockaddr_nl>() as u32,
        ) < 0
        {
            libc::close(fd);
            return Err(format!(
                "netlink bind failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        let tv = libc::timeval {
            tv_sec: 3,
            tv_usec: 0,
        };
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            &tv as *const _ as *const libc::c_void,
            std::mem::size_of::<libc::timeval>() as u32,
        );

        let sent = libc::send(fd, frame.as_ptr() as *const libc::c_void, frame.len(), 0);
        libc::close(fd);

        if sent < 0 {
            return Err(format!(
                "netlink send failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        Ok(()) // SET operations: timeout on recv is normal (= success)
    }
}

#[cfg(not(target_os = "linux"))]
pub fn sdio_write(_addr: u32, _data: &[u8]) -> Result<(), String> {
    Ok(())
}

/// Firmware RAM addresses for crash counters — must be provided via firmware config.
pub const ADDR_CRASH_SUPPRESS: u32 = 0; // TODO: load from firmware config
pub const ADDR_HARDFAULT: u32 = 0;      // TODO: load from firmware config

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirmwareHealth {
    /// Counters stable — firmware is healthy.
    Healthy,
    /// Counters increasing slowly (1-3 increments) — firmware under stress.
    Degraded,
    /// Counters spiking (4+ total increments) — crash imminent, trigger recovery.
    Critical,
    /// Could not read counters (nexmon not available).
    Unknown,
}

pub struct FirmwareMonitor {
    prev_crash_suppress: u32,
    prev_hardfault: u32,
    pub crash_suppress: u32,
    pub hardfault: u32,
    health: FirmwareHealth,
    initialized: bool,
}

impl FirmwareMonitor {
    pub fn new() -> Self {
        Self {
            prev_crash_suppress: 0,
            prev_hardfault: 0,
            crash_suppress: 0,
            hardfault: 0,
            health: FirmwareHealth::Unknown,
            initialized: false,
        }
    }

    /// Update counters from raw values (used by tests and by poll()).
    pub fn update_counters(&mut self, crash_suppress: u32, hardfault: u32) {
        if !self.initialized {
            self.prev_crash_suppress = crash_suppress;
            self.prev_hardfault = hardfault;
            self.crash_suppress = crash_suppress;
            self.hardfault = hardfault;
            self.initialized = true;
            self.health = FirmwareHealth::Healthy;
            return;
        }

        self.prev_crash_suppress = self.crash_suppress;
        self.prev_hardfault = self.hardfault;
        self.crash_suppress = crash_suppress;
        self.hardfault = hardfault;

        let delta_crash = crash_suppress.saturating_sub(self.prev_crash_suppress);
        let delta_fault = hardfault.saturating_sub(self.prev_hardfault);
        let total_delta = delta_crash + delta_fault;

        self.health = if total_delta == 0 {
            FirmwareHealth::Healthy
        } else if total_delta <= 3 {
            FirmwareHealth::Degraded
        } else {
            FirmwareHealth::Critical
        };
    }

    /// Poll firmware counters via SDIO RAMRW. Returns health status.
    pub fn poll(&mut self) -> FirmwareHealth {
        let crash = match sdio_read(ADDR_CRASH_SUPPRESS, 4) {
            Ok(data) if data.len() == 4 => u32::from_le_bytes(data[..4].try_into().unwrap()),
            _ => {
                self.health = FirmwareHealth::Unknown;
                return self.health;
            }
        };
        let fault = match sdio_read(ADDR_HARDFAULT, 4) {
            Ok(data) if data.len() == 4 => u32::from_le_bytes(data[..4].try_into().unwrap()),
            _ => {
                self.health = FirmwareHealth::Unknown;
                return self.health;
            }
        };
        self.update_counters(crash, fault);
        self.health
    }

    pub fn health(&self) -> FirmwareHealth {
        self.health
    }

    /// Reset firmware counters to zero (write 4 zero bytes to each address).
    pub fn reset_counters(&mut self) -> Result<(), String> {
        sdio_write(ADDR_CRASH_SUPPRESS, &[0, 0, 0, 0])?;
        sdio_write(ADDR_HARDFAULT, &[0, 0, 0, 0])?;
        self.prev_crash_suppress = 0;
        self.prev_hardfault = 0;
        self.crash_suppress = 0;
        self.hardfault = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_frame_layout() {
        let test_addr: u32 = 0x1000; // arbitrary test address
        let payload = build_read_payload(test_addr, 4);
        let frame = build_netlink_frame(CMD_SDIO_RAMRW, false, &payload);

        assert_eq!(frame.len(), 40); // 16 + 8 + 8 + 8
        // nlmsg_len
        assert_eq!(u32::from_le_bytes(frame[0..4].try_into().unwrap()), 40);
        // NEX magic
        assert_eq!(&frame[16..19], b"NEX");
        // cmd = 0x500
        assert_eq!(u32::from_le_bytes(frame[24..28].try_into().unwrap()), 0x500);
        // set = 0 (GET)
        assert_eq!(u32::from_le_bytes(frame[28..32].try_into().unwrap()), 0);
        // addr
        assert_eq!(
            u32::from_le_bytes(frame[32..36].try_into().unwrap()),
            test_addr
        );
        // length = 4
        assert_eq!(u32::from_le_bytes(frame[36..40].try_into().unwrap()), 4);
    }

    #[test]
    fn test_write_frame_layout() {
        let test_addr: u32 = 0x1000; // arbitrary test address
        let payload = build_write_payload(test_addr, &[0, 0, 0, 0]);
        let frame = build_netlink_frame(CMD_SDIO_RAMRW, true, &payload);

        assert_eq!(frame.len(), 40); // 16 + 8 + 8 + 8
        // set = 1 (SET)
        assert_eq!(u32::from_le_bytes(frame[28..32].try_into().unwrap()), 1);
    }

    #[test]
    fn test_health_assessment_healthy() {
        let mut mon = FirmwareMonitor::new();
        // First check sets baseline
        mon.update_counters(5, 2);
        assert!(matches!(mon.health(), FirmwareHealth::Healthy));
        // Same counters = healthy
        mon.update_counters(5, 2);
        assert!(matches!(mon.health(), FirmwareHealth::Healthy));
    }

    #[test]
    fn test_health_assessment_degraded() {
        let mut mon = FirmwareMonitor::new();
        mon.update_counters(5, 2);
        mon.update_counters(7, 2); // +2 crash_suppress
        assert!(matches!(mon.health(), FirmwareHealth::Degraded));
    }

    #[test]
    fn test_health_assessment_critical() {
        let mut mon = FirmwareMonitor::new();
        mon.update_counters(5, 2);
        mon.update_counters(9, 5); // +4 crash_suppress, +3 hardfault
        assert!(matches!(mon.health(), FirmwareHealth::Critical));
    }
}
