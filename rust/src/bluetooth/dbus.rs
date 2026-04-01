//! BlueZ D-Bus wrapper module.
//!
//! Provides a clean API over BlueZ D-Bus interactions for PAN tethering,
//! device pairing/trust/removal, adapter scanning, and agent registration.
//!
//! On non-Linux platforms, stub implementations allow compilation and testing.

// ─── Shared types (not platform-gated) ───────────────────────────────────────

/// Represents an active PAN (Personal Area Network) connection.
#[derive(Debug, Clone)]
pub struct PanConnection {
    /// The network interface name (e.g. "bnep0").
    pub interface: String,
}

/// A BlueZ device discovered or paired via D-Bus.
#[derive(Debug, Clone)]
pub struct BlueZDevice {
    /// D-Bus object path, e.g. "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF".
    pub path: String,
    /// MAC address, e.g. "AA:BB:CC:DD:EE:FF".
    pub mac: String,
    /// Human-readable name / alias.
    pub name: String,
    /// Whether the device is paired.
    pub paired: bool,
    /// Whether the device is trusted.
    pub trusted: bool,
    /// Whether the device is currently connected.
    pub connected: bool,
}

/// Events emitted during the pairing / agent flow.
#[derive(Debug, Clone)]
pub enum PairingEvent {
    /// BlueZ asks us to confirm a passkey displayed on both devices.
    ConfirmPasskey { device: String, passkey: u32 },
    /// BlueZ asks us to display a passkey the remote device should enter.
    DisplayPasskey { device: String, passkey: u32 },
    /// Pairing request from a remote device (no passkey).
    RequestConfirmation { device: String },
    /// Pairing completed (success or failure).
    PairingComplete { device: String, success: bool },
}

