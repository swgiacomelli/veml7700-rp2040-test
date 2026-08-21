//! Embassy-rp CDC-ACM device bring-up.
//!
//! VID/PID, strings, and other [`embassy_usb::Config`] fields stay with the
//! caller so another firmware can reuse this helper unchanged.

use embassy_rp::interrupt::typelevel::{Binding, USBCTRL_IRQ};
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_usb::Builder;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use static_cell::StaticCell;

const CDC_MAX_PACKET_SIZE: u16 = 64;

/// USB CDC-ACM class plus the device that must be polled on [`usb_task`].
pub struct CdcAcmDevice {
    pub class: CdcAcmClass<'static, Driver<'static, USB>>,
    pub usb: embassy_usb::UsbDevice<'static, Driver<'static, USB>>,
}

/// Build a single CDC-ACM function on the RP2040 native USB port.
pub fn new(
    usb: USB,
    irq: impl Binding<USBCTRL_IRQ, InterruptHandler<USB>>,
    config: embassy_usb::Config<'static>,
) -> CdcAcmDevice {
    let driver = Driver::new(usb, irq);

    static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static CTRL_BUF: StaticCell<[u8; 64]> = StaticCell::new();
    static CDC_STATE: StaticCell<State> = StaticCell::new();

    let mut builder = Builder::new(
        driver,
        config,
        CONFIG_DESC.init([0; 256]),
        BOS_DESC.init([0; 256]),
        &mut [],
        CTRL_BUF.init([0; 64]),
    );

    let class = CdcAcmClass::new(
        &mut builder,
        CDC_STATE.init(State::new()),
        CDC_MAX_PACKET_SIZE,
    );
    let usb = builder.build();
    CdcAcmDevice { class, usb }
}

#[embassy_executor::task]
pub async fn usb_task(mut usb: embassy_usb::UsbDevice<'static, Driver<'static, USB>>) -> ! {
    usb.run().await
}
