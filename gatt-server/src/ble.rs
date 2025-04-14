use crate::ble_gatt::init_gatt_services;
use core::assert_eq;
use core::time::Duration;
use defmt::*;

use embassy_stm32_wpan::hci::BdAddr;
use embassy_stm32_wpan::hci::{
    host::{AdvertisingType, EncryptionKey, HostHci, OwnAddressType},
    vendor::command::{
        gap::{
            AddressType, AdvertisingFilterPolicy, AuthenticationRequirements,
            DiscoverableParameters, GapCommands, IoCapability, LocalName, Pin, Role,
            SecureConnectionSupport,
        },
        gatt::GattCommands,
        hal::{ConfigData, HalCommands, PowerLevel},
    },
    Event,
};

use embassy_stm32_wpan::{lhci::LhciC1DeviceInformationCcrp, TlMbox};
use embassy_stm32_wpan::sub::ble::Ble;

const BLE_GAP_DEVICE_NAME_LENGTH: u8 = 7;

fn is_command_complete(event: Event) -> Result<(), ()> {
    match event {
        Event::CommandComplete(_) => Ok(()),
        _ => {
            error!("Command failed to complete: {}", event);
            Err(())
        }
    }
}

pub async fn init_ble(mbox: &mut TlMbox<'_>) -> Result<(), ()> {
    // Init BLE stack
    let res = mbox
        .sys_subsystem
        .shci_c2_ble_init(Default::default())
        .await;
    info!("BLE init: {}", res);

    mbox.ble_subsystem.reset().await;
    let response = mbox.ble_subsystem.tl_read().await;
    info!("Reset BLE: {}", response.as_hci_event());
    is_command_complete(response.as_hci_event()).unwrap();

    let bd_addr = get_bd_addr();
    mbox.ble_subsystem
        .write_config_data(&ConfigData::public_address(bd_addr).build())
        .await;
    let response = mbox.ble_subsystem.tl_read().await;
    info!("Config public address {:x}: {}", bd_addr, response.as_hci_event());
    is_command_complete(response.as_hci_event()).unwrap();

    let rand_addr = get_random_addr();
    mbox.ble_subsystem
        .write_config_data(&ConfigData::random_address(rand_addr).build())
        .await;
    let response = mbox.ble_subsystem.tl_read().await;
    info!("Config random address {:x}: {}", bd_addr, response.as_hci_event());
    is_command_complete(response.as_hci_event()).unwrap();

    let irk = get_irk();
    mbox.ble_subsystem
        .write_config_data(&ConfigData::identity_root(&irk).build())
        .await;
    let response = mbox.ble_subsystem.tl_read().await;
    info!("Config identity root {}: {}", irk, response.as_hci_event());
    assert_eq!(*response.payload(), [0x01, 0x0C, 0xFC, 0x00]);
    is_command_complete(response.as_hci_event()).unwrap();

    let erk = get_erk();
    mbox.ble_subsystem
        .write_config_data(&ConfigData::identity_root(&erk).build())
        .await;
    let response = mbox.ble_subsystem.tl_read().await;
    info!("Config encryption root {}: {}", erk, response.as_hci_event());
    assert_eq!(*response.payload(), [0x01, 0x0C, 0xFC, 0x00]);
    is_command_complete(response.as_hci_event()).unwrap();

    mbox.ble_subsystem
        .set_tx_power_level(PowerLevel::ZerodBm)
        .await;
    let response = mbox.ble_subsystem.tl_read().await;
    info!("Config tx power level: {}", response.as_hci_event());
    assert_eq!(*response.payload(), [0x01, 0x0F, 0xFC, 0x00]);
    is_command_complete(response.as_hci_event()).unwrap();

    mbox.ble_subsystem.init_gatt().await;
    let response = mbox.ble_subsystem.tl_read().await;
    info!("GATT init: {}", response.as_hci_event());
    is_command_complete(response.as_hci_event()).unwrap();

    mbox.ble_subsystem
        .init_gap(Role::PERIPHERAL, false, BLE_GAP_DEVICE_NAME_LENGTH)
        .await;
    let response = mbox.ble_subsystem.tl_read().await;
    info!("GAP init: {}", response.as_hci_event());
    is_command_complete(response.as_hci_event()).unwrap();

    mbox.ble_subsystem
        .set_io_capability(IoCapability::DisplayConfirm)
        .await;
    let response = mbox.ble_subsystem.tl_read().await;
    info!("Set IO capabilities: {}", response.as_hci_event());
    assert_eq!(*response.payload(), [0x01, 0x85, 0xFC, 0x00]);
    is_command_complete(response.as_hci_event()).unwrap();

    mbox.ble_subsystem
        .set_authentication_requirement(&AuthenticationRequirements {
            bonding_required: false,
            mitm_protection_required: false,
            secure_connection_support: SecureConnectionSupport::Optional,
            keypress_notification_support: false,
            encryption_key_size_range: (8, 16),
            fixed_pin: Pin::Requested,
            identity_address_type: AddressType::Public,
        })
        .await
        .unwrap();
    let response = mbox.ble_subsystem.tl_read().await;
    info!("Set auth capabilities: {}", response.as_hci_event());
    is_command_complete(response.as_hci_event()).unwrap();
    assert_eq!(*response.payload(), [0x01, 0x86, 0xFC, 0x00]);

    mbox.ble_subsystem
        .le_set_scan_response_data(b"LUMEN_RSP")
        .await
        .unwrap();
    let response = mbox.ble_subsystem.tl_read().await;
    info!("Set scan response data: {}", response.as_hci_event());
    is_command_complete(response.as_hci_event()).unwrap();
    assert_eq!(*response.payload(), [0x01, 0x09, 0x20, 0x00]);

    info!("Initializing GATT services");
    let ble_context = init_gatt_services(&mut mbox.ble_subsystem).await.unwrap();
    info!("BLE Context: {}", ble_context);

    let discovery_params = DiscoverableParameters {
        advertising_type: AdvertisingType::ConnectableUndirected,
        advertising_interval: Some((Duration::from_millis(100), Duration::from_millis(100))),
        address_type: OwnAddressType::Public,
        filter_policy: AdvertisingFilterPolicy::AllowConnectionAndScan,
        local_name: Some(LocalName::Complete(b"LUMEN_01")),
        advertising_data: b"BUY LUMEN",
        conn_interval: (None, None),
    };

    mbox.ble_subsystem
        .set_discoverable(&discovery_params)
        .await
        .unwrap();
    let response = mbox.ble_subsystem.tl_read().await;
    is_command_complete(response.as_hci_event()).unwrap();
    info!("Set discoverable: {}", response.as_hci_event());
    assert_eq!(*response.payload(), [0x01, 0x83, 0xFC, 0x00]);

    Ok(())
}