// ─── Linux implementation ────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod inner {
    use super::*;
    use dbus::arg::{RefArg, Variant};
    use dbus::blocking::Connection;
    use log::{info, warn};
    use std::collections::HashMap;
    use std::sync::mpsc::Sender;
    use std::time::Duration;

    /// The D-Bus object path we register our agent at.
    const AGENT_PATH: &str = "/org/bluez/agent/oxigotchi";
    /// Default adapter path.
    const ADAPTER_PATH: &str = "/org/bluez/hci0";

    /// Extract a boolean from a D-Bus property map.
    ///
    /// The `dbus` crate maps D-Bus booleans to `i64` (1 = true, 0 = false).
    /// We also try a string fallback for edge cases.
    fn prop_bool(
        props: &HashMap<String, Variant<Box<dyn RefArg>>>,
        key: &str,
    ) -> Option<bool> {
        props.get(key).and_then(|v| {
            if let Some(n) = v.0.as_i64() {
                return Some(n != 0);
            }
            let s = format!("{:?}", v.0);
            match s.as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            }
        })
    }

    /// Extract a string from a D-Bus property map.
    fn prop_str(
        props: &HashMap<String, Variant<Box<dyn RefArg>>>,
        key: &str,
    ) -> Option<String> {
        props.get(key).and_then(|v| v.0.as_str().map(|s| s.to_string()))
    }

    /// Wraps all BlueZ D-Bus interactions.
    pub struct DbusBluez {
        conn: Connection,
        /// Active PAN connection, if any.
        pan: Option<PanConnection>,
        /// Device path of the PAN-connected device.
        pan_device: Option<String>,
        /// Channel for pairing events.
        pairing_tx: Option<Sender<PairingEvent>>,
    }

    impl DbusBluez {
        /// Connect to the system D-Bus.
        pub fn new() -> Result<Self, String> {
            let conn = Connection::new_system().map_err(|e| format!("D-Bus connect: {e}"))?;
            info!("[dbus] Connected to system bus");
            Ok(Self {
                conn,
                pan: None,
                pan_device: None,
                pairing_tx: None,
            })
        }

        /// Set a channel for receiving pairing events.
        pub fn set_pairing_channel(&mut self, tx: Sender<PairingEvent>) {
            self.pairing_tx = Some(tx);
        }

        /// List paired AND trusted devices via ObjectManager.
        pub fn list_paired_devices(&self) -> Result<Vec<BlueZDevice>, String> {
            let proxy = self.conn.with_proxy("org.bluez", "/", Duration::from_secs(5));
            use dbus::blocking::stdintf::org_freedesktop_dbus::ObjectManager;
            let objects = proxy
                .get_managed_objects()
                .map_err(|e| format!("GetManagedObjects: {e}"))?;

            let mut devices = Vec::new();
            for (path, ifaces) in &objects {
                if let Some(props) = ifaces.get("org.bluez.Device1") {
                    let paired = prop_bool(props, "Paired").unwrap_or(false);
                    let trusted = prop_bool(props, "Trusted").unwrap_or(false);
                    if !paired || !trusted {
                        continue;
                    }
                    let mac = prop_str(props, "Address").unwrap_or_default();
                    let name = prop_str(props, "Alias").unwrap_or_else(|| mac.clone());
                    let connected = prop_bool(props, "Connected").unwrap_or(false);
                    devices.push(BlueZDevice {
                        path: path.to_string(),
                        mac,
                        name,
                        paired,
                        trusted,
                        connected,
                    });
                }
            }
            info!("[dbus] Found {} paired+trusted devices", devices.len());
            Ok(devices)
        }

        /// Connect to a device's PAN Network (NAP profile) with 30s timeout.
        pub fn connect_pan(&mut self, device_path: &str) -> Result<PanConnection, String> {
            let proxy = self.conn.with_proxy(
                "org.bluez",
                device_path,
                Duration::from_secs(30),
            );
            let (iface_name,): (String,) = proxy
                .method_call("org.bluez.Network1", "Connect", ("nap",))
                .map_err(|e| format!("PAN Connect: {e}"))?;

            info!("[dbus] PAN connected on {iface_name} via {device_path}");
            let pc = PanConnection {
                interface: iface_name,
            };
            self.pan = Some(pc.clone());
            self.pan_device = Some(device_path.to_string());
            Ok(pc)
        }

        /// Disconnect the active PAN connection via Network1.Disconnect.
        pub fn disconnect_pan(&mut self) -> Result<(), String> {
            if let Some(dev) = self.pan_device.take() {
                let proxy =
                    self.conn
                        .with_proxy("org.bluez", &dev, Duration::from_secs(10));
                let _: () = proxy
                    .method_call("org.bluez.Network1", "Disconnect", ())
                    .map_err(|e| format!("PAN Disconnect: {e}"))?;
                info!("[dbus] PAN disconnected from {dev}");
            }
            self.pan = None;
            Ok(())
        }

        /// Return the PAN interface name if connected (e.g. "bnep0").
        pub fn pan_interface(&self) -> Option<&str> {
            self.pan.as_ref().map(|p| p.interface.as_str())
        }

        /// Pair with a device.
        pub fn pair_device(&self, device_path: &str) -> Result<(), String> {
            let proxy = self.conn.with_proxy(
                "org.bluez",
                device_path,
                Duration::from_secs(30),
            );
            let _: () = proxy
                .method_call("org.bluez.Device1", "Pair", ())
                .map_err(|e| format!("Pair: {e}"))?;
            info!("[dbus] Paired with {device_path}");
            Ok(())
        }

        /// Set Trusted=true on a device.
        pub fn trust_device(&self, device_path: &str) -> Result<(), String> {
            let proxy = self.conn.with_proxy(
                "org.bluez",
                device_path,
                Duration::from_secs(5),
            );
            use dbus::blocking::stdintf::org_freedesktop_dbus::Properties;
            proxy
                .set("org.bluez.Device1", "Trusted", Variant(true))
                .map_err(|e| format!("Trust: {e}"))?;
            info!("[dbus] Trusted {device_path}");
            Ok(())
        }

        /// Remove a device from BlueZ via Adapter1.RemoveDevice.
        pub fn remove_device(&self, device_path: &str) -> Result<(), String> {
            // Derive adapter path: /org/bluez/hci0/dev_XX -> /org/bluez/hci0
            let adapter = device_path
                .rfind('/')
                .map(|i| &device_path[..i])
                .unwrap_or(ADAPTER_PATH);
            let proxy =
                self.conn
                    .with_proxy("org.bluez", adapter, Duration::from_secs(10));
            let dev_path = dbus::Path::from(device_path);
            let _: () = proxy
                .method_call("org.bluez.Adapter1", "RemoveDevice", (dev_path,))
                .map_err(|e| format!("RemoveDevice: {e}"))?;
            info!("[dbus] Removed {device_path}");
            Ok(())
        }

        /// Whether a PAN connection is active.
        pub fn is_connected(&self) -> bool {
            self.pan.is_some()
        }

        /// Ping the bus to verify the D-Bus connection and bluez are alive.
        pub fn is_bus_alive(&self) -> bool {
            let proxy = self.conn.with_proxy(
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                Duration::from_secs(2),
            );
            let result: Result<(String,), _> =
                proxy.method_call("org.freedesktop.DBus", "GetNameOwner", ("org.bluez",));
            result.is_ok()
        }

        /// Register our agent with BlueZ's AgentManager1.
        pub fn register_agent(&self) -> Result<(), String> {
            let proxy = self.conn.with_proxy(
                "org.bluez",
                "/org/bluez",
                Duration::from_secs(5),
            );
            let agent = dbus::Path::from(AGENT_PATH);
            let _: () = proxy
                .method_call(
                    "org.bluez.AgentManager1",
                    "RegisterAgent",
                    (agent.clone(), "DisplayYesNo"),
                )
                .map_err(|e| format!("RegisterAgent: {e}"))?;
            let _: () = proxy
                .method_call(
                    "org.bluez.AgentManager1",
                    "RequestDefaultAgent",
                    (agent,),
                )
                .map_err(|e| format!("RequestDefaultAgent: {e}"))?;
            info!("[dbus] Agent registered at {AGENT_PATH}");
            Ok(())
        }

        /// Start BT scanning: set discoverable + pairable, then StartDiscovery.
        pub fn start_scan(&self) -> Result<(), String> {
            let proxy = self.conn.with_proxy(
                "org.bluez",
                ADAPTER_PATH,
                Duration::from_secs(5),
            );
            use dbus::blocking::stdintf::org_freedesktop_dbus::Properties;
            let _ = proxy.set("org.bluez.Adapter1", "Discoverable", Variant(true));
            let _ = proxy.set("org.bluez.Adapter1", "Pairable", Variant(true));
            let _: () = proxy
                .method_call("org.bluez.Adapter1", "StartDiscovery", ())
                .map_err(|e| format!("StartDiscovery: {e}"))?;
            info!("[dbus] Scan started on {ADAPTER_PATH}");
            Ok(())
        }

        /// Stop BT scanning: StopDiscovery, set discoverable + pairable = false.
        pub fn stop_scan(&self) -> Result<(), String> {
            let proxy = self.conn.with_proxy(
                "org.bluez",
                ADAPTER_PATH,
                Duration::from_secs(5),
            );
            let _: () = proxy
                .method_call("org.bluez.Adapter1", "StopDiscovery", ())
                .map_err(|e| format!("StopDiscovery: {e}"))?;
            use dbus::blocking::stdintf::org_freedesktop_dbus::Properties;
            let _ = proxy.set("org.bluez.Adapter1", "Discoverable", Variant(false));
            let _ = proxy.set("org.bluez.Adapter1", "Pairable", Variant(false));
            info!("[dbus] Scan stopped on {ADAPTER_PATH}");
            Ok(())
        }
    }
}

