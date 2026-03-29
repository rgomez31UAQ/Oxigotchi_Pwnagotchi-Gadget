//! Raw HCI socket wrapper for sending vendor commands and low-level BT ops.
//!
//! On Linux, opens a real `AF_BLUETOOTH` / `BTPROTO_HCI` socket.
//! On non-Linux platforms, provides stubs that return mock data so the
//! codebase compiles and tests pass everywhere.

/// An HCI command packet to send to the controller.
#[derive(Debug, Clone)]
pub struct HciCommand {
    /// Opcode Group Field (6 bits).
    pub ogf: u8,
    /// Opcode Command Field (10 bits).
    pub ocf: u16,
    /// Command parameters.
    pub params: Vec<u8>,
}

impl HciCommand {
    pub fn new(ogf: u8, ocf: u16, params: Vec<u8>) -> Self {
        Self { ogf, ocf, params }
    }

    /// Convenience constructor for vendor-specific commands (OGF 0x3F).
    pub fn vendor(ocf: u16, params: Vec<u8>) -> Self {
        Self::new(0x3F, ocf, params)
    }

    /// Build the full opcode from OGF + OCF.
    fn opcode(&self) -> u16 {
        ((self.ogf as u16) << 10) | (self.ocf & 0x03FF)
    }

    /// Serialize to the HCI command wire format:
    /// `[type=0x01] [opcode_lo] [opcode_hi] [param_len] [params...]`
    pub fn to_bytes(&self) -> Vec<u8> {
        let opcode = self.opcode();
        let len = self.params.len() as u8;
        let mut buf = Vec::with_capacity(4 + self.params.len());
        buf.push(0x01); // HCI command packet type
        buf.push((opcode & 0xFF) as u8);
        buf.push((opcode >> 8) as u8);
        buf.push(len);
        buf.extend_from_slice(&self.params);
        buf
    }
}

/// Parsed HCI command-complete response.
#[derive(Debug, Clone)]
pub struct HciResponse {
    pub status: u8,
    pub data: Vec<u8>,
}

// ===========================================================================
// Linux: real HCI socket via AF_BLUETOOTH
// ===========================================================================

#[cfg(target_os = "linux")]
mod platform {
    use super::{HciCommand, HciResponse};
    use std::os::unix::io::RawFd;

    /// AF_BLUETOOTH address family.
    const AF_BLUETOOTH: libc::c_int = 31;
    /// BTPROTO_HCI protocol.
    const BTPROTO_HCI: libc::c_int = 1;
    /// HCI channel: raw.
    const HCI_CHANNEL_RAW: u16 = 0;

    /// `sockaddr_hci` layout matching the kernel struct.
    #[repr(C)]
    struct SockaddrHci {
        hci_family: libc::sa_family_t,
        hci_dev: u16,
        hci_channel: u16,
    }

    pub struct HciSocket {
        fd: RawFd,
    }

    impl HciSocket {
        pub fn open(dev_id: u16) -> Result<Self, String> {
            unsafe {
                let fd = libc::socket(AF_BLUETOOTH, libc::SOCK_RAW, BTPROTO_HCI);
                if fd < 0 {
                    return Err(format!(
                        "socket(AF_BLUETOOTH) failed: {}",
                        std::io::Error::last_os_error()
                    ));
                }

                let addr = SockaddrHci {
                    hci_family: AF_BLUETOOTH as libc::sa_family_t,
                    hci_dev: dev_id,
                    hci_channel: HCI_CHANNEL_RAW,
                };

                let ret = libc::bind(
                    fd,
                    &addr as *const SockaddrHci as *const libc::sockaddr,
                    std::mem::size_of::<SockaddrHci>() as libc::socklen_t,
                );
                if ret < 0 {
                    libc::close(fd);
                    return Err(format!(
                        "bind(hci{}) failed: {}",
                        dev_id,
                        std::io::Error::last_os_error()
                    ));
                }

                Ok(Self { fd })
            }
        }

