# VEML7700 hardware evidence record

This file is a capture template for observations that may be proposed to the
[`ph-veml7700-als` hardware evidence registry](https://github.com/photon-circus/ph-veml7700-als/blob/main/docs/HARDWARE_CONTRACT.md).
One successful reading is useful bring-up information, but it is not by itself
silicon qualification or calibrated optical evidence.

## Physical setup

Fill in facts that the firmware cannot observe. Do not replace measurements with
nominal values from a product page.

- Observation date/time and time zone:
- Operator:
- RP2040 board make, model, revision, and identifying markings:
- VEML7700 IC top marking and, if known, lot/date code:
- Sensor breakout make, model, and revision, or schematic/BOM:
- Sensor `V_DD`, measured value, instrument, and uncertainty:
- SDA/SCL pull-up rail, measured value, instrument, and uncertainty:
- SDA and SCL pull-up resistance (including parallel on-board pull-ups):
- Wiring type and approximate SDA/SCL length:
- Power source and power-cycle method:
- Ambient temperature and measurement method:
- Optical source, geometry, diffuser/cover window, and stabilization time:
- Reference light meter, calibration status, range, and uncertainty:
- Firmware Git commit and UF2 digest:
- `ph-veml7700-als` version or Git commit:

The checked-in firmware configuration is RP2040 I2C0, GP4 SDA, GP5 SCL,
100 kHz, and 7-bit address `0x10`. Verify the flashed commit and record any local
changes rather than assuming those values apply to a captured log.

## Baseline capture

1. Disconnect power from the sensor long enough to obtain a real power-on state.
2. Apply power and avoid all console commands except `evidence`.
3. Save the complete, unedited `BEGIN VEML7700_EVIDENCE v1` through
   `END VEML7700_EVIDENCE` block.
4. Repeat across independent power cycles and identify each physical sensor.

The raw register `0x03` observation can provide evidence about registry item
`S-11` (the otherwise undocumented complete power-on word). The ID word and
address response can corroborate the tested population, but already documented
identity and bus behavior should not be presented as new proof merely because a
single board worked.

## Experiments that would address unresolved items

These require a controlled procedure or additional firmware; the baseline dump
alone does not resolve them.

| Registry item | Observation needed |
| --- | --- |
| `S-22` | Measure power-saving refresh intervals at several gains while holding mode, integration time, source, and temperature constant. |
| `S-55` | Timestamp the first genuinely new conversion after wake for every integration time over repeated trials; report timer resolution and the method used to distinguish new data. |
| `S-51`, `S-52` | Sweep a measured optical source through and beyond the nominal full-scale range and retain every raw sample; distinguish clipping, wrapping, and source/reference limits. |
| `S-49`, `S-50` | Apply controlled above/below-threshold sequences for each persistence setting and log every conversion and status read. |
| `S-53` | Read status repeatedly without another conversion or configuration write and record whether either flag changes. |
| `S-54` | Record status immediately before and after disabling and re-enabling threshold monitoring, with optical input controlled. |

For every experiment, retain raw logs, sample counts, repetitions, negative and
contrary observations, instrument details, uncertainty, and the exact procedure.
Avoid conclusions broader than the observed sensor population and conditions.

## Raw log

Paste or attach the original serial output here. Do not edit anomalous values.

```text

```
