//! HCI-level BLE scanning and Classic Inquiry for device discovery.
//!
//! Two public functions:
//! - `hci_le_scan()` — passive BLE scan collecting advertising reports
//! - `hci_inquiry()` — Classic BT inquiry collecting device responses

use crate::bluetooth::attacks::hci::{HciCommand, HciSocket};
use crate::bluetooth::model::observation::{
    BtCategory, BtDeviceAttackState, BtDeviceObservation, BtTransport,
};

// HCI opcodes — LE Controller (OGF 0x08)
const OGF_LE: u8 = 0x08;
const LE_SET_SCAN_PARAMS: u16 = 0x000B;
const LE_SET_SCAN_ENABLE: u16 = 0x000C;

// HCI opcodes — Link Control (OGF 0x01)
const OGF_LINK_CTL: u8 = 0x01;
const INQUIRY: u16 = 0x0001;

// HCI event codes
const EVT_INQUIRY_COMPLETE: u8 = 0x01;
const EVT_INQUIRY_RESULT_RSSI: u8 = 0x22;
const EVT_EXTENDED_INQUIRY_RESULT: u8 = 0x2F;
const EVT_LE_META: u8 = 0x3E;
const LE_SUBEVENT_ADV_REPORT: u8 = 0x02;

// GIAC LAP for inquiry (General Inquiry Access Code)
const GIAC_LAP: [u8; 3] = [0x33, 0x8B, 0x9E];

/// Map BLE Appearance value to BtCategory.
fn appearance_to_category(appearance: u16) -> BtCategory {
    match appearance {
        0x00C1..=0x00C4 => BtCategory::Wearable,  // Watch
        0x0440..=0x0448 => BtCategory::Wearable,  // Fitness/Sports
        0x0040..=0x0043 => BtCategory::Phone,
        0x0080..=0x0083 => BtCategory::Computer,
        0x0200          => BtCategory::IoT,         // Generic Tag (AirTag, Tile)
        0x0240..=0x024F => BtCategory::Audio,       // Media Player/Speaker
        0x0941..=0x0944 => BtCategory::Audio,       // Headphone/Earbuds
        0x03C0..=0x03C8 => BtCategory::Peripheral,  // HID
        _ => BtCategory::Unknown,
    }
}

/// Map Classic BT Class of Device major class to BtCategory.
fn cod_major_to_category(major: u8) -> BtCategory {
    match major {
        0x01 => BtCategory::Computer,
        0x02 => BtCategory::Phone,
        0x04 => BtCategory::Audio,
        0x05 => BtCategory::Peripheral,
        0x06 => BtCategory::Peripheral,  // Imaging
        0x07 => BtCategory::Wearable,
        0x08 => BtCategory::IoT,         // Toy
        _ => BtCategory::Unknown,
    }
}

/// Format 6 raw BD_ADDR bytes (little-endian HCI order) as "AA:BB:CC:DD:EE:FF".
fn format_bdaddr(bytes: &[u8]) -> String {
    if bytes.len() < 6 {
        return "??:??:??:??:??:??".into();
    }
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        bytes[5], bytes[4], bytes[3], bytes[2], bytes[1], bytes[0]
    )
}

/// Parse a Complete/Shortened Local Name from AD structures.
/// Returns the first name found, or None.
fn parse_ad_name(ad_data: &[u8]) -> Option<String> {
    let mut offset = 0;
    while offset < ad_data.len() {
        let len = ad_data[offset] as usize;
        if len == 0 || offset + 1 + len > ad_data.len() {
            break;
        }
        let ad_type = ad_data[offset + 1];
        // 0x09 = Complete Local Name, 0x08 = Shortened Local Name
        if (ad_type == 0x09 || ad_type == 0x08) && len > 1 {
            let name_bytes = &ad_data[offset + 2..offset + 1 + len];
            return Some(String::from_utf8_lossy(name_bytes).into_owned());
        }
        offset += 1 + len;
    }
    None
}

