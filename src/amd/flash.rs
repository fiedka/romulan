// SPDX-License-Identifier: MIT
use serde::{Deserialize, Serialize};
use zerocopy::{AsBytes, FromBytes, Unaligned};

/// Embedded Firmware Structure
///
/// https://doc.coreboot.org/soc/amd/psp_integration.html
#[derive(AsBytes, Unaligned, FromBytes, Clone, Copy, Debug, Serialize, Deserialize)]
#[repr(packed)]
pub struct EFS {
    /// 0x00: Magic of EFS (0x55AA55AA)
    pub magic: u32,

    /* Special firmware */
    pub imc_fw: u32,
    pub gbe_fw: u32,
    pub xhci_fw: u32,

    /* PSP */
    /// 0x10: PSP directory for ...
    pub psp_legacy: u32,
    /// 0x14: PSP directory for family 17 models 00 and later
    pub psp_17_00: u32,

    /* "BIOS" */
    /// 0x18: BIOS directory for family 17 models 00 to 0f
    pub bios_17_00_0f: u32,
    /// 0x1c: BIOS directory for family 17 models 10 to 1f
    pub bios_17_10_1f: u32,
    /// 0x20: BIOS directory for family 17 models 30 to 3f and family 19 models 00 to 0f
    pub bios_17_30_3f_19_00_0f: u32,
    /// 0x24: bit 0 is set to 0 if this is a second generation structure
    pub second_gen: u32,
    /// 0x28: BIOS directory for family 17 model 60 and later
    pub bios_17_60: u32,
    pub _2c: u32,

    /* Promontory */
    /// 0x30: promontory firmware
    pub promontory: u32,
    /// 0x34: low power promontory firmware
    pub lp_promontory: u32,
    pub _38: u32,
    pub _3c: u32,

    /* SPI flash */
    /// 0x40: SPI mode for family 15 models 60 to 6f
    pub spi_mode_15_60_6f: u8,
    /// 0x41: SPI speed for family 15 models 60 to 6f
    pub spi_speed_15_60_6f: u8,
    pub _42: u8,

    /// 0x43: SPI mode for family 17 models 00 to 1f
    pub spi_mode_17_00_1f: u8,
    /// 0x44: SPI speed for family 17 models 00 to 1f
    pub spi_speed_17_00_1f: u8,
    /// 0x45: Micron flag (0x0A for Micron, 0xFF otherwise) for family 17 models 00 to 1f
    pub micron_17_00_1f: u8,
    pub _46: u8,

    /// 0x47: SPI mode for family 17 model 30 and later
    pub spi_mode_17_30: u8,
    /// 0x48: SPI speed for family 17 model 30 and later
    pub spi_speed_17_30: u8,
    /// 0x49: Micron flag (0xAA for Micron, 0x55 for automatic) for family 17 model 30 and later
    pub micron_17_30: u8,
    pub _4a: u8,
}
