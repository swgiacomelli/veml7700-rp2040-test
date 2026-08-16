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
//!   id            read the VEML7700 ID register (0x07)
//!   conf          read back ALS_CONF
//!   read          one ALS + WHITE raw sample
//!   stream        continuous ALS samples until any key is pressed

use core::fmt::Write as _;

use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_rp::bind_interrupts;
use embassy_rp::i2c::{self, Async, I2c};
use embassy_rp::peripherals::{I2C0, USB};
use embassy_rp::usb::{Driver, InterruptHandler as UsbIrq};
use embassy_time::{Delay, Duration, Timer};
use embassy_usb::Builder;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::driver::EndpointError;
use embedded_hal_async::i2c::I2c as _;
use heapless::String;
use panic_halt as _;
use ph_veml7700_als::{MeasurementConfig, Veml7700};
use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
    I2C0_IRQ => i2c::InterruptHandler<I2C0>;
    USBCTRL_IRQ => UsbIrq<USB>;
});

const MEASUREMENT_CONFIG: MeasurementConfig = MeasurementConfig::maximum_range_start();

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // ---- I2C0: GP4 = SDA, GP5 = SCL. 100 kHz for perfboard bring-up.
    // Note embassy-rp takes SCL first.
    let mut cfg = i2c::Config::default();
    cfg.frequency = 100_000;
    let mut bus = I2c::new_async(p.I2C0, p.PIN_5, p.PIN_4, Irqs, cfg);

    // ---- USB CDC-ACM.
    let driver = Driver::new(p.USB, Irqs);
    let mut usb_cfg = embassy_usb::Config::new(0xc0de, 0xcafe);
    usb_cfg.manufacturer = Some("photon-circus");
    usb_cfg.product = Some("veml7700-console");
    usb_cfg.serial_number = Some("0001");
    usb_cfg.max_power = 100;
    usb_cfg.max_packet_size_0 = 64;

    static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static CTRL_BUF: StaticCell<[u8; 64]> = StaticCell::new();
    static CDC_STATE: StaticCell<State> = StaticCell::new();

    let mut builder = Builder::new(
        driver,
        usb_cfg,
        CONFIG_DESC.init([0; 256]),
        BOS_DESC.init([0; 256]),
        &mut [], // no msos descriptors
        CTRL_BUF.init([0; 64]),
    );

    let mut class = CdcAcmClass::new(&mut builder, CDC_STATE.init(State::new()), 64);
    let usb = builder.build();
    spawner.must_spawn(usb_task(usb));

    loop {
        class.wait_connection().await;
        let _ = console(&mut class, &mut bus).await;
    }
}

#[embassy_executor::task]
async fn usb_task(mut usb: embassy_usb::UsbDevice<'static, Driver<'static, USB>>) -> ! {
    usb.run().await
}

// ---------------------------------------------------------------------------
// Console
// ---------------------------------------------------------------------------

type Cdc = CdcAcmClass<'static, Driver<'static, USB>>;

async fn write_str(class: &mut Cdc, s: &str) -> Result<(), EndpointError> {
    // CDC max packet is 64 bytes; chunk and avoid a zero-length terminator.
    for chunk in s.as_bytes().chunks(63) {
        class.write_packet(chunk).await?;
    }
    Ok(())
}

async fn console(
    class: &mut Cdc,
    bus: &mut I2c<'static, I2C0, Async>,
) -> Result<(), EndpointError> {
    write_str(class, "\r\nveml7700 console. type 'help'.\r\n> ").await?;

    let mut line: [u8; 64] = [0; 64];
    let mut len = 0usize;
    let mut packet = [0u8; 64];

    loop {
        let n = class.read_packet(&mut packet).await?;
        for &b in &packet[..n] {
            match b {
                b'\r' | b'\n' => {
                    write_str(class, "\r\n").await?;
                    let cmd = core::str::from_utf8(&line[..len]).unwrap_or("");
                    dispatch(class, bus, cmd.trim()).await?;
                    len = 0;
                    write_str(class, "> ").await?;
                }
                0x08 | 0x7f => {
                    if len > 0 {
                        len -= 1;
                        write_str(class, "\x08 \x08").await?;
                    }
                }
                0x20..=0x7e => {
                    if len < line.len() {
                        line[len] = b;
                        len += 1;
                        class.write_packet(&[b]).await?; // local echo
                    }
                }
                _ => {}
            }
        }
    }
}

