use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
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

    pub fn get_checksum(self: &Self) -> Result<u32, String> {
        match self {
            Directory::Bios(d) => Ok(d.header.checksum),
            Directory::BiosCombo(d) => Ok(d.header.checksum),
            Directory::BiosLevel2(d) => Ok(d.header.checksum),
            Directory::Psp(d) => Ok(d.header.checksum),
            Directory::PspCombo(d) => Ok(d.header.checksum),
            Directory::PspLevel2(d) => Ok(d.header.checksum),
        }
    }

    pub fn get_entries(self: &Self) -> Result<Vec<ComboDirectoryEntry>, String> {
        match self {
            Directory::BiosCombo(d) => {
                let mut es = Vec::<ComboDirectoryEntry>::new();
                for e in d.entries() {
                    es.push(e)
                }
                Ok(es)
            }
            Directory::PspCombo(d) => {
                let mut es = Vec::<ComboDirectoryEntry>::new();
                for e in d.entries() {
                    es.push(e)
                }
                Ok(es)
            }
            _ => Err("not a combo directory".to_string()),
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