/// Parse Appearance from AD structures. Returns 0 if not found.
fn parse_ad_appearance(ad_data: &[u8]) -> u16 {
    let mut offset = 0;
    while offset < ad_data.len() {
        let len = ad_data[offset] as usize;
        if len == 0 || offset + 1 + len > ad_data.len() {
            break;
        }
        let ad_type = ad_data[offset + 1];
        // 0x19 = Appearance
        if ad_type == 0x19 && len >= 3 {
            return u16::from_le_bytes([ad_data[offset + 2], ad_data[offset + 3]]);
        }
        offset += 1 + len;
    }
    0
}

/// Parse one LE Advertising Report from raw event params.
/// `data` starts after the subevent byte (i.e., at Num_Reports).
fn parse_le_adv_reports(data: &[u8]) -> Vec<BtDeviceObservation> {
    if data.is_empty() {
        return vec![];
    }
    let num_reports = data[0] as usize;
    let mut results = Vec::with_capacity(num_reports);
    let mut offset = 1;

    for _ in 0..num_reports {
        // Each report: event_type(1) + addr_type(1) + addr(6) + data_len(1) + data(N) + rssi(1)
        if offset + 9 > data.len() {
            break;
        }
        let _event_type = data[offset];
        let addr_type_byte = data[offset + 1];
        let addr_bytes = &data[offset + 2..offset + 8];
        let data_len = data[offset + 8] as usize;
        offset += 9;

        if offset + data_len + 1 > data.len() {
            break;
        }
        let ad_data = &data[offset..offset + data_len];
        let rssi = data[offset + data_len] as i8;
        offset += data_len + 1;

        let address = format_bdaddr(addr_bytes);
        let addr_type_str = if addr_type_byte == 0 { "public" } else { "random" };
        let name = parse_ad_name(ad_data);
        let appearance = parse_ad_appearance(ad_data);
        let category = if appearance != 0 {
            appearance_to_category(appearance)
        } else {
            BtCategory::Unknown
        };

        results.push(BtDeviceObservation {
            id: format!("ble:{address}"),
            address,
            address_type: Some(addr_type_str.to_string()),
            transport: BtTransport::Ble,
            name,
            rssi: Some(rssi as i16),
            rssi_best: Some(rssi as i16),
            category,
            services: Vec::new(),
            manufacturer: None,
            first_seen: chrono::Utc::now(),
            ts: chrono::Utc::now(),
            seen_count: 1,
            attack_state: BtDeviceAttackState::Untouched,
        });
    }

    results
}