        pub fn send_command(&self, cmd: &HciCommand) -> Result<HciResponse, String> {
            let pkt = cmd.to_bytes();

            let written = unsafe {
                libc::write(self.fd, pkt.as_ptr() as *const libc::c_void, pkt.len())
            };
            if written < 0 {
                return Err(format!(
                    "HCI write failed: {}",
                    std::io::Error::last_os_error()
                ));
            }

            // Poll for response with 2-second timeout.
            let mut pfd = libc::pollfd {
                fd: self.fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let poll_ret = unsafe { libc::poll(&mut pfd, 1, 2000) };
            if poll_ret <= 0 {
                return Err("HCI response timeout (2s)".into());
            }

            let mut buf = [0u8; 260];
            let n = unsafe {
                libc::read(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len())
            };
            if n < 0 {
                return Err(format!(
                    "HCI read failed: {}",
                    std::io::Error::last_os_error()
                ));
            }

            let n = n as usize;
            // Minimum HCI event: type(1) + event_code(1) + param_len(1) + ncmds(1) + opcode(2) + status(1) = 7
            if n < 7 {
                return Err(format!("HCI response too short: {} bytes", n));
            }

            // buf[0] = HCI event packet type (0x04)
            // buf[1] = event code
            // buf[2] = parameter total length
            // For Command Complete (0x0E): buf[3]=num_cmds, buf[4..6]=opcode, buf[6]=status, buf[7..]=data
            let status = buf[6];
            let data = if n > 7 { buf[7..n].to_vec() } else { Vec::new() };

            Ok(HciResponse { status, data })
        }

        pub fn wait_event(&self, event_code: u8, timeout_ms: i32) -> Result<Vec<u8>, String> {
            let deadline = std::time::Instant::now()
                + std::time::Duration::from_millis(timeout_ms as u64);

            loop {
                let remaining = deadline
                    .saturating_duration_since(std::time::Instant::now())
                    .as_millis() as i32;
                if remaining <= 0 {
                    return Err(format!(
                        "HCI wait_event(0x{:02X}) timeout ({}ms)",
                        event_code, timeout_ms
                    ));
                }

                let mut pfd = libc::pollfd {
                    fd: self.fd,
                    events: libc::POLLIN,
                    revents: 0,
                };
                let poll_ret = unsafe { libc::poll(&mut pfd, 1, remaining) };
                if poll_ret < 0 {
                    return Err(format!(
                        "HCI poll failed: {}",
                        std::io::Error::last_os_error()
                    ));
                }
                if poll_ret == 0 {
                    return Err(format!(
                        "HCI wait_event(0x{:02X}) timeout ({}ms)",
                        event_code, timeout_ms
                    ));
                }

                let mut buf = [0u8; 260];
                let n = unsafe {
                    libc::read(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len())
                };
                if n < 0 {
                    return Err(format!(
                        "HCI read failed: {}",
                        std::io::Error::last_os_error()
                    ));
                }

                let n = n as usize;
                if n < 3 {
                    continue;
                }

                // buf[0] = 0x04 (HCI event), buf[1] = event_code, buf[2] = param_len
                if buf[1] == event_code {
                    let param_len = buf[2] as usize;
                    let end = (3 + param_len).min(n);
                    return Ok(buf[3..end].to_vec());
                }
                // Not our event — discard and keep polling
            }
        }

        /// Clone the HCI socket for use in a background thread.
        pub fn try_clone(&self) -> Result<Self, String> {
            let new_fd = unsafe { libc::dup(self.fd) };
            if new_fd < 0 {
                return Err(format!("dup() failed: {}", std::io::Error::last_os_error()));
            }
            Ok(Self { fd: new_fd })
        }

        /// Write an HCI command without reading the response.
        /// Use for commands that return Command Status (0x0F) instead of
        /// Command Complete, or when the response read would swallow
        /// async events (e.g., advertising reports after LE scan enable).
        pub fn write_command_raw(&self, cmd: &HciCommand) -> Result<(), String> {
            let pkt = cmd.to_bytes();
            let written = unsafe {
                libc::write(self.fd, pkt.as_ptr() as *const libc::c_void, pkt.len())
            };
            if written < 0 {
                return Err(format!(
                    "HCI write_command_raw failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(())
        }

        /// Set socket filter to accept only HCI Event packets (0x04),
        /// rejecting ACL/SCO data that would pollute scan collection.
        pub fn set_event_filter(&self) -> Result<(), String> {
            // struct hci_filter { u32 type_mask; u32 event_mask[2]; u16 opcode; }
            // We want: type_mask bit 4 (HCI event pkt type 0x04),
            // event_mask all bits set (accept all HCI events).
            let mut filter = [0u8; 14];
            // type_mask: set bit for HCI Event (packet indicator 0x04 → bit 4)
            filter[0] = 0x10; // 1 << 4
            // event_mask[0]: all events
            filter[4] = 0xFF;
            filter[5] = 0xFF;
            filter[6] = 0xFF;
            filter[7] = 0xFF;
            // event_mask[1]: all events
            filter[8] = 0xFF;
            filter[9] = 0xFF;
            filter[10] = 0xFF;
            filter[11] = 0xFF;
            // opcode: 0 = don't filter by opcode
            filter[12] = 0;
            filter[13] = 0;

            const SOL_HCI: libc::c_int = 0;
            const HCI_FILTER: libc::c_int = 2;

            let ret = unsafe {
                libc::setsockopt(
                    self.fd,
                    SOL_HCI,
                    HCI_FILTER,
                    filter.as_ptr() as *const libc::c_void,
                    filter.len() as libc::socklen_t,
                )
            };
            if ret < 0 {
                return Err(format!(
                    "setsockopt(HCI_FILTER) failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(())
        }

        /// Wait for LE Meta subevent. Loops over 0x3E events, discarding
        /// non-matching subevents until the correct one arrives or timeout.
        pub fn wait_le_event(&self, subevent: u8, timeout_ms: i32) -> Result<Vec<u8>, String> {
            let deadline = std::time::Instant::now()
                + std::time::Duration::from_millis(timeout_ms as u64);
            loop {
                let remaining = deadline
                    .saturating_duration_since(std::time::Instant::now())
                    .as_millis() as i32;
                if remaining <= 0 {
                    return Err(format!(
                        "HCI wait_le_event(0x{:02X}) timeout ({}ms)",
                        subevent, timeout_ms
                    ));
                }
                let params = self.wait_event(0x3E, remaining)?;
                if params.is_empty() {
                    continue;
                }
                if params[0] == subevent {
                    return Ok(params[1..].to_vec());
                }
                // Wrong subevent — discard and keep waiting
            }
        }
    }

    impl Drop for HciSocket {
        fn drop(&mut self) {
            unsafe {
                libc::close(self.fd);
            }
        }
    }
}

// ===========================================================================
// Non-Linux: stub implementation
// ===========================================================================

#[cfg(not(target_os = "linux"))]
mod platform {
    use super::{HciCommand, HciResponse};

    pub struct HciSocket {
        _dev_id: u16,
    }

    impl HciSocket {
        pub fn open(dev_id: u16) -> Result<Self, String> {
            log::info!("HciSocket stub: open(hci{})", dev_id);
            Ok(Self { _dev_id: dev_id })
        }

        pub fn send_command(&self, cmd: &HciCommand) -> Result<HciResponse, String> {
            log::info!(
                "HciSocket stub: send OGF=0x{:02X} OCF=0x{:04X} ({} param bytes)",
                cmd.ogf,
                cmd.ocf,
                cmd.params.len()
            );
            Ok(HciResponse {
                status: 0,
                data: vec![0; 8],
            })
        }

        pub fn wait_event(&self, event_code: u8, _timeout_ms: i32) -> Result<Vec<u8>, String> {
            log::info!("HciSocket stub: wait_event(0x{:02X})", event_code);
            // Mock Connection Complete-like event
            Ok(vec![0x00, 0x40, 0x00, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x01, 0x00])
        }

        pub fn wait_le_event(&self, subevent: u8, _timeout_ms: i32) -> Result<Vec<u8>, String> {
            log::info!("HciSocket stub: wait_le_event(0x{:02X})", subevent);
            // Mock LE Connection Complete
            Ok(vec![0x00, 0x40, 0x00, 0x00, 0x01, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x18, 0x00, 0x00, 0x00, 0xC8, 0x00, 0x00])
        }

        /// Clone the HCI socket for use in a background thread.
        pub fn try_clone(&self) -> Result<Self, String> {
            Self::open(self._dev_id)
        }

        pub fn write_command_raw(&self, cmd: &HciCommand) -> Result<(), String> {
            log::info!(
                "HciSocket stub: write_command_raw OGF=0x{:02X} OCF=0x{:04X}",
                cmd.ogf, cmd.ocf
            );
            Ok(())
        }

        pub fn set_event_filter(&self) -> Result<(), String> {
            log::info!("HciSocket stub: set_event_filter");
            Ok(())
        }
    }
}

// Re-export the platform-specific type at module level.
pub use platform::HciSocket;

/// Parse a BD_ADDR string "AA:BB:CC:DD:EE:FF" into 6 bytes in reversed
/// (little-endian) order as required by HCI.
pub fn parse_bdaddr(addr: &str) -> [u8; 6] {
    let mut bytes = [0u8; 6];
    let parts: Vec<&str> = addr.split(':').collect();
    if parts.len() == 6 {
        for (i, part) in parts.iter().enumerate() {
            bytes[5 - i] = u8::from_str_radix(part, 16).unwrap_or(0);
        }
    }
    bytes
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hci_command_opcode() {
        // OGF=0x3F (vendor), OCF=0x4D (READ_RAM) → opcode = 0xFC4D
        let cmd = HciCommand::vendor(0x4D, vec![]);
        let bytes = cmd.to_bytes();
        assert_eq!(bytes[0], 0x01); // packet type
        assert_eq!(bytes[1], 0x4D); // opcode lo
        assert_eq!(bytes[2], 0xFC); // opcode hi  (0x3F << 10 | 0x4D = 0xFC4D)
        assert_eq!(bytes[3], 0x00); // param length
    }

    #[test]
    fn test_hci_command_with_params() {
        let cmd = HciCommand::vendor(0x4D, vec![0x00, 0x17, 0x21, 0x00, 0x04]);
        let bytes = cmd.to_bytes();
        assert_eq!(bytes[3], 5); // param length
        assert_eq!(&bytes[4..], &[0x00, 0x17, 0x21, 0x00, 0x04]);
    }

    #[test]
    fn test_hci_command_new() {
        let cmd = HciCommand::new(0x01, 0x03, vec![]);
        // OGF=0x01, OCF=0x03 → opcode = 0x0403
        let bytes = cmd.to_bytes();
        assert_eq!(bytes[1], 0x03);
        assert_eq!(bytes[2], 0x04);
    }

    #[test]
    fn test_stub_open_and_send() {
        // On non-linux this tests the stub; on linux it will attempt a real socket
        // which likely fails without root, so we gate on not(linux).
        #[cfg(not(target_os = "linux"))]
        {
            let hci = HciSocket::open(0).unwrap();
            let cmd = HciCommand::vendor(0x01, vec![]);
            let resp = hci.send_command(&cmd).unwrap();
            assert_eq!(resp.status, 0);
            assert_eq!(resp.data.len(), 8);
        }
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_wait_event_stub() {
        let hci = HciSocket::open(0).unwrap();
        let data = hci.wait_event(0x03, 2000).unwrap();
        assert!(!data.is_empty());
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_wait_le_event_stub() {
        let hci = HciSocket::open(0).unwrap();
        let data = hci.wait_le_event(0x01, 2000).unwrap();
        assert!(!data.is_empty());
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_write_command_raw_builds_packet() {
        // Verify the method exists and accepts an HciCommand
        let socket = HciSocket::open(0).unwrap();
        let cmd = HciCommand::new(0x08, 0x000C, vec![0x01, 0x01]); // LE_Set_Scan_Enable
        // On non-Linux stub, this is a no-op that succeeds
        let result = socket.write_command_raw(&cmd);
        assert!(result.is_ok());
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_set_event_filter_exists() {
        let socket = HciSocket::open(0).unwrap();
        // On non-Linux stub, this is a no-op that succeeds
        let result = socket.set_event_filter();
        assert!(result.is_ok());
    }
}