pub async fn ble_loop(ble: &Ble) {
    loop {
        let response = ble.tl_read().await;
        match response.as_hci_event() {
            Event::LeConnectionComplete(connection_complete) => {
                info!("{}", connection_complete);
            }
            Event::DisconnectionComplete(disconnect_complete) => {
                info!("{}", disconnect_complete);
            }
            Event::LePhyUpdateComplete(update_complete) => {
                info!("{}", update_complete)
            }
            Event::LeDataLengthChangeEvent(change_event) => {
                info!("{}", change_event)
            }
            Event::CommandComplete(command_complete) => {
                info!("{}", command_complete)
            }
            Event::CommandStatus(command_status) => {
                info!("{}", command_status)
            }
            Event::Vendor(vendor_event) => {
                info!("{}", vendor_event)
            }
            event => {
                info!("Other event: {}", event)
            }
        }
    }
}

fn get_bd_addr() -> BdAddr {
    let mut bytes = [0u8; 6];

    let lhci_info = LhciC1DeviceInformationCcrp::new();
    bytes[0] = (lhci_info.uid64 & 0xff) as u8;
    bytes[1] = ((lhci_info.uid64 >> 8) & 0xff) as u8;
    bytes[2] = ((lhci_info.uid64 >> 16) & 0xff) as u8;
    bytes[3] = lhci_info.device_type_id;
    bytes[4] = (lhci_info.st_company_id & 0xff) as u8;
    bytes[5] = (lhci_info.st_company_id >> 8 & 0xff) as u8;

    BdAddr(bytes)
}

fn get_random_addr() -> BdAddr {
    let mut bytes = [0u8; 6];

    let lhci_info = LhciC1DeviceInformationCcrp::new();
    bytes[0] = (lhci_info.uid64 & 0xff) as u8;
    bytes[1] = ((lhci_info.uid64 >> 8) & 0xff) as u8;
    bytes[2] = ((lhci_info.uid64 >> 16) & 0xff) as u8;
    bytes[3] = 0;
    bytes[4] = 0x6E;
    bytes[5] = 0xED;

    BdAddr(bytes)
}

const BLE_CFG_IRK: [u8; 16] = [
    0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0,
];
const BLE_CFG_ERK: [u8; 16] = [
    0xfe, 0xdc, 0xba, 0x09, 0x87, 0x65, 0x43, 0x21, 0xfe, 0xdc, 0xba, 0x09, 0x87, 0x65, 0x43, 0x21,
];

fn get_irk() -> EncryptionKey {
    EncryptionKey(BLE_CFG_IRK)
}

fn get_erk() -> EncryptionKey {
    EncryptionKey(BLE_CFG_ERK)
}