/// Parse an Inquiry Result with RSSI (event 0x22) or Extended Inquiry Result (event 0x2F).
fn parse_inquiry_result(event_code: u8, data: &[u8]) -> Vec<BtDeviceObservation> {
    let mut results = Vec::new();

    if event_code == EVT_INQUIRY_RESULT_RSSI {
        // Inquiry Result with RSSI: num_responses(1) + per-response data
        if data.is_empty() {
            return results;
        }
        let num = data[0] as usize;
        let mut offset = 1;
        // Each: BD_ADDR(6) + page_scan_rep(1) + reserved(1) + CoD(3) + clock_offset(2) + RSSI(1) = 14
        for _ in 0..num {
            if offset + 14 > data.len() {
                break;
            }
            let addr_bytes = &data[offset..offset + 6];
            // Skip page_scan_rep(1) + reserved(1) = offset+6..offset+8
            let cod = &data[offset + 8..offset + 11];
            // Skip clock_offset(2) = offset+11..offset+13
            let rssi = data[offset + 13] as i8;
            offset += 14;

            let address = format_bdaddr(addr_bytes);
            let major_class = (cod[1] >> 2) & 0x1F;
            let category = cod_major_to_category(major_class);

            results.push(BtDeviceObservation {
                id: format!("classic:{address}"),
                address,
                address_type: None,
                transport: BtTransport::Classic,
                name: None,
                rssi: Some(rssi as i16),
                rssi_best: Some(rssi as i16),
                category,
                services: Vec::new(),
                manufacturer: None,
                first_seen: chrono::Utc::now(),
                ts: chrono::Utc::now(),
                seen_count: 1,
                attack_state: BtDeviceAttackState::Untouched,
            });
        }
    } else if event_code == EVT_EXTENDED_INQUIRY_RESULT {
        // Extended Inquiry Result: always 1 response
        // num_responses(1) + BD_ADDR(6) + page_scan_rep(1) + reserved(1) + CoD(3) + clock_offset(2) + RSSI(1) + EIR(240)
        if data.len() < 15 {
            return results;
        }
        let addr_bytes = &data[1..7];
        let cod = &data[9..12];
        let rssi = data[14] as i8;
        let eir_data = if data.len() > 15 { &data[15..] } else { &[] };

        let address = format_bdaddr(addr_bytes);
        let major_class = (cod[1] >> 2) & 0x1F;
        let category = cod_major_to_category(major_class);
        let name = parse_ad_name(eir_data);

        results.push(BtDeviceObservation {
            id: format!("classic:{address}"),
            address,
            address_type: None,
            transport: BtTransport::Classic,
            name,
            rssi: Some(rssi as i16),
            rssi_best: Some(rssi as i16),
            category,
            services: Vec::new(),
            manufacturer: None,
            first_seen: chrono::Utc::now(),
            ts: chrono::Utc::now(),
            seen_count: 1,
            attack_state: BtDeviceAttackState::Untouched,
        });
    }

    results
}

// ---------------------------------------------------------------------------
// Public scan functions
// ---------------------------------------------------------------------------

/// Run a passive BLE scan for `duration_ms` milliseconds.
/// Returns all unique devices seen via LE Advertising Reports.
pub fn hci_le_scan(hci: &HciSocket, duration_ms: u32) -> Vec<BtDeviceObservation> {
    // Set event filter to reject ACL/SCO data
    if let Err(e) = hci.set_event_filter() {
        log::warn!("bt_scan: set_event_filter failed: {e}");
    }

    // Drain any stale events from the socket before sending commands.
    // Without this, send_command() may read a queued event and misparse it.
    hci.drain_events();

    // Defensive: disable any stale scan (ignore errors).
    let disable_cmd = HciCommand::new(OGF_LE, LE_SET_SCAN_ENABLE, vec![0x00, 0x00]);
    let _ = hci.send_command(&disable_cmd);

    // Set scan parameters: passive(0), interval=100ms(0x00A0), window=100ms(0x00A0),
    // own_addr=public(0), filter=accept_all(0)
    let scan_params = HciCommand::new(
        OGF_LE,
        LE_SET_SCAN_PARAMS,
        vec![0x00, 0xA0, 0x00, 0xA0, 0x00, 0x00, 0x00],
    );
    match hci.send_command(&scan_params) {
        Ok(resp) if resp.status != 0 => {
            log::warn!("bt_scan: LE_Set_Scan_Parameters failed (status=0x{:02X})", resp.status);
            return vec![];
        }
        Err(e) => {
            log::warn!("bt_scan: LE_Set_Scan_Parameters error: {e}");
            return vec![];
        }
        _ => {}
    }

    // Enable scan — use write_command_raw to avoid swallowing first ad report
    let enable_cmd = HciCommand::new(OGF_LE, LE_SET_SCAN_ENABLE, vec![0x01, 0x01]); // enable + filter dups
    if let Err(e) = hci.write_command_raw(&enable_cmd) {
        log::warn!("bt_scan: LE_Set_Scan_Enable write failed: {e}");
        return vec![];
    }

    // Brief pause for Command Complete to arrive before we start reading events
    std::thread::sleep(std::time::Duration::from_millis(20));

    // Collect advertising reports for the scan duration
    let mut all_devices = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(duration_ms as u64);

    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        let remaining_ms = remaining.as_millis() as i32;
        if remaining_ms <= 0 {
            break;
        }

        match hci.wait_event(EVT_LE_META, remaining_ms) {
            Ok(params) => {
                // params[0] = subevent code
                if params.len() > 1 && params[0] == LE_SUBEVENT_ADV_REPORT {
                    let devices = parse_le_adv_reports(&params[1..]);
                    all_devices.extend(devices);
                }
                // else: different LE subevent, skip
            }
            Err(_) => {
                // Timeout or read error — scan period done
                break;
            }
        }
    }

    // Disable scan (cleanup — always run)
    let disable_cmd = HciCommand::new(OGF_LE, LE_SET_SCAN_ENABLE, vec![0x00, 0x00]);
    let _ = hci.send_command(&disable_cmd);

    log::info!("bt_scan: LE scan found {} devices", all_devices.len());
    all_devices
}

