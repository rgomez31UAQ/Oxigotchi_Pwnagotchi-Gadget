//! L2CAP SOCK_SEQPACKET socket wrapper for BT offensive attacks.
//!
//! On Linux, opens a real `AF_BLUETOOTH` / `BTPROTO_L2CAP` socket.
//! On non-Linux platforms, provides stubs that return mock data so the
//! codebase compiles and tests pass everywhere.

/// Convert a BD_ADDR string "AA:BB:CC:DD:EE:FF" into 6 bytes in reversed
/// (little-endian) order, delegating to the HCI parser.
pub fn bdaddr_to_bytes(addr: &str) -> [u8; 6] {
    super::hci::parse_bdaddr(addr)
}

// ===========================================================================
// Linux: real L2CAP socket via AF_BLUETOOTH / BTPROTO_L2CAP
// ===========================================================================

#[cfg(target_os = "linux")]
mod platform {
    use std::os::unix::io::RawFd;

    /// AF_BLUETOOTH address family.
    const AF_BLUETOOTH: libc::c_int = 31;
    /// BTPROTO_L2CAP protocol.
    const BTPROTO_L2CAP: libc::c_int = 0;

    /// `sockaddr_l2` layout matching the kernel struct.
    #[repr(C)]
    struct SockaddrL2 {
        l2_family: libc::sa_family_t,
        l2_psm: u16,
        l2_bdaddr: [u8; 6],
        l2_cid: u16,
        l2_bdaddr_type: u8,
    }

    pub struct L2capSocket {
        fd: RawFd,
    }

    impl L2capSocket {
        /// Connect to a remote BD_ADDR with the given PSM and CID.
        ///
        /// `addr_type`: 0 = BDADDR_BREDR, 1 = BDADDR_LE_PUBLIC, 2 = BDADDR_LE_RANDOM.
        pub fn connect(addr: &str, addr_type: u8, psm: u16, cid: u16) -> Result<Self, String> {
            let bdaddr = super::super::hci::parse_bdaddr(addr);

            unsafe {
                let fd = libc::socket(AF_BLUETOOTH, libc::SOCK_SEQPACKET, BTPROTO_L2CAP);
                if fd < 0 {
                    return Err(format!(
                        "socket(AF_BLUETOOTH, SOCK_SEQPACKET, BTPROTO_L2CAP) failed: {}",
                        std::io::Error::last_os_error()
                    ));
                }

                // Set 5-second send timeout via SO_SNDTIMEO.
                let tv = libc::timeval {
                    tv_sec: 5,
                    tv_usec: 0,
                };
                let ret = libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_SNDTIMEO,
                    &tv as *const libc::timeval as *const libc::c_void,
                    std::mem::size_of::<libc::timeval>() as libc::socklen_t,
                );
                if ret < 0 {
                    libc::close(fd);
                    return Err(format!(
                        "setsockopt(SO_SNDTIMEO) failed: {}",
                        std::io::Error::last_os_error()
                    ));
                }

                let peer = SockaddrL2 {
                    l2_family: AF_BLUETOOTH as libc::sa_family_t,
                    l2_psm: psm.to_le(),
                    l2_bdaddr: bdaddr,
                    l2_cid: cid.to_le(),
                    l2_bdaddr_type: addr_type,
                };

                let ret = libc::connect(
                    fd,
                    &peer as *const SockaddrL2 as *const libc::sockaddr,
                    std::mem::size_of::<SockaddrL2>() as libc::socklen_t,
                );
                if ret < 0 {
                    libc::close(fd);
                    return Err(format!(
                        "connect({}) failed: {}",
                        addr,
                        std::io::Error::last_os_error()
                    ));
                }

                Ok(Self { fd })
            }
        }

