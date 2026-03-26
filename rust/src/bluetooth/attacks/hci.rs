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
    }
}

// Re-export the platform-specific type at module level.
pub use platform::HciSocket;

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
}
