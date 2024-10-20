use alloc::string::{String, ToString};
use alloc::{boxed::Box, vec::Vec};
use core::fmt::{self, Display};
use core::mem;
use serde::{Deserialize, Serialize};
use zerocopy::{AsBytes, FromBytes, LayoutVerified as LV};

use super::{ComboDirectoryEntry, ComboDirectoryHeader, DirectoryHeader};

#[derive(AsBytes, FromBytes, Clone, Copy, Debug, Deserialize, Serialize)]
#[repr(C)]
pub struct PspDirectoryEntry {
    /// 0x00: type of entry
    pub kind: u8,
    /// 0x01: used to filter entries by model
    pub sub_program: u8,
    /// 0x02: specifies which ROM contains the entry
    pub rom_id: u8,
    pub _03: u8,
    /// 0x04: size of the entry
    pub size: u32,
    /// 0x08: address mode and location or value of the entry
    pub value: u64,
}

const ADDR_MASK: usize = 0x3FFF_FFFF;

#[derive(Debug)]
pub enum AddrMode {
    PhysAddr,
    FlashOffset,
    DirHeaderOffset,
    PartitionOffset,
}

// FIXME: mask per SoC generation
const MAPPING_MASK: usize = 0x00ff_ffff;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[repr(u8)]
pub enum PspEntryType {
    SoftFuseChain = 0x0b,
    PspLevel2Dir = 0x40,
    PspLevel2ADir = 0x48,
    PspLevel2BDir = 0x4a,
}

impl Display for PspDirectoryEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = self.kind;
        let sub = self.sub_program;
        let desc = self.description();
        let v = if self.kind == PspEntryType::SoftFuseChain as u8 {
            // It is often just 1 or 0. There may be other bits.
            // From coreboot `src/soc/amd/genoa_poc/Makefile.mk`:
            // See #57299 (NDA) for bit definitions.
            // set-bit=$(call int-shift-left, 1 $(call _toint,$1))
            // PSP_SOFTFUSE=$(shell A=$(call int-add, \
            // $(foreach bit,$(sort $(PSP_SOFTFUSE_BITS)),$(call set-bit,$(bit)))); printf "0x%x" $$A)
            format!("0x{:08x}", self.value)
        } else {
            format!("{:08x} @ {:08x}", self.size, self.value)
        };
        write!(f, "{kind:02x}.{sub} {desc:52} {v:20}",)
    }
}

impl PspDirectoryEntry {
    pub fn data(&self, data: &[u8]) -> Result<Box<[u8]>, String> {
        let value = (self.value as usize) & ADDR_MASK;
        if self.size == 0xFFFF_FFFF {
            return Ok(value.to_le_bytes().to_vec().into_boxed_slice());
        }

        let start = match self.addr_mode() {
            AddrMode::PhysAddr => value & MAPPING_MASK,
            AddrMode::FlashOffset => value,
            _ => value,
        };

        let end = start + self.size as usize;
        let len = data.len();
        if end <= len {
            Ok(data[start..end].to_vec().into_boxed_slice())
        } else {
            let r = format!("{start:08x}:{end:08x}");
            Err(format!("{self} invalid: {r} exceedis size {len:08x}"))
        }
    }

    // https://doc.coreboot.org/soc/amd/psp_integration.html#psp-directory-table-entries
    // coreboot util/amdfwtool/amdfwtool.h
    pub fn addr_mode(&self) -> AddrMode {
        match self.value >> 62 {
            0 => AddrMode::PhysAddr,
            1 => AddrMode::FlashOffset,
            2 => AddrMode::DirHeaderOffset,
            3 => AddrMode::PartitionOffset,
            _ => unreachable!(),
        }
    }

