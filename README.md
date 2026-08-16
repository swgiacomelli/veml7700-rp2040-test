# VEML7700 RP2040 console

An Embassy-based RP2040 firmware crate for bringing up a VEML7700 ambient
light sensor. It exposes a USB CDC serial console and uses I2C0 on GP4 (SDA)
and GP5 (SCL).

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
commands.
