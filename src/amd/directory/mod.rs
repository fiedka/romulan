use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::fmt::{self, Display};
use core::str;
use serde::{Deserialize, Serialize};
use zerocopy::{AsBytes, FromBytes};

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

impl Display for Directory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = self.get_magic().to_le_bytes();
        let m = str::from_utf8(&b).unwrap();
        write!(f, "{m}",)
    }
}

impl<'a> Directory {
    pub fn new(data: &'a [u8]) -> Result<Self, String> {
        match &data[..4] {
            b"$BHD" => BiosDirectory::new(data).map(Self::Bios),
            b"2BHD" => BiosComboDirectory::new(data).map(Self::BiosCombo),
            b"$BL2" => BiosDirectory::new(data).map(Self::BiosLevel2),
            b"$PSP" => PspDirectory::new(data).map(Self::Psp),
            b"2PSP" => PspComboDirectory::new(data).map(Self::PspCombo),
            b"$PL2" => PspDirectory::new(data).map(Self::PspLevel2),
            unknown => Err(format!("unknown directory signature {:02x?}", unknown)),
        }
    }

    pub fn get_checksum(self: &Self) -> u32 {
        match self {
            Directory::Bios(d) => d.header.checksum,
            Directory::BiosCombo(d) => d.header.checksum,
            Directory::BiosLevel2(d) => d.header.checksum,
            Directory::Psp(d) => d.header.checksum,
            Directory::PspCombo(d) => d.header.checksum,
            Directory::PspLevel2(d) => d.header.checksum,
        }
    }

    pub fn get_magic(self: &Self) -> u32 {
        match self {
            Directory::Bios(d) => d.header.magic,
            Directory::BiosCombo(d) => d.header.magic,
            Directory::BiosLevel2(d) => d.header.magic,
            Directory::Psp(d) => d.header.magic,
            Directory::PspCombo(d) => d.header.magic,
            Directory::PspLevel2(d) => d.header.magic,
        }
    }

    pub fn get_combo_entries(self: &Self) -> Result<&Vec<ComboDirectoryEntry>, String> {
        match self {
            Directory::BiosCombo(d) => Ok(&d.entries),
            Directory::PspCombo(d) => Ok(&d.entries),
            _ => Err("not a combo directory".to_string()),
        }
    }

    pub fn get_bios_entries(self: &Self) -> Result<&Vec<BiosDirectoryEntry>, String> {
        match self {
            Directory::Bios(d) => Ok(&d.entries),
            _ => Err("not a BIOS directory".to_string()),
        }
    }

    pub fn get_psp_entries(self: &Self) -> Result<&Vec<PspDirectoryEntry>, String> {
        match self {
            Directory::Psp(d) => Ok(&d.entries),
            _ => Err("not a PSP directory".to_string()),
        }
    }
}

#[derive(AsBytes, FromBytes, Clone, Copy, Debug, Deserialize, Serialize)]
#[repr(C)]
pub struct DirectoryHeader {
    /// 0x00: Magic of directory ("$BHD" or "$PSP")
    pub magic: u32,
    /// 0x04: CRC of all directory data after this
    pub checksum: u32,
    /// 0x08: number of entries
    pub entries: u32,
    pub rsvd_0c: u32,
}

#[derive(AsBytes, FromBytes, Clone, Copy, Debug, Deserialize, Serialize)]
#[repr(C)]
pub struct ComboDirectoryHeader {
    /// 0x00: Magic of directory ("2BHD" or "2PSP")
    pub magic: u32,
    /// 0x04: CRC of all directory data after this
    pub checksum: u32,
    /// 0x08: number of entries
    pub entries: u32,
    /// 0x0c: 0 for dynamic look up through all entries, 1 for PSP or chip ID match.
    /// Only for PSP combo directory
    pub look_up_mode: u32,
    pub rsvd_10: u32,
    pub rsvd_14: u32,
    pub rsvd_18: u32,
    pub rsvd_1c: u32,
}

#[derive(AsBytes, FromBytes, Clone, Copy, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct ComboDirectoryEntry {
    /// 0x00: 0 to compare PSP ID, 1 to compare chip ID
    pub id_select: u32,
    /// 0x04: PSP or chip ID
    pub id: u32,
    /// 0x08: Address of directory
    pub directory: u64,
}

impl Display for ComboDirectoryEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:08x} @ {:08x}", self.id, self.directory)
    }
}