    // SMU binaries should start with "SMURULESSMURULES"
    pub fn description(&self) -> &'static str {
        match self.kind {
            0x00 => "AMD Public Key",
            0x01 => "PSP Boot Loader",
            0x02 => "PSP Secure OS",
            0x03 => "PSP Recovery Boot Loader",
            0x04 => "PSP Non-volatile Data",
            0x05 => "PSP RTM public key",
            0x06 => "Unknown (seen in A3MSTX_3.60K legacy PSP)",
            0x08 => "SMU Firmware",
            0x09 => "AMD Secure Debug Key",
            0x0A => "OEM Public Key",
            0x0B => "PSP Soft Fuse Chain",
            0x0C => "PSP Trustlet",
            0x0D => "PSP Trustlet Public Key",
            0x10 => "Unknown (seen in A3MSTX_3.60K legacy PSP)",
            0x12 => "SMU Firmware 2",
            0x13 => "PSP Early Secure Unlock Debug",
            0x14 => "Unknown (seen in A3MSTX_3.60K legacy PSP)",
            0x1A => "Unknown (seen in A3MSTX_3.60K legacy PSP)",
            0x1B => "Boot Driver",
            0x1C => "SoC_Driver",
            0x1D => "Debug Driver",
            0x1F => "Interface Driver",
            0x20 => "IP Discovery",
            0x21 => "Wrapped iKEK",
            0x22 => "PSP Token Unlock",
            0x24 => "Security Policy",
            0x25 => "MP2 Firmware",
            0x26 => "MP2 Firmware Part 2",
            0x27 => "User Mode Unit Test",
            0x28 => "System Driver",
            0x29 => "KVM Image",
            0x2A => "MP5 Firmware",
            0x2B => "Embedded Firmware Signature",
            0x2C => "TEE Write-once NVRAM",
            0x2D => "External Chipset PSP Boot Loader",
            0x2E => "External Chipset MP0 Firmware",
            0x2F => "External Chipset MP1 Firmware",
            0x30 => "PSP AGESA Binary 0",
            0x31 => "PSP AGESA Binary 1",
            0x32 => "PSP AGESA Binary 2",
            0x33 => "PSP AGESA Binary 3",
            0x34 => "PSP AGESA Binary 4",
            0x35 => "PSP AGESA Binary 5",
            0x36 => "PSP AGESA Binary 6",
            0x37 => "PSP AGESA Binary 7",
            0x38 => "SEV Data",
            0x39 => "SEV Code",
            0x3A => "Processor Serial Number Allow List",
            0x3B => "SERDES Microcode",
            0x3C => "VBIOS Pre-load",
            0x3D => "WLAN Umac",
            0x3E => "WLAN Imac",
            0x3F => "WLAN Bluetooth",
            0x40 => "PSP Level 2 Directory",
            0x41 => "External Chipset MP0 Boot Loader",
            0x42 => "DXIO PHY SRAM Firmware",
            0x43 => "DXIO PHY SRAM Firmware Public Key",
            0x44 => "USB PHY Firmware",
            0x45 => "Security Policy for tOS",
            0x46 => "External Chipset PSP Boot Loader",
            0x47 => "DRTM TA",
            0x48 => "Recovery L2A PSP Directory",
            0x49 => "Recovery L2 BIOS Directory",
            0x4A => "Recovery L2B PSP Directory",
            0x4C => "External Chipset Security Policy",
            0x4D => "External Chipset Secure Debug Unlock",
            0x4E => "PMU Public Key",
            0x4F => "UMC Firmware",
            0x50 => "PSP Boot Loader Public Keys Table",
            0x51 => "PSP tOS Public Keys Table",
            0x52 => "OEM PSP Boot Loader Application",
            0x53 => "OEM PSP Boot Loader Application Public Key",
            0x54 => "PSP RPMC NVRAM",
            // SPL table file in coreboot src/soc/amd/genoa_poc/Makefile.mk
            0x55 => "PSP Boot Loader Anti-rollback",
            0x56 => "PSP Secure OS Anti-rollback",
            0x57 => "CVIP Configuration Table",
            0x58 => "DMCU-ERAM",
            0x59 => "DMCU-ISR",
            0x5A => "MSMU Binary 0",
            0x5B => "MSMU Binary 1",
            0x5C => "SPI ROM Configuration",
            0x5D => "MPIO",
            0x5F => "PSP SMU SCS (Fam. 15h+16h), TPM lite (Fam. 17h+19h)",
            /* 0x60 - 0x70 are BIOS directory types, maybe unused here */
            0x71 => "DMCUB",
            0x73 => "PSP Boot Loader AB",
            0x76 => "RIB",
            0x80 => "OEM Sys-TA",
            0x81 => "OEM Sys-TA Signing Key",
            0x85 => "FW AMF SRAM",
            0x86 => "FW AMF DRAM",
            0x88 => "FW AMF WLAN",
            0x89 => "FW AMF MFD",
            0x8C => "FW MPDMA TF",
            0x8D => "TA IKEK",
            0x90 => "FW MPCCX",
            0x91 => "FW GMI3 PHY",
            0x92 => "FW MPDMA PM",
            0x94 => "FW LSDMA",
            0x95 => "FW C20 MP",
            0x98 => "FW FCFG TABLE",
            0x9A => "FW MINIMSMU",
            0x9D => "FW SRAM FW EXT",
            0xA2 => "FW UMSMU",
            _ => "Unknown",
        }
    }
}

