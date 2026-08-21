//! VEML7700 bring-up commands on top of the reusable console.

use embassy_futures::select::{Either, select};
use embassy_time::{Delay, Duration, Timer};
use embedded_hal_async::i2c::I2c;
use ph_veml7700_als::{MeasurementConfig, Veml7700};

use crate::console::{Transport, writeln};

const MEASUREMENT_CONFIG: MeasurementConfig = MeasurementConfig::maximum_range_start();

/// Board wiring and I2C setup printed by `evidence`.
pub struct BoardInfo {
    pub mcu: &'static str,
    pub i2c_controller: &'static str,
    pub sda: &'static str,
    pub scl: &'static str,
    pub i2c_frequency_hz: u32,
    pub i2c_address_7bit: u8,
}

pub struct VemlApp<'a, I2C> {
    bus: &'a mut I2C,
    board: BoardInfo,
}

impl<'a, I2C> VemlApp<'a, I2C> {
    pub fn new(bus: &'a mut I2C, board: BoardInfo) -> Self {
        Self { bus, board }
    }
}

impl<I2C> VemlApp<'_, I2C>
where
    I2C: I2c,
{
    pub(crate) async fn cmd_scan<T: Transport>(&mut self, io: &mut T) -> Result<(), T::Error> {
        let mut probe = [0u8; 1];
        for addr in 0x08u8..0x78 {
            if self.bus.read(addr, &mut probe).await.is_ok() {
                writeln!(io, "  found 0x{:02x}", addr).await?;
            }
        }
        writeln!(io, "scan done").await
    }

    pub(crate) async fn cmd_evidence<T: Transport>(&mut self, io: &mut T) -> Result<(), T::Error> {
        // Read raw words directly so observations of undocumented/reserved
        // bits are not lost to the driver's typed decoders. This command
        // writes no register values, but selecting/reading a register could
        // still have an undocumented device-side effect.
        writeln!(io, "BEGIN VEML7700_EVIDENCE v1").await?;
        writeln!(io, "mcu={}", self.board.mcu).await?;
        writeln!(io, "i2c_controller={}", self.board.i2c_controller).await?;
        writeln!(io, "sda={}", self.board.sda).await?;
        writeln!(io, "scl={}", self.board.scl).await?;
        writeln!(io, "i2c_frequency_hz={}", self.board.i2c_frequency_hz).await?;
        writeln!(io, "i2c_address_7bit=0x{:02x}", self.board.i2c_address_7bit).await?;

        let address = self.board.i2c_address_7bit;
        for register in 0x00u8..=0x07 {
            let mut bytes = [0u8; 2];
            match self.bus.write_read(address, &[register], &mut bytes).await {
                Ok(()) => {
                    let raw = u16::from_le_bytes(bytes);
                    writeln!(
                        io,
                        "register=0x{:02x},lsb=0x{:02x},msb=0x{:02x},word=0x{:04x}",
                        register, bytes[0], bytes[1], raw
                    )
                    .await?;
                }
                Err(_) => {
                    writeln!(io, "register=0x{:02x},error=i2c", register).await?;
                }
            }
        }
        writeln!(io, "END VEML7700_EVIDENCE").await
    }

    pub(crate) async fn cmd_id<T: Transport>(&mut self, io: &mut T) -> Result<(), T::Error> {
        let mut sensor = Veml7700::new(&mut *self.bus);
        match sensor.read_device_id().await {
            Ok(id) => {
                writeln!(
                    io,
                    "id=0x{:04x} device_code=0x{:02x} supported={}",
                    id.raw(),
                    id.device_code(),
                    id.is_supported()
                )
                .await
            }
            Err(_) => writeln!(io, "i2c error reading id").await,
        }
    }

    pub(crate) async fn cmd_conf<T: Transport>(&mut self, io: &mut T) -> Result<(), T::Error> {
        let mut sensor = Veml7700::new(&mut *self.bus);
        match sensor.read_configuration().await {
            Ok(config) => {
                writeln!(
                    io,
                    "gain={:?} integration={}ms power={:?} monitor={:?}",
                    config.measurement.gain(),
                    config.measurement.integration_time().milliseconds(),
                    config.power_state,
                    config.threshold_monitor
                )
                .await
            }
            Err(_) => writeln!(io, "i2c error reading conf").await,
        }
    }

    pub(crate) async fn cmd_read<T: Transport>(&mut self, io: &mut T) -> Result<(), T::Error> {
        let mut sensor = Veml7700::new(&mut *self.bus);
        let mut delay = Delay;
        match sensor.measure_once(&mut delay, MEASUREMENT_CONFIG).await {
            Ok(measurement) => {
                writeln!(
                    io,
                    "als={:5} white={:5} nominal={} mLux",
                    measurement.als.counts(),
                    measurement.white.counts(),
                    measurement.nominal_illuminance.milli_lux_rounded()
                )
                .await
            }
            _ => writeln!(io, "i2c error").await,
        }
    }

    pub(crate) async fn cmd_stream<T: Transport>(&mut self, io: &mut T) -> Result<(), T::Error> {
        writeln!(io, "streaming — press any key to stop").await?;
        let mut sink = [0u8; 64];
        loop {
            let tick = Timer::after(Duration::from_millis(250));
            match select(tick, io.read(&mut sink)).await {
                Either::First(_) => {
                    let mut sensor = Veml7700::new(&mut *self.bus);
                    let mut delay = Delay;
                    match sensor.measure_once(&mut delay, MEASUREMENT_CONFIG).await {
                        Ok(measurement) => {
                            writeln!(
                                io,
                                "als={:5} nominal={} mLux",
                                measurement.als.counts(),
                                measurement.nominal_illuminance.milli_lux_rounded()
                            )
                            .await?;
                        }
                        Err(_) => writeln!(io, "i2c error").await?,
                    }
                }
                Either::Second(Err(e)) => return Err(e),
                Either::Second(Ok(_)) => break,
            }
        }
        Ok(())
    }
}