// ─── Non-Linux stubs ─────────────────────────────────────────────────────────

#[cfg(not(target_os = "linux"))]
mod inner {
    use super::*;
    use std::sync::mpsc::Sender;

    /// Stub BlueZ D-Bus wrapper for non-Linux platforms.
    pub struct DbusBluez {
        pairing_tx: Option<Sender<PairingEvent>>,
    }

    impl DbusBluez {
        pub fn new() -> Result<Self, String> {
            Ok(Self { pairing_tx: None })
        }

        pub fn set_pairing_channel(&mut self, tx: Sender<PairingEvent>) {
            self.pairing_tx = Some(tx);
        }

        pub fn list_paired_devices(&self) -> Result<Vec<BlueZDevice>, String> {
            Ok(vec![])
        }

        pub fn connect_pan(&mut self, _device_path: &str) -> Result<PanConnection, String> {
            Err("not supported on this platform".to_string())
        }

        pub fn disconnect_pan(&mut self) -> Result<(), String> {
            Ok(())
        }

        pub fn pan_interface(&self) -> Option<&str> {
            None
        }

        pub fn pair_device(&self, _device_path: &str) -> Result<(), String> {
            Err("not supported on this platform".to_string())
        }

        pub fn trust_device(&self, _device_path: &str) -> Result<(), String> {
            Err("not supported on this platform".to_string())
        }

        pub fn remove_device(&self, _device_path: &str) -> Result<(), String> {
            Err("not supported on this platform".to_string())
        }

        pub fn is_connected(&self) -> bool {
            false
        }

        pub fn is_bus_alive(&self) -> bool {
            true
        }

        pub fn register_agent(&self) -> Result<(), String> {
            Ok(())
        }

        pub fn start_scan(&self) -> Result<(), String> {
            Ok(())
        }

        pub fn stop_scan(&self) -> Result<(), String> {
            Ok(())
        }
    }
}

pub use inner::DbusBluez;

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pan_connection_struct() {
        let pc = PanConnection {
            interface: "bnep0".to_string(),
        };
        assert_eq!(pc.interface, "bnep0");
    }

    #[test]
    fn test_bluez_device_struct() {
        let dev = BlueZDevice {
            path: "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF".to_string(),
            mac: "AA:BB:CC:DD:EE:FF".to_string(),
            name: "iPhone".to_string(),
            paired: true,
            trusted: true,
            connected: false,
        };
        assert!(dev.paired);
        assert!(dev.trusted);
        assert!(!dev.connected);
    }

    #[test]
    fn test_pairing_event_variants() {
        let ev = PairingEvent::ConfirmPasskey {
            device: "iPhone".into(),
            passkey: 123456,
        };
        match ev {
            PairingEvent::ConfirmPasskey { passkey, .. } => assert_eq!(passkey, 123456),
            _ => panic!("wrong variant"),
        }
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn test_stub_creates_ok() {
        let mut dbus = DbusBluez::new().unwrap();
        assert!(dbus.list_paired_devices().unwrap().is_empty());
        assert!(dbus.pan_interface().is_none());
        assert!(!dbus.is_connected());
        assert!(dbus.connect_pan("/org/bluez/hci0/dev_AA").is_err());
    }
}