// TODO: What are the other fields? coreboot util/amdfwtool...?
#[derive(AsBytes, FromBytes, Clone, Copy, Debug, Deserialize, Serialize)]
#[repr(C)]
pub struct PspBackupDir {
    pub _00: u32,
    pub _04: u32,
    pub _08: u32,
    pub _0c: u32,
    pub addr: u32,
    pub _14: u32,
}

impl<'a> PspBackupDir {
    pub fn new(data: &'a [u8]) -> Result<Self, String> {
        match Self::read_from_prefix(data) {
            Some(s) => Ok(s),
            None => Err("could not parse PSP backup directory".to_string()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PspDirectory {
    pub header: DirectoryHeader,
    pub entries: Vec<PspDirectoryEntry>,
}

impl Display for PspDirectory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PSP directory checksum {:08x}, {} entries",
            self.header.checksum, self.header.entries
        )
    }
}

impl<'a> PspDirectory {
    pub fn new(data: &'a [u8]) -> Result<Self, String> {
        let m = &data[..4];
        if m == b"$PSP" || m == b"$PL2" {
            let header =
                DirectoryHeader::read_from_prefix(data).ok_or("PSP directory header invalid")?;

            let hs = mem::size_of::<DirectoryHeader>();
            let (entries, _) = LV::<_, [PspDirectoryEntry]>::new_slice_from_prefix(
                &data[hs..],
                header.entries as usize,
            )
            .ok_or("PSP directory entries invalid")?;

            return Ok(Self {
                header,
                entries: entries.to_vec(),
            });
        }

        Err(format!("PSP directory header not found: {m:02x?}"))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[repr(C)]
pub struct PspComboDirectory {
    pub header: ComboDirectoryHeader,
    pub entries: Vec<ComboDirectoryEntry>,
}

impl<'a> PspComboDirectory {
    pub fn new(data: &'a [u8]) -> Result<Self, String> {
        if &data[..4] == b"2PSP" {
            let header =
                ComboDirectoryHeader::read_from_prefix(data).ok_or("PSP combo header invalid")?;

            let hs = mem::size_of::<ComboDirectoryHeader>();
            let (entries, _) = LV::<_, [ComboDirectoryEntry]>::new_slice_from_prefix(
                &data[hs..],
                header.entries as usize,
            )
            .ok_or("PSP combo entries invalid")?;

            return Ok(Self {
                header,
                entries: entries.to_vec(),
            });
        }

        Err(format!("PSP combo header not found"))
    }

    pub fn header(&self) -> ComboDirectoryHeader {
        self.header
    }

    pub fn entries(&self) -> Vec<ComboDirectoryEntry> {
        self.entries.clone()
    }
}