/// Run a Classic BT Inquiry for `inquiry_length` * 1.28 seconds.
/// Typical value: 3 (= ~3.84 seconds). Returns discovered devices.
pub fn hci_inquiry(hci: &HciSocket, inquiry_length: u8) -> Vec<BtDeviceObservation> {
    // Set event filter to reject ACL/SCO data
    if let Err(e) = hci.set_event_filter() {
        log::warn!("bt_scan: set_event_filter failed: {e}");
    }
    hci.drain_events();

    // HCI_Inquiry: LAP(3) + inquiry_length(1) + num_responses(1)
    let mut params = Vec::with_capacity(5);
    params.extend_from_slice(&GIAC_LAP);
    params.push(inquiry_length);
    params.push(0); // unlimited responses

    let inquiry_cmd = HciCommand::new(OGF_LINK_CTL, INQUIRY, params);

    // Use write_command_raw — Inquiry returns Command Status (0x0F), not Command Complete
    if let Err(e) = hci.write_command_raw(&inquiry_cmd) {
        log::warn!("bt_scan: HCI_Inquiry write failed: {e}");
        return vec![];
    }

    // Read Command Status event (0x0F) — verify inquiry was accepted
    match hci.wait_event(0x0F, 2000) {
        Ok(params) => {
            // Command Status: status(1) + ncmds(1) + opcode(2)
            if !params.is_empty() && params[0] != 0x00 {
                log::warn!("bt_scan: HCI_Inquiry rejected (status=0x{:02X})", params[0]);
                return vec![];
            }
        }
        Err(e) => {
            log::warn!("bt_scan: HCI_Inquiry command status timeout: {e}");
            return vec![];
        }
    }

    // Collect inquiry results until Inquiry Complete or timeout
    let timeout_ms = (inquiry_length as u64 * 1280) + 2000; // inquiry duration + 2s margin
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let mut all_devices = Vec::new();

    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        let remaining_ms = remaining.as_millis() as i32;
        if remaining_ms <= 0 {
            break;
        }

        // Read any HCI event
        // Try Inquiry Result with RSSI (0x22), Extended (0x2F), or Complete (0x01)
        // We use a raw read approach: wait_event with the most common code,
        // but we need to accept multiple event types. Use a short timeout and retry.
        let result = hci.wait_event(EVT_INQUIRY_RESULT_RSSI, remaining_ms.min(500));
        match result {
            Ok(params) => {
                let devices = parse_inquiry_result(EVT_INQUIRY_RESULT_RSSI, &params);
                all_devices.extend(devices);
            }
            Err(_) => {
                // Could be timeout or a different event type arrived.
                // Try Extended Inquiry Result
                if let Ok(params) = hci.wait_event(EVT_EXTENDED_INQUIRY_RESULT, 100) {
                    let devices = parse_inquiry_result(EVT_EXTENDED_INQUIRY_RESULT, &params);
                    all_devices.extend(devices);
                }
                // Check for Inquiry Complete
                if hci.wait_event(EVT_INQUIRY_COMPLETE, 100).is_ok() {
                    break;
                }
            }
        }
    }

    log::info!("bt_scan: Inquiry found {} devices", all_devices.len());
    all_devices
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_appearance_to_category() {
        assert_eq!(appearance_to_category(0x00C1), BtCategory::Wearable); // Watch
        assert_eq!(appearance_to_category(0x0941), BtCategory::Audio);    // Headphone
        assert_eq!(appearance_to_category(0x0240), BtCategory::Audio);    // Media Player
        assert_eq!(appearance_to_category(0x0040), BtCategory::Phone);
        assert_eq!(appearance_to_category(0x0080), BtCategory::Computer);
        assert_eq!(appearance_to_category(0x0200), BtCategory::IoT);      // AirTag
        assert_eq!(appearance_to_category(0x03C0), BtCategory::Peripheral); // HID
        assert_eq!(appearance_to_category(0x0440), BtCategory::Wearable); // Fitness
        assert_eq!(appearance_to_category(0xFFFF), BtCategory::Unknown);
    }

    #[test]
    fn test_cod_major_to_category() {
        assert_eq!(cod_major_to_category(0x01), BtCategory::Computer);
        assert_eq!(cod_major_to_category(0x02), BtCategory::Phone);
        assert_eq!(cod_major_to_category(0x04), BtCategory::Audio);
        assert_eq!(cod_major_to_category(0x05), BtCategory::Peripheral);
        assert_eq!(cod_major_to_category(0x07), BtCategory::Wearable);
        assert_eq!(cod_major_to_category(0x08), BtCategory::IoT);
        assert_eq!(cod_major_to_category(0x00), BtCategory::Unknown);
    }

    #[test]
    fn test_format_bdaddr() {
        assert_eq!(format_bdaddr(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66]), "66:55:44:33:22:11");
        assert_eq!(format_bdaddr(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]), "00:00:00:00:00:00");
        assert_eq!(format_bdaddr(&[0xFF, 0xEE, 0xDD, 0xCC, 0xBB, 0xAA]), "AA:BB:CC:DD:EE:FF");
    }

    #[test]
    fn test_parse_ad_name() {
        // AD: [len=5, type=0x09, 'T','e','s','t']
        let ad = [5, 0x09, b'T', b'e', b's', b't'];
        assert_eq!(parse_ad_name(&ad), Some("Test".into()));

        // Shortened name
        let ad2 = [4, 0x08, b'F', b'o', b'o'];
        assert_eq!(parse_ad_name(&ad2), Some("Foo".into()));

        // No name
        let ad3 = [2, 0x01, 0x06]; // Flags only
        assert_eq!(parse_ad_name(&ad3), None);

        // Empty
        assert_eq!(parse_ad_name(&[]), None);
    }

    #[test]
    fn test_parse_ad_appearance() {
        // AD: [len=3, type=0x19, 0xC1, 0x00] = Watch (0x00C1)
        let ad = [3, 0x19, 0xC1, 0x00];
        assert_eq!(parse_ad_appearance(&ad), 0x00C1);

        // No appearance
        let ad2 = [2, 0x01, 0x06];
        assert_eq!(parse_ad_appearance(&ad2), 0);
    }

    #[test]
    fn test_parse_le_adv_reports_single() {
        // Construct a single LE Advertising Report
        // num_reports(1) + event_type(1) + addr_type(1) + addr(6) + data_len(1) + ad_data(N) + rssi(1)
        let ad_data = [5u8, 0x09, b'T', b'e', b's', b't', 3, 0x19, 0xC1, 0x00]; // name="Test", appearance=Watch
        let mut report = vec![
            1,    // num_reports = 1
            0x00, // event_type = ADV_IND
            0x00, // addr_type = public
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, // addr (LE order)
            ad_data.len() as u8,
        ];
        report.extend_from_slice(&ad_data);
        report.push(0xCE); // RSSI = -50 (signed)

        let devices = parse_le_adv_reports(&report);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].address, "66:55:44:33:22:11");
        assert_eq!(devices[0].id, "ble:66:55:44:33:22:11");
        assert_eq!(devices[0].name, Some("Test".into()));
        assert_eq!(devices[0].rssi, Some(-50));
        assert_eq!(devices[0].category, BtCategory::Wearable);
        assert_eq!(devices[0].transport, BtTransport::Ble);
        assert_eq!(devices[0].address_type, Some("public".into()));
    }

    #[test]
    fn test_parse_le_adv_reports_empty() {
        assert_eq!(parse_le_adv_reports(&[]).len(), 0);
        assert_eq!(parse_le_adv_reports(&[0]).len(), 0); // 0 reports
    }

    #[test]
    fn test_parse_inquiry_result_rssi() {
        // Inquiry Result with RSSI: num(1) + BD_ADDR(6) + page_scan_rep(1) + reserved(1) + CoD(3) + clock_offset(2) + RSSI(1) = 15
        let mut data = vec![1u8]; // num_responses=1
        data.extend_from_slice(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66]); // BD_ADDR
        data.push(0x01); // page_scan_rep
        data.push(0x00); // reserved
        // CoD: major class = Audio (0x04). CoD bytes: [minor|service, major<<2|format, service_hi]
        // major=0x04 → cod[1] = 0x04 << 2 = 0x10
        data.extend_from_slice(&[0x00, 0x10, 0x00]); // CoD with major=Audio
        data.extend_from_slice(&[0x00, 0x00]); // clock_offset
        data.push(0xD0u8); // RSSI = -48

        let devices = parse_inquiry_result(0x22, &data);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].address, "66:55:44:33:22:11");
        assert_eq!(devices[0].id, "classic:66:55:44:33:22:11");
        assert_eq!(devices[0].category, BtCategory::Audio);
        assert_eq!(devices[0].rssi, Some(-48));
        assert_eq!(devices[0].transport, BtTransport::Classic);
        assert!(devices[0].name.is_none());
    }

    #[test]
    fn test_parse_extended_inquiry_result() {
        // Extended: num(1) + BD_ADDR(6) + page_scan_rep(1) + reserved(1) + CoD(3) + clock_offset(2) + RSSI(1) + EIR(240)
        let mut data = vec![1u8]; // num_responses=1
        data.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]); // BD_ADDR
        data.push(0x01); // page_scan_rep
        data.push(0x00); // reserved
        data.extend_from_slice(&[0x00, 0x08, 0x00]); // CoD: major=Phone (0x02 << 2 = 0x08)
        data.extend_from_slice(&[0x00, 0x00]); // clock_offset
        data.push(0xC8u8); // RSSI = -56
        // EIR: name "Buds"
        data.extend_from_slice(&[5, 0x09, b'B', b'u', b'd', b's']);
        // Pad to at least something
        while data.len() < 255 {
            data.push(0);
        }

        let devices = parse_inquiry_result(0x2F, &data);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].address, "FF:EE:DD:CC:BB:AA");
        assert_eq!(devices[0].id, "classic:FF:EE:DD:CC:BB:AA");
        assert_eq!(devices[0].name, Some("Buds".into()));
        assert_eq!(devices[0].category, BtCategory::Phone);
        assert_eq!(devices[0].rssi, Some(-56));
    }

    #[test]
    fn test_hci_le_scan_stub() {
        // On non-Linux, this returns empty (no real HCI)
        #[cfg(not(target_os = "linux"))]
        {
            let hci = HciSocket::open(0).unwrap();
            let devices = hci_le_scan(&hci, 100);
            assert!(devices.is_empty());
        }
    }

    #[test]
    fn test_hci_inquiry_stub() {
        #[cfg(not(target_os = "linux"))]
        {
            let hci = HciSocket::open(0).unwrap();
            let devices = hci_inquiry(&hci, 1);
            assert!(devices.is_empty());
        }
    }
}