        /// Send raw bytes over the L2CAP socket.
        pub fn send(&self, data: &[u8]) -> Result<usize, String> {
            let n = unsafe {
                libc::write(self.fd, data.as_ptr() as *const libc::c_void, data.len())
            };
            if n < 0 {
                return Err(format!(
                    "L2CAP write failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(n as usize)
        }

        /// Receive bytes from the L2CAP socket with a millisecond timeout.
        ///
        /// Returns `Err` on timeout (`timeout_ms > 0` and poll returns 0).
        pub fn recv(&self, buf: &mut [u8], timeout_ms: i32) -> Result<usize, String> {
            let mut pfd = libc::pollfd {
                fd: self.fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let poll_ret = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
            if poll_ret < 0 {
                return Err(format!(
                    "L2CAP poll failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
            if poll_ret == 0 {
                return Err(format!("L2CAP recv timeout ({}ms)", timeout_ms));
            }

            let n = unsafe {
                libc::read(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len())
            };
            if n < 0 {
                return Err(format!(
                    "L2CAP read failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(n as usize)
        }
    }

    impl Drop for L2capSocket {
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
    pub struct L2capSocket {
        _addr: String,
    }

    impl L2capSocket {
        pub fn connect(addr: &str, _addr_type: u8, _psm: u16, _cid: u16) -> Result<Self, String> {
            log::info!("L2capSocket stub: connect({})", addr);
            Ok(Self {
                _addr: addr.to_string(),
            })
        }

        pub fn send(&self, data: &[u8]) -> Result<usize, String> {
            log::info!("L2capSocket stub: send {} bytes", data.len());
            Ok(data.len())
        }

        pub fn recv(&self, buf: &mut [u8], _timeout_ms: i32) -> Result<usize, String> {
            // Return 8 mock bytes: an SMP Pairing Response-like packet.
            let mock: [u8; 8] = [0x02, 0x03, 0x00, 0x01, 0x10, 0x01, 0x01, 0x00];
            let n = mock.len().min(buf.len());
            buf[..n].copy_from_slice(&mock[..n]);
            Ok(n)
        }
    }
}

// Re-export the platform-specific type at module level.
pub use platform::L2capSocket;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bdaddr_to_bytes() {
        let bytes = bdaddr_to_bytes("AA:BB:CC:DD:EE:FF");
        // parse_bdaddr reverses: index 0 = FF, 1 = EE, ..., 5 = AA
        assert_eq!(bytes, [0xFF, 0xEE, 0xDD, 0xCC, 0xBB, 0xAA]);
    }

    #[test]
    fn test_bdaddr_to_bytes_invalid() {
        let bytes = bdaddr_to_bytes("not-valid");
        assert_eq!(bytes, [0u8; 6]);
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn test_connect_stub() {
        let sock = L2capSocket::connect("AA:BB:CC:DD:EE:FF", 0, 0x001F, 0).unwrap();
        // SMP data: Pairing Request (opcode 0x01)
        let data = [0x01u8, 0x03, 0x00, 0x01, 0x10, 0x07, 0x07];
        let n = sock.send(&data).unwrap();
        assert_eq!(n, data.len());
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn test_recv_stub() {
        let sock = L2capSocket::connect("AA:BB:CC:DD:EE:FF", 0, 0x001F, 0).unwrap();
        let mut buf = [0u8; 64];
        let n = sock.recv(&mut buf, 1000).unwrap();
        assert!(n > 0);
        // First mock byte is the SMP opcode field
        assert_eq!(buf[0], 0x02);
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn test_connect_signaling() {
        // L2CAP signaling channel: PSM=1, CID=0x0001
        let sock = L2capSocket::connect("11:22:33:44:55:66", 0, 1, 0x0001).unwrap();
        let echo = [0xFFu8; 4];
        let n = sock.send(&echo).unwrap();
        assert_eq!(n, 4);
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn test_connect_att() {
        // ATT channel: PSM=0, CID=0x0004
        let sock = L2capSocket::connect("AA:BB:CC:DD:EE:FF", 1, 0, 0x0004).unwrap();
        let att_mtu_req = [0x02u8, 0x17, 0x00]; // ATT_MTU_REQ with MTU=23
        let n = sock.send(&att_mtu_req).unwrap();
        assert_eq!(n, att_mtu_req.len());
    }
}
