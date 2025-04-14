#![no_std]
#![no_main]

mod ble;
mod ble_gatt;
use ble::{init_ble, ble_loop};

use core::assert_eq;
use defmt::*;
use embassy_executor::Spawner;
use {defmt_rtt as _, panic_probe as _};

use embassy_stm32::{
    bind_interrupts,
    ipcc::{Config, ReceiveInterruptHandler, TransmitInterruptHandler},
    rcc::WPAN_DEFAULT,
};
use embassy_stm32_wpan::{TlMbox, sub::ble::Ble};

bind_interrupts!(struct Irqs{
    IPCC_C1_RX => ReceiveInterruptHandler;
    IPCC_C1_TX => TransmitInterruptHandler;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("\n\n=======================");
    info!("Start STM32WB");

    let mut config = embassy_stm32::Config::default();
    config.rcc = WPAN_DEFAULT;
    let p = embassy_stm32::init(config);

    let config = Config::default();
    let mut mbox = TlMbox::init(p.IPCC, Irqs, config);

    loop {
        let wireless_fw_info = mbox.sys_subsystem.wireless_fw_info();
        match wireless_fw_info {
            None => info!("Not yet installed"),
            Some(fw_info) => {
                let version_major = fw_info.version_major();
                let version_minor = fw_info.version_minor();
                let subversion = fw_info.subversion();

                let sram2a_size = fw_info.sram2a_size();
                let sram2b_size = fw_info.sram2b_size();

                info!(
                    "version {}.{}.{} - SRAM2a {} - SRAM2b {}",
                    version_major, version_minor, subversion, sram2a_size, sram2b_size
                );
                break;
            }
        }
    }

    let sys_event = mbox.sys_subsystem.read().await;
    assert_eq!(*sys_event.payload(), [0x00, 0x92, 0x00]);
    info!("Read System: {}", sys_event.as_hci_event());
    core::mem::drop(sys_event);

    init_ble(&mut mbox).await.unwrap();
    spawner.spawn(run_ble_loop(mbox.ble_subsystem)).unwrap();

}

#[embassy_executor::task]
async fn run_ble_loop(ble: Ble) {
    ble_loop(&ble).await;
}

