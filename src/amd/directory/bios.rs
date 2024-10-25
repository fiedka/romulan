use alloc::string::ToString;
use alloc::{boxed::Box, string::String, vec::Vec};
use core::fmt::{self, Display};
use core::mem;
use serde::{Deserialize, Serialize};
use zerocopy::{AsBytes, FromBytes, LayoutVerified as LV};

use super::{AddrMode, ComboDirectoryEntry, ComboDirectoryHeader, DirectoryHeader};

// From coreboot commit 30cf1551683810504f7823e42d4cb6515459cff8:
// > In modern AMD systems, the PSP brings up DRAM then uncompresses the
// > BIOS image into memory prior to x86 beginning execution.
// > The PSP supports a zlib engine, and interprets the first 256 bytes as a
// > header, where offset 0x14 containing the uncompressed size.
// > For further details, see AMD Platform Security Processor BIOS Architecture
// > Design Guide for AMD Family 17h Processors (NDA only, #55758).
#[derive(AsBytes, FromBytes, Clone, Copy, Debug)]
#[repr(C)]
pub struct BiosBinaryHeader {
    pub _00: u32,
    pub _04: u32,
    pub _08: u32,
    pub _0d: u32,
    pub _10: u32,
    pub size: u32, // the _uncompressed_ size
    pub _rest: [u8; 232],
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[repr(u8)]
pub enum BiosEntryType {
    BiosBinary = 0x62,
    BiosLevel2Dir = 0x70,
}

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

// TODO: resolve flags
impl Display for BiosDirectoryEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = self.kind;
        let sub = self.sub_program;
        let rk = self.region_kind;
        let desc = self.description();
        let fl = self.flags;

        let size = self.size;
        let src = self.source;
        let d = self.destination;
        let dest = if d == 0xffff_ffff_ffff_ffff || d == 0x0000_0000_0000_0000 {
            String::from("")
        } else {
            format!(" -> {:08x}", self.destination)
        };
        let v = format!("{size:08x} @ 0x{src:08x}{dest:12}");

        write!(f, "{kind:02x}.{sub:02x}.{rk:02x} {desc:40} {fl:08b} {v}")
    }
}

const BIOS_HEADER_SIZE: usize = mem::size_of::<BiosBinaryHeader>();

// https://en.wikipedia.org/wiki/List_of_file_signatures
const ZLIB_DEFAULT_COMPRESSION_MAGIC: u16 = 0x789c;
const ZLIB_BEST_COMPRESSION_MAGIC: u16 = 0x78da;

// TODO: this was the original value - but it errors for some entries...
// From my observsation, it never fits.
// const BIOS_ENTRY_MASK: usize = 0x01FF_FFFF;
const BIOS_ENTRY_MASK: usize = 0x00FF_FFFF;

impl BiosDirectoryEntry {
    pub fn data(&self, data: &[u8], offset: usize) -> Result<Box<[u8]>, String> {
        let start = self.addr(offset);
        let s = if self.kind == BiosEntryType::BiosBinary as u8 && self.is_compressed() {
            let b = start + BIOS_HEADER_SIZE;
            let d = [data[b], data[b + 1]];
            let magic = u16::from_be_bytes(d);
            // NOTE: The flag in the directory entry may be wrong. While this
            // is not complete, a malformed header may cause other issues.
            match magic {
                ZLIB_DEFAULT_COMPRESSION_MAGIC | ZLIB_BEST_COMPRESSION_MAGIC => {
                    match BiosBinaryHeader::read_from_prefix(&data[start..]) {
                        Some(h) => h.size as usize + BIOS_HEADER_SIZE,
                        None => return Err(format!("could not parse BIOS entry header @ {b:08x}")),
                    }
                }
                _ => return Err(format!("no zlib magic @ {b:08x} ({magic:02x})")),
            }
        } else {
            self.size as usize
        };
        let end = start + s;
        let len = data.len();
        if end <= len {
            Ok(data[start..end].to_vec().into_boxed_slice())
        } else {
            let r = format!("{start:08x}:{end:08x}");
            Err(format!("{self} invalid: range {r} exceeds size {len:08x}"))
        }
    }

    pub fn addr(&self, offset: usize) -> usize {
        let v = self.source as usize;
        match self.addr_mode() {
            AddrMode::PhysAddr => v & BIOS_ENTRY_MASK,
            AddrMode::FlashOffset => v & BIOS_ENTRY_MASK,
            AddrMode::DirHeaderOffset => offset + (v & BIOS_ENTRY_MASK),
            // TODO: PartitionOffset
            _ => v,
        }
    }

    pub fn addr_mode(&self) -> AddrMode {
        match self.source >> 62 {
            0 => AddrMode::PhysAddr,
            1 => AddrMode::FlashOffset,
            2 => AddrMode::DirHeaderOffset,
            3 => AddrMode::PartitionOffset,
            _ => unreachable!(),
        }
    }

    pub fn is_compressed(&self) -> bool {
        (self.flags & 0x1) == 1
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
    pub addr: usize,
    pub header: DirectoryHeader,
    pub entries: Vec<BiosDirectoryEntry>,
}

impl<'a> BiosDirectory {
    pub fn new(data: &'a [u8], addr: usize) -> Result<Self, String> {
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
                addr,
                header,
                entries: entries.to_vec(),
            });
        }

        Err("BIOS directory header not found".to_string())
    }

    pub fn header(&self) -> DirectoryHeader {
        self.header
    }

    pub fn entries(&self) -> Vec<BiosDirectoryEntry> {
        // so much for zero copy... do we ever needs this though?
        self.entries.clone()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[repr(C)]
pub struct BiosComboDirectory {
    pub addr: usize,
    pub header: ComboDirectoryHeader,
    pub entries: Vec<ComboDirectoryEntry>,
}

impl<'a> BiosComboDirectory {
    pub fn new(data: &'a [u8], addr: usize) -> Result<Self, String> {
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
                addr,
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
