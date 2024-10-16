// SPDX-License-Identifier: MIT
use alloc::string::ToString;
use core::fmt::{self, Display};
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
    /// coreboot util/amdfwtool says: introduced after RAVEN/PICASSO
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
    /// 0x40: SPI flash configuration for family 15 models 60 to 6f
    pub spi_cfg_15_60_6f: SpiCfg,
    pub _42: u8,
    /// 0x43: SPI flash configuration for family 17 models 00 to 1f
    pub spi_cfg_17_00_1f: SpiCfg2,
    pub _46: u8,
    /// 0x47: SPI flash configuration for family 17 model 30 and later
    pub spi_cfg_17_30: SpiCfg3,
    pub _4a: u8,
}

pub fn get_real_addr(addr: u32) -> Option<u32> {
    if addr == 0x0000_0000 || addr == 0xffff_ffff {
        None
    } else {
        Some(addr)
    }
}

impl Display for EFS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let efs = self;
        writeln!(f, ": EFS :")?;

        let is_gen2 = efs.second_gen & 0x1 == 0;
        writeln!(f, ":: Second gen? {is_gen2}")?;

        writeln!(f, ":: Firmware ::")?;
        let a = get_real_addr(efs.imc_fw);
        writeln!(f, " IMC Firmware                                  {a:08x?}")?;
        let a = get_real_addr(efs.gbe_fw);
        writeln!(f, " Gigabit ethernet firmware                     {a:08x?}")?;
        let a = get_real_addr(efs.xhci_fw);
        writeln!(f, " XHCI firmware                                 {a:08x?}")?;
        let a = get_real_addr(efs.bios_17_00_0f);
        writeln!(f, " Fam 17 Model 00-0f BIOS                       {a:08x?}")?;
        let a = get_real_addr(efs.bios_17_10_1f);
        writeln!(f, " Fam 17 Model 00-0f BIOS                       {a:08x?}")?;
        let a = get_real_addr(efs.bios_17_30_3f_19_00_0f);
        writeln!(f, " Fam 17 Model 30-0f + Fam 19 Model 00-0f BIOS  {a:08x?}")?;
        let a = get_real_addr(efs.bios_17_60);
        writeln!(f, " Fam 17 Model 60+ BIOS                         {a:08x?}")?;
        let a = get_real_addr(efs.psp_legacy);
        writeln!(f, " PSP legacy (before Fam 17)                    {a:08x?}")?;
        let a = get_real_addr(efs.psp_17_00);
        writeln!(f, " PSP modern (Fam 17 and later)                 {a:08x?}")?;
        let a = get_real_addr(efs.promontory);
        writeln!(f, " Promontory firmware                           {a:08x?}")?;
        let a = get_real_addr(efs.lp_promontory);
        writeln!(f, " LP Promontory firmware                        {a:08x?}")?;

        writeln!(f, ":: SPI flash configuration ::")?;
        let s1 = efs.spi_cfg_15_60_6f;
        let s2 = efs.spi_cfg_17_00_1f;
        let s3 = efs.spi_cfg_17_30;
        writeln!(f, " Fam 15 Models 60-6f         {s1}")?;
        writeln!(f, " Fam 17 Models 00-1f         {s2}")?;
        writeln!(f, " Fam 17 Models 30 and later  {s3}")
    }
}

#[derive(AsBytes, FromBytes, Clone, Copy, Debug, Deserialize, Serialize)]
#[repr(C)]
pub struct SpiMode(u8);

// see coreboot util/amdfwtool/amdfwtool
impl Display for SpiMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self.0 {
            0 => "Normal (up to 33M)".to_string(),
            1 => "Reserved (error?)".to_string(),
            2 => "Dual IO (1-1-2)".to_string(),
            3 => "Quad IO (1-1-4)".to_string(),
            4 => "Dual IO (1-2-2)".to_string(),
            5 => "Quad IO (1-4-4)".to_string(),
            6 => "Normal (up to 66M)".to_string(),
            7 => "Fast Read".to_string(),
            _ => format!("unknown ({:02x})", self.0),
        };
        write!(f, "{s:18}")
    }
}

#[derive(AsBytes, FromBytes, Clone, Copy, Debug, Deserialize, Serialize)]
#[repr(C)]
pub struct SpiSpeed(u8);

impl Display for SpiSpeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self.0 {
            0 => "66.66Mhz".to_string(),
            1 => "33.33Mhz".to_string(),
            2 => "22.22Mhz".to_string(),
            3 => "16.66MHz".to_string(),
            4 => "100MHz".to_string(),
            5 => "800KHz".to_string(),
            _ => format!("unknown ({:02x})", self.0),
        };
        write!(f, "{s:12}")
    }
}

#[derive(AsBytes, FromBytes, Clone, Copy, Debug, Deserialize, Serialize)]
#[repr(C)]
pub struct Micron(u8);

impl Display for Micron {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self.0 {
            0x0A => "always".to_string(),
            0xFF => "unused".to_string(),
            _ => format!("unknown ({:02x})", self.0),
        };
        write!(f, "{s}")
    }
}

#[derive(AsBytes, FromBytes, Clone, Copy, Debug, Deserialize, Serialize)]
#[repr(C)]
pub struct Micron2(u8);

impl Display for Micron2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self.0 {
            0xAA => "always".to_string(),
            0x55 => "automatic".to_string(),
            0xFF => "unused".to_string(),
            _ => format!("unknown ({:02x})", self.0),
        };
        write!(f, "{s}")
    }
}

#[derive(AsBytes, FromBytes, Clone, Copy, Debug, Deserialize, Serialize)]
#[repr(C)]
pub struct SpiCfg {
    pub mode: SpiMode,
    pub speed: SpiSpeed,
}

#[derive(AsBytes, FromBytes, Clone, Copy, Debug, Deserialize, Serialize)]
#[repr(C)]
pub struct SpiCfg2 {
    pub mode: SpiMode,
    pub speed: SpiSpeed,
    pub micron: Micron,
}

#[derive(AsBytes, FromBytes, Clone, Copy, Debug, Deserialize, Serialize)]
#[repr(C)]
pub struct SpiCfg3 {
    pub mode: SpiMode,
    pub speed: SpiSpeed,
    pub micron: Micron2,
}

impl Display for SpiCfg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "mode: {} speed: {}", self.mode, self.speed)
    }
}

impl Display for SpiCfg2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "mode: {} speed: {} micron: {}",
            self.mode, self.speed, self.micron
        )
    }
}

impl Display for SpiCfg3 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "mode: {} speed: {} micron: {}",
            self.mode, self.speed, self.micron
        )
    }
}
