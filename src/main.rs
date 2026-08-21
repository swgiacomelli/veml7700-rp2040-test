#![no_std]
#![no_main]

//! RP2040 + VEML7700 bring-up: USB CDC console over the native USB port,
//! I2C0 on GP4 (SDA) / GP5 (SCL).
//!
//! Sensor access is provided by the async `ph-veml7700-als` driver.
//!
//! Console commands (CR or LF terminated):
//!   help          list commands
//!   scan          probe every 7-bit address, list responders
//!   evidence      print setup metadata and raw registers without configuring the sensor
//!   id            read the VEML7700 ID register (0x07)
//!   conf          read back ALS_CONF
//!   read          one ALS + WHITE raw sample
//!   stream        continuous ALS samples until any key is pressed

mod commands; // this firmware
mod console; // copy this directory into a new app as `mod console;`

use commands::{BoardInfo, VemlApp};
use console::usb::{self, CdcAcmDevice};
use console::{CdcAcmTransport, Console, match_commands};
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::i2c::{self, I2c};
use embassy_rp::peripherals::{I2C0, USB};
use embassy_rp::usb::InterruptHandler as UsbIrq;
use panic_halt as _;

bind_interrupts!(struct Irqs {
    I2C0_IRQ => i2c::InterruptHandler<I2C0>;
    USBCTRL_IRQ => UsbIrq<USB>;
});

const VEML7700_ADDRESS: u8 = 0x10;
const I2C_FREQUENCY_HZ: u32 = 100_000;

match_commands! {
    <I2C: embedded_hal_async::i2c::I2c> for VemlApp<'_, I2C> {
        "scan", "probe every 7-bit address, list responders" => cmd_scan,
        "evidence", "print setup metadata and raw registers without configuring the sensor" => cmd_evidence,
        "id", "read the VEML7700 ID register (0x07)" => cmd_id,
        "conf", "read back ALS_CONF" => cmd_conf,
        "read", "one ALS + WHITE raw sample" => cmd_read,
        "stream", "continuous ALS samples until any key is pressed" => cmd_stream,
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // ---- I2C0: GP4 = SDA, GP5 = SCL. 100 kHz for perfboard bring-up.
    // Note embassy-rp takes SCL first.
    let mut cfg = i2c::Config::default();
    cfg.frequency = I2C_FREQUENCY_HZ;
    let mut bus = I2c::new_async(p.I2C0, p.PIN_5, p.PIN_4, Irqs, cfg);

    // ---- USB CDC-ACM.
    let mut usb_cfg = embassy_usb::Config::new(0xc0de, 0xcafe);
    usb_cfg.manufacturer = Some("photon-circus");
    usb_cfg.product = Some("veml7700-console");
    usb_cfg.serial_number = Some("0001");
    usb_cfg.max_power = 100;
    usb_cfg.max_packet_size_0 = 64;

    let CdcAcmDevice { class, usb } = usb::new(p.USB, Irqs, usb_cfg);
    spawner.must_spawn(usb::usb_task(usb));

    let mut transport = CdcAcmTransport::new(class);
    let mut app = VemlApp::new(
        &mut bus,
        BoardInfo {
            mcu: "RP2040",
            i2c_controller: "I2C0",
            sda: "GP4",
            scl: "GP5",
            i2c_frequency_hz: I2C_FREQUENCY_HZ,
            i2c_address_7bit: VEML7700_ADDRESS,
        },
    );
    let console = Console::new("veml7700 console. type 'help'.", "> ");

    loop {
        transport.wait_connection().await;
        let _ = console.run(&mut transport, &mut app).await;
    }
}