async fn dispatch(
    class: &mut Cdc,
    bus: &mut I2c<'static, I2C0, Async>,
    cmd: &str,
) -> Result<(), EndpointError> {
    let mut out: String<192> = String::new();

    match cmd {
        "" => {}
        "help" => {
            write_str(class, "help | scan | id | conf | read | stream\r\n").await?;
        }
        "scan" => {
            let mut probe = [0u8; 1];
            for addr in 0x08u8..0x78 {
                if bus.read(addr, &mut probe).await.is_ok() {
                    out.clear();
                    let _ = write!(out, "  found 0x{:02x}\r\n", addr);
                    write_str(class, &out).await?;
                }
            }
            write_str(class, "scan done\r\n").await?;
        }
        "id" => {
            let mut sensor = Veml7700::new(&mut *bus);
            match sensor.read_device_id().await {
                Ok(id) => {
                    let _ = write!(
                        out,
                        "id=0x{:04x} device_code=0x{:02x} supported={}\r\n",
                        id.raw(),
                        id.device_code(),
                        id.is_supported()
                    );
                }
                Err(_) => {
                    let _ = write!(out, "i2c error reading id\r\n");
                }
            }
            write_str(class, &out).await?;
        }
        "conf" => {
            let mut sensor = Veml7700::new(&mut *bus);
            match sensor.read_configuration().await {
                Ok(config) => {
                    let _ = write!(
                        out,
                        "gain={:?} integration={}ms power={:?} monitor={:?}\r\n",
                        config.measurement.gain(),
                        config.measurement.integration_time().milliseconds(),
                        config.power_state,
                        config.threshold_monitor
                    );
                }
                Err(_) => {
                    let _ = write!(out, "i2c error reading conf\r\n");
                }
            }
            write_str(class, &out).await?;
        }
        "read" => {
            let mut sensor = Veml7700::new(&mut *bus);
            let mut delay = Delay;
            match sensor.measure_once(&mut delay, MEASUREMENT_CONFIG).await {
                Ok(measurement) => {
                    let _ = write!(
                        out,
                        "als={:5} white={:5} nominal={} mLux\r\n",
                        measurement.als.counts(),
                        measurement.white.counts(),
                        measurement.nominal_illuminance.milli_lux_rounded()
                    );
                }
                _ => {
                    let _ = write!(out, "i2c error\r\n");
                }
            }
            write_str(class, &out).await?;
        }
        "stream" => {
            write_str(class, "streaming — press any key to stop\r\n").await?;
            let mut sink = [0u8; 64];
            loop {
                let tick = Timer::after(Duration::from_millis(250));
                match select(tick, class.read_packet(&mut sink)).await {
                    Either::First(_) => {
                        out.clear();
                        let mut sensor = Veml7700::new(&mut *bus);
                        let mut delay = Delay;
                        match sensor.measure_once(&mut delay, MEASUREMENT_CONFIG).await {
                            Ok(measurement) => {
                                let _ = write!(
                                    out,
                                    "als={:5} nominal={} mLux\r\n",
                                    measurement.als.counts(),
                                    measurement.nominal_illuminance.milli_lux_rounded()
                                );
                            }
                            Err(_) => {
                                let _ = write!(out, "i2c error\r\n");
                            }
                        }
                        write_str(class, &out).await?;
                    }
                    Either::Second(_) => break,
                }
            }
        }
        other => {
            let _ = write!(out, "unknown: {}\r\n", other);
            write_str(class, &out).await?;
        }
    }
    Ok(())
}
