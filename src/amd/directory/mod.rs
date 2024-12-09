use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::fmt::{self, Display};
use core::str;
use serde::{Deserialize, Serialize};
use zerocopy::{AsBytes, FromBytes};

// TODO: generate test fixtures with https://github.com/amd/firmware_binaries
// and coreboot util/amdfwtool (coreboot has a copy of the AMD binaries repo)

pub use self::bios::*;
pub use self::psp::*;

mod bios;
mod psp;

#[derive(Clone, Debug)]
pub enum Directory {
    Bios(BiosDirectory),
    BiosLevel2(BiosDirectory),
    BiosCombo(BiosComboDirectory),
    Psp(PspDirectory),
    PspLevel2(PspDirectory),
    PspCombo(PspComboDirectory),
}

use core::mem::discriminant as tag;
impl PartialEq<Self> for Directory {
    fn eq(&self, rhs: &Self) -> bool {
        tag(self) == tag(rhs)
    }
}

impl Display for Directory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = self.get_magic().to_le_bytes();
        let m = str::from_utf8(&b).unwrap();
        write!(f, "{m}",)
    }
}

impl<'a> Directory {
    pub fn new(data: &'a [u8], addr: usize) -> Result<Self, String> {
        match &data[..4] {
            b"$BHD" => BiosDirectory::new(data, addr).map(Self::Bios),
            b"2BHD" => BiosComboDirectory::new(data, addr).map(Self::BiosCombo),
            b"$BL2" => BiosDirectory::new(data, addr).map(Self::BiosLevel2),
            b"$PSP" => PspDirectory::new(data, addr).map(Self::Psp),
            b"2PSP" => PspComboDirectory::new(data, addr).map(Self::PspCombo),
            b"$PL2" => PspDirectory::new(data, addr).map(Self::PspLevel2),
            unknown => Err(format!(
                "unknown directory signature {:02x?} @ {addr:08x}",
                unknown
            )),
        }
    }

    pub fn get_checksum(&self) -> u32 {
        match self {
            Directory::Bios(d) => d.header.checksum,
            Directory::BiosCombo(d) => d.header.checksum,
            Directory::BiosLevel2(d) => d.header.checksum,
            Directory::Psp(d) => d.header.checksum,
            Directory::PspCombo(d) => d.header.checksum,
            Directory::PspLevel2(d) => d.header.checksum,
        }
    }

    pub fn get_magic(&self) -> u32 {
        match self {
            Directory::Bios(d) => d.header.magic,
            Directory::BiosCombo(d) => d.header.magic,
            Directory::BiosLevel2(d) => d.header.magic,
            Directory::Psp(d) => d.header.magic,
            Directory::PspCombo(d) => d.header.magic,
            Directory::PspLevel2(d) => d.header.magic,
        }
    }

    pub fn get_combo_header(&self) -> Result<&ComboDirectoryHeader, String> {
        match self {
            Directory::BiosCombo(d) => Ok(&d.header),
            Directory::PspCombo(d) => Ok(&d.header),
            _ => Err("not a combo directory".to_string()),
        }
    }

    pub fn get_combo_entries(&self) -> Result<&Vec<ComboDirectoryEntry>, String> {
        match self {
            Directory::BiosCombo(d) => Ok(&d.entries),
            Directory::PspCombo(d) => Ok(&d.entries),
            _ => Err("not a combo directory".to_string()),
        }
    }

    pub fn get_bios_entries(&self) -> Result<&Vec<BiosDirectoryEntry>, String> {
        match self {
            Directory::Bios(d) => Ok(&d.entries),
            _ => Err("not a BIOS directory".to_string()),
        }
    }

    pub fn get_psp_entries(&self) -> Result<&Vec<PspDirectoryEntry>, String> {
        match self {
            Directory::Psp(d) => Ok(&d.entries),
            _ => Err("not a PSP directory".to_string()),
        }
    }
}

#[derive(AsBytes, FromBytes, Clone, Copy, Debug, Deserialize, Serialize)]
#[repr(C)]
pub struct DirectoryHeader {
    /// "$BHD", "$BL2", "$PSP" or "$PL2"
    pub magic: u32,
    /// Fletcher32 of all directory data after this
    pub checksum: u32,
    /// number of entries
    pub entries: u32,
    pub _0c: u32,
}

impl Display for DirectoryHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "directory has {} entries, checksum {:08x}",
            self.checksum, self.entries
        )
    }
}

#[derive(AsBytes, FromBytes, Clone, Copy, Debug, Deserialize, Serialize)]
#[repr(C)]
pub struct ComboDirectoryHeader {
    /// "2BHD" or "2PSP"
    pub magic: u32,
    /// Fletcher32 of all directory data after this
    pub checksum: u32,
    /// number of entries
    pub entries: u32,
    /// Only for PSP combo directory:
    /// - 0 for dynamic look up through all entries,
    /// - 1 for PSP or chip ID match.
    pub look_up_mode: u32,
    pub _10: u32,
    pub _14: u32,
    pub _18: u32,
    pub _1c: u32,
}

impl Display for ComboDirectoryHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "directory has {} entries, checksum {:08x}, mode {}",
            self.entries, self.checksum, self.look_up_mode
        )
    }
}

#[derive(AsBytes, FromBytes, Clone, Copy, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct PspOrFamId(u32);

impl PartialEq for PspOrFamId {
    fn eq(&self, rhs: &Self) -> bool {
        self.0 == rhs.0
    }
}

impl Display for PspOrFamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self.0 {
            0x0000_0000 => "Carrizo".to_string(), // TODO: really?!
            0x1022_0B00 => "Stoneyridge".to_string(),
            0xbc09_0000 => "(maybe Summit Ridge; seen on A300 3.60S + X570)".to_string(),
            0xBC0A_0000 => "Raven Ridge or Picasso".to_string(),
            // Matisse somewhere here? Pinnacle Ridge? Castle Peak?
            0xbc0a_0100 => {
                "(maybe Pinnacle Ridge or Matisse/2; seen on A300 3.60K + X570)".to_string()
            }
            // Dali? Matisse? ...
            0xbc0b_0500 => "(maybe Vermeer; seen on ASRock A520M + X370)".to_string(),
            0xBC0C_0000 => "Renoir or Lucienne".to_string(),
            0xBC0C_0111 => "Genoa".to_string(),
            0xBC0C_0140 => "Cezanne".to_string(),
            0xBC0D_0400 => "Phoenix".to_string(),
            0xBC0D_0900 => "Mendocino".to_string(),
            0xBC0E_0200 => "Glinda".to_string(),
            // TODO: Vermeer, Rembrandt...?
            _ => format!("unknown ({:08x})", self.0),
        };
        write!(f, "{s}")
    }
}

#[derive(AsBytes, FromBytes, Clone, Copy, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct ComboDirectoryEntry {
    /// 0 to compare PSP ID, 1 to compare chip family ID
    pub id_select: u32,
    pub id: PspOrFamId,
    /// Address of directory
    pub directory: u64,
}

impl Display for ComboDirectoryEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sel = match self.id_select {
            0 => "PSP ID",
            1 => "Fam ID",
            // this should not occur
            _ => "(ID meaning unknown)",
        };
        write!(f, "{sel} {} @ {:08x}", self.id, self.directory)
    }
}

#[derive(Debug)]
pub enum AddrMode {
    PhysAddr,
    FlashOffset,
    DirHeaderOffset,
    PartitionOffset,
}
