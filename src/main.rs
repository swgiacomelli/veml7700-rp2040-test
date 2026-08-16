#![no_std]
#![no_main]

//! RP2040 + VEML7700 bring-up: USB CDC console over the native USB port,
//! I2C0 on GP4 (SDA) / GP5 (SCL).
//!
//! Raw register access is used here deliberately so this is independent of
//! any driver crate — swap `sensor::*` for your own driver once it lands.
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
use embassy_time::{Duration, Timer};
use embassy_usb::Builder;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::driver::EndpointError;
use embedded_hal_async::i2c::I2c as _;
use heapless::String;
use panic_halt as _;
use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
    I2C0_IRQ => i2c::InterruptHandler<I2C0>;
    USBCTRL_IRQ => UsbIrq<USB>;
});

// ---------------------------------------------------------------------------
// VEML7700 register-level bits (datasheet + app note DS 84286)
// ---------------------------------------------------------------------------
mod sensor {
    pub const ADDR: u8 = 0x10; // fixed, no address pins

    pub const REG_ALS_CONF: u8 = 0x00;
    pub const REG_POWER_SAVING: u8 = 0x03;
    pub const REG_ALS: u8 = 0x04;
    pub const REG_WHITE: u8 = 0x05;
    pub const REG_ID: u8 = 0x07; // low byte reads 0x81

    /// gain x1/8, IT 100 ms, persistence 1, INT off, powered on.
    /// Widest dynamic range — the sane default for an unknown light level.
    pub const CONF_STARTUP: u16 = 0b10 << 11;

    /// lux per count for gain x1/8 @ 100 ms integration.
    pub const RESOLUTION: f32 = 0.2304;
}

/// 16-bit registers are little-endian: [cmd, lsb, msb].
async fn reg_write(
    bus: &mut I2c<'static, I2C0, Async>,
    reg: u8,
    val: u16,
) -> Result<(), i2c::Error> {
    bus.write(sensor::ADDR, &[reg, val as u8, (val >> 8) as u8])
        .await
}

async fn reg_read(bus: &mut I2c<'static, I2C0, Async>, reg: u8) -> Result<u16, i2c::Error> {
    let mut buf = [0u8; 2];
    bus.write_read(sensor::ADDR, &[reg], &mut buf).await?;
    Ok(u16::from_le_bytes(buf))
}

// ---------------------------------------------------------------------------

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

    // ---- Sensor power-on. Datasheet asks for >=2.5 ms after the shutdown bit
    // clears, plus one full integration period before the first valid sample.
    let _ = reg_write(&mut bus, sensor::REG_POWER_SAVING, 0x0000).await;
    let _ = reg_write(&mut bus, sensor::REG_ALS_CONF, sensor::CONF_STARTUP).await;
    Timer::after(Duration::from_millis(150)).await;

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
            match reg_read(bus, sensor::REG_ID).await {
                Ok(v) => {
                    let _ = write!(out, "id = 0x{:04x} (low byte should be 0x81)\r\n", v);
                }
                Err(_) => {
                    let _ = write!(out, "i2c error reading id\r\n");
                }
            }
            write_str(class, &out).await?;
        }
        "conf" => {
            match reg_read(bus, sensor::REG_ALS_CONF).await {
                Ok(v) => {
                    let gain = (v >> 11) & 0b11;
                    let it = (v >> 6) & 0b1111;
                    let sd = v & 1;
                    let _ = write!(
                        out,
                        "conf = 0x{:04x}  gain_bits={:02b} it_bits={:04b} shutdown={}\r\n",
                        v, gain, it, sd
                    );
                }
                Err(_) => {
                    let _ = write!(out, "i2c error reading conf\r\n");
                }
            }
            write_str(class, &out).await?;
        }
        "read" => {
            let als = reg_read(bus, sensor::REG_ALS).await;
            let white = reg_read(bus, sensor::REG_WHITE).await;
            match (als, white) {
                (Ok(a), Ok(w)) => {
                    let lux = a as f32 * sensor::RESOLUTION;
                    let _ = write!(
                        out,
                        "als={:5}  white={:5}  ~{} lux (uncorrected)\r\n",
                        a, w, lux as u32
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
                        match reg_read(bus, sensor::REG_ALS).await {
                            Ok(a) => {
                                let _ = write!(
                                    out,
                                    "als={:5}  ~{} lux\r\n",
                                    a,
                                    (a as f32 * sensor::RESOLUTION) as u32
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
