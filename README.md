# VEML7700 RP2040 console

An Embassy-based RP2040 firmware crate for bringing up a VEML7700 ambient
light sensor. It exposes a USB CDC serial console, uses I2C0 on GP4 (SDA) and
GP5 (SCL), and accesses the sensor through the async
[`ph-veml7700-als`](https://github.com/photon-circus/ph-veml7700-als) driver.

## Wiring

| RP2040 | VEML7700 |
| --- | --- |
| 3V3 | VIN |
| GND | GND |
| GP4 | SDA |
| GP5 | SCL |

Use I2C pull-up resistors if they are not already present on the sensor board.

## Build and flash with UF2

Install the UF2 converter once:

```console
cargo install elf2uf2-rs
```

Hold the board's BOOTSEL button while connecting USB (or reset it into the
USB bootloader), then build and deploy the UF2 image:

```console
cargo run --release
```

The configured Cargo runner converts the ELF to UF2 and copies it to the
mounted `RPI-RP2` drive.

To create a UF2 file for manual copying instead, run:

```console
cargo build --release
elf2uf2-rs target/thumbv6m-none-eabi/release/veml7700-console veml7700-console.uf2
```

Connect to the USB CDC serial port and enter `help` to list the console
commands. The `read` and `stream` commands perform controlled one-shot captures
using the driver's maximum-range starting configuration and report its nominal
integer illuminance in millilux.

## Recording hardware evidence

Run `evidence` before any command that configures the sensor, ideally immediately
after a complete sensor power cycle. It prints the known firmware-side setup and
the raw little-endian bytes and word for registers `0x00` through `0x07`. The
command deliberately bypasses typed register decoding so unexpected reserved
bits remain visible. It writes no register values, although register reads may
still have undocumented device-side effects.

The output is useful only when accompanied by the physical setup, conditions,
procedure, and original serial log. Use [HARDWARE_EVIDENCE.md](HARDWARE_EVIDENCE.md)
as the record and experiment guide. In particular, the firmware cannot determine
the exact RP2040 board revision, sensor breakout and silicon markings, rail
voltage, pull-up resistance, wiring geometry, temperature, or optical reference.
