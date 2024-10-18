use alloc::{boxed::Box, string::String, vec::Vec};
use core::fmt::{self, Display};
use core::mem;
use serde::{Deserialize, Serialize};
use zerocopy::{AsBytes, FromBytes, LayoutVerified as LV};

use super::{ComboDirectoryEntry, ComboDirectoryHeader, DirectoryHeader};

#[derive(AsBytes, FromBytes, Clone, Copy, Debug, Deserialize, Serialize)]
#[repr(C)]
pub struct BiosDirectoryEntry {
    /// 0x00: type of entry
    pub kind: u8,
    /// 0x01: memory region security attributes
    pub region_kind: u8,
    /// 0x02: flags (specific to type of entry)
    pub flags: u8,
    /// 0x03: used to filter entries by model
    pub sub_program: u8,
    /// 0x04: size of the entry
    pub size: u32,
    /// 0x08: source address
    pub source: u64,
    /// 0x10: destination address
    pub destination: u64,
}

// TODO: flags
impl Display for BiosDirectoryEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let d = self.destination;
        let dest = if d == 0xffff_ffff_ffff_ffff || d == 0x0000_0000_0000_0000 {
            String::from("")
        } else {
            format!(" -> {:08x}", self.destination)
        };
        let desc = self.description();
        let size = self.size;
        let src = self.source;
        let kind = self.kind;
        let sub = self.sub_program;
        let rk = self.region_kind;
        let fl = self.flags;
        write!(
            f,
            "{kind:02x}.{sub:02x}.{rk:02x} {desc:40} {fl:08b} {size:08x} @ 0x{src:08x}{dest}"
        )
    }
}

// TODO: this was the original value - but it errors for some entries...
// const BIOS_ENTRY_MASK: usize = 0x01FF_FFFF;
const BIOS_ENTRY_MASK: usize = 0x00FF_FFFF;

impl BiosDirectoryEntry {
    pub fn data(&self, data: &[u8]) -> Result<Box<[u8]>, String> {
        let start = BIOS_ENTRY_MASK & self.source as usize;
        let end = start + self.size as usize;
        if end <= data.len() {
            Ok(data[start..end].to_vec().into_boxed_slice())
        } else {
            Err(format!(
                "BIOS directory entry invalid: {:08X}:{:08X}",
                start, end
            ))
        }
    }

    pub fn instance(&self) -> u8 {
        (self.flags >> 4) & 0xF
    }

    // PMU: platform measurement unit or platform management unit?
    // https://docs.amd.com/r/en-US/ug1085-zynq-ultrascale-trm/Low-Power-Operation-Mode
    pub fn description(&self) -> &'static str {
        match self.kind {
            0x05 => "BIOS Signing Key",
            0x07 => "BIOS Signature",
            0x60 => "AGESA PSP Customization Block",
            0x61 => "AGESA PSP Output Block",
            0x62 => "BIOS Binary",
            0x63 => "AGESA PSP Output Block NVRAM",
            0x64 => match self.instance() {
                0x01 => "PMU Firmware Code (DDR4 UDIMM 1D)",
                0x02 => "PMU Firmware Code (DDR4 RDIMM 1D)",
                0x03 => "PMU Firmware Code (DDR4 LRDIMM 1D)",
                0x04 => "PMU Firmware Code (DDR4 2D)",
                0x05 => "PMU Firmware Code (DDR4 2D Diagnostic)",
                _ => "PMU Firmware Code (Unknown)",
            },
            0x65 => match self.instance() {
                0x01 => "PMU Firmware Data (DDR4 UDIMM 1D)",
                0x02 => "PMU Firmware Data (DDR4 RDIMM 1D)",
                0x03 => "PMU Firmware Data (DDR4 LRDIMM 1D)",
                0x04 => "PMU Firmware Data (DDR4 2D)",
                0x05 => "PMU Firmware Data (DDR4 2D Diagnostic)",
                _ => "PMU Firmware Data (Unknown)",
            },
            0x66 => "Microcode",
            0x67 => "Machine Check Exception Data",
            0x68 => "AGESA PSP Customization Block Backup",
            0x6A => "MP2 Firmware",
            0x6D => "Maybe NVAR (seen in ASRock A520M-HVS)",
            0x70 => "BIOS Level 2 Directory",
            _ => "Unknown",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BiosDirectory {
    pub header: DirectoryHeader,
    pub entries: Vec<BiosDirectoryEntry>,
}

impl<'a> BiosDirectory {
    pub fn new(data: &'a [u8]) -> Result<Self, String> {
        if &data[..4] == b"$BHD" || &data[..4] == b"$BL2" {
            let header =
                DirectoryHeader::read_from_prefix(data).ok_or("BIOS directory header invalid")?;

            let hs = mem::size_of::<DirectoryHeader>();
            let (entries, _) = LV::<_, [BiosDirectoryEntry]>::new_slice_from_prefix(
                &data[hs..],
                header.entries as usize,
            )
            .ok_or("BIOS directory entries invalid")?;

            return Ok(Self {
                header,
                entries: entries.to_vec(),
            });
        }

        Err(format!("BIOS directory header not found"))
    }

    pub fn header(&self) -> DirectoryHeader {
        self.header
    }

    pub fn entries(&self) -> Vec<BiosDirectoryEntry> {
        self.entries.clone() // so much for zero copy
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[repr(C)]
pub struct BiosComboDirectory {
    pub header: ComboDirectoryHeader,
    pub entries: Vec<ComboDirectoryEntry>,
}

impl<'a> BiosComboDirectory {
    pub fn new(data: &'a [u8]) -> Result<Self, String> {
        if &data[..4] == b"2BHD" {
            let header =
                ComboDirectoryHeader::read_from_prefix(data).ok_or("BIOS combo header invalid")?;
            let hs = mem::size_of::<ComboDirectoryHeader>();
            let (entries, _) = LV::<_, [ComboDirectoryEntry]>::new_slice_from_prefix(
                &data[hs..],
                header.entries as usize,
            )
            .ok_or("BIOS combo entries invalid")?;

            return Ok(Self {
                header,
                entries: entries.to_vec(),
            });
        }

        Err(format!("BIOS combo header not found"))
    }

    pub fn header(&self) -> ComboDirectoryHeader {
        self.header
    }

    pub fn entries(&self) -> Vec<ComboDirectoryEntry> {
        self.entries.clone()
    }
}
