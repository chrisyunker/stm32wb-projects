use embassy_stm32_wpan::hci::{
    event::command::{CommandComplete, ReturnParameters},
    vendor::{
        command::gatt::{
            AddCharacteristicParameters, AddServiceParameters, CharacteristicEvent,
            CharacteristicPermission, CharacteristicProperty, EncryptionKeySize, GattCommands,
            ServiceType, UpdateCharacteristicValueParameters, Uuid,
        },
        event::{self, command::VendorReturnParameters, AttributeHandle},
    },
    Event,
};
use embassy_stm32_wpan::sub::ble::Ble;

pub struct CharHandles {
    pub read: AttributeHandle,
    pub write: AttributeHandle,
    pub notify: AttributeHandle,
}

impl defmt::Format for CharHandles {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "CharHandles {{ read: {:x}, write: {:x} notify: {:x} }}",
            self.read,
            self.write,
            self.notify,
        )
    }
}

pub struct BleContext {
    pub service_handle: AttributeHandle,
    pub chars: CharHandles,
    pub is_subscribed: bool,
}

impl defmt::Format for BleContext {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "BleContext {{ service_handle: {}, char_handles: {} is_subscribed: {} }}",
            self.service_handle,
            self.chars,
            self.is_subscribed,
        )
    }
}

pub async fn init_gatt_services(ble_subsystem: &mut Ble) -> Result<BleContext, ()> {
    let service_handle = gatt_add_service(ble_subsystem, Uuid::Uuid16(0x500)).await?;

    let read = gatt_add_char(
        ble_subsystem,
        service_handle,
        Uuid::Uuid16(0x501),
        CharacteristicProperty::READ,
        Some(b"Hello from embassy"),
    )
    .await?;

    let write = gatt_add_char(
        ble_subsystem,
        service_handle,
        Uuid::Uuid16(0x502),
        CharacteristicProperty::WRITE_WITHOUT_RESPONSE
            | CharacteristicProperty::WRITE
            | CharacteristicProperty::READ,
        None,
    )
    .await?;

    let notify = gatt_add_char(
        ble_subsystem,
        service_handle,
        Uuid::Uuid16(0x503),
        CharacteristicProperty::NOTIFY | CharacteristicProperty::READ,
        None,
    )
    .await?;

    Ok(BleContext {
        service_handle,
        chars: CharHandles {
            read,
            write,
            notify,
        },
        is_subscribed: false,
    })
}

pub async fn gatt_add_service(ble_subsystem: &mut Ble, uuid: Uuid) -> Result<AttributeHandle, ()> {
    ble_subsystem
        .add_service(&AddServiceParameters {
            uuid,
            service_type: ServiceType::Primary,
            max_attribute_records: 8,
        })
        .await;
    let response = ble_subsystem.tl_read().await;

    if let Event::CommandComplete(CommandComplete {
        return_params:
            ReturnParameters::Vendor(VendorReturnParameters::GattAddService(
                event::command::GattService { service_handle, .. },
            )),
        ..
    }) = response.as_hci_event()
    {
        Ok(service_handle)
    } else {
        Err(())
    }
}

pub async fn gatt_add_char(
    ble_subsystem: &mut Ble,
    service_handle: AttributeHandle,
    characteristic_uuid: Uuid,
    characteristic_properties: CharacteristicProperty,
    default_value: Option<&[u8]>,
) -> Result<AttributeHandle, ()> {
    ble_subsystem
        .add_characteristic(&AddCharacteristicParameters {
            service_handle,
            characteristic_uuid,
            characteristic_value_len: 32,
            characteristic_properties,
            security_permissions: CharacteristicPermission::empty(),
            gatt_event_mask: CharacteristicEvent::all(),
            encryption_key_size: EncryptionKeySize::with_value(7).unwrap(),
            is_variable: true,
        })
        .await;
    let response = ble_subsystem.tl_read().await;
    //info!("gatt_add_char Response: {:x}", response.payload());
    //assert_eq!(*response.payload(), [0x01, 0x04, 0xFD, 0x00, 0x11, 0x00]);

    if let Event::CommandComplete(CommandComplete {
        return_params:
            ReturnParameters::Vendor(VendorReturnParameters::GattAddCharacteristic(
                event::command::GattCharacteristic {
                    characteristic_handle,
                    ..
                },
            )),
        ..
    }) = response.as_hci_event()
    {
        if let Some(value) = default_value {
            ble_subsystem
                .update_characteristic_value(&UpdateCharacteristicValueParameters {
                    service_handle,
                    characteristic_handle,
                    offset: 0,
                    value,
                })
                .await
                .unwrap();
            let response = ble_subsystem.tl_read().await;
            //info!("Option Response: {:x}", response.payload());
            assert_eq!(*response.payload(), [0x01, 0x06, 0xFD, 0x00]);
        }
        //info!("gatt_add_char CHAR HANDLE: {:x}", characteristic_handle.0);
        Ok(characteristic_handle)
    } else {
        Err(())
    }
}
