use alloc::string::{String, ToString};
use alloc::{boxed::Box, vec::Vec};
use core::convert::TryFrom;
use core::fmt::{self, Display};
use core::mem;
use serde::{Deserialize, Serialize};
use zerocopy::{AsBytes, FromBytes, LayoutVerified as LV};

use super::{AddrMode, ComboDirectoryEntry, ComboDirectoryHeader, DirectoryHeader};

#[derive(AsBytes, FromBytes, Clone, Copy, Debug)]
#[repr(C)]
pub struct Version {
    major: u8,
    minor: u8,
    patch: u8,
    rev: u8,
}

impl Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let j = self.major;
        let i = self.minor;
        let p = self.patch;
        let r = self.rev;
        if (j == 0 && i == 0 && p == 0 && r == 0)
            || (j == 0xff && i == 0xff && p == 0xff && r == 0xff)
        {
            write!(f, "unversioned")
        } else {
            write!(
                f,
                "{:02x}.{:02x}.{:02x}.{:02x}",
                self.major, self.minor, self.patch, self.rev
            )
        }
    }
}

// NOTE: rustfmt always wants to put comments after the previuos line
#[rustfmt::skip] 
const KNOWN_MAGICS: [&str; 21] = [
    // seen for most binaries
    "$PS1",
    // alternative variants for AGESA binaries
    "0BAB", "0BAW",
    // another variant
    "AC0B", "AC1B", "AC2B", "AC3B", "AC4B", "AC5B", "AC6B", "AC7B", "AC8B",
    // typical for AGESA binaries, but not always
    "AW0B", "AW1B", "AW2B", "AW3B", "AW4B", "AW5B", "AW6B", "AW7B", "AW8B",
];

#[derive(AsBytes, FromBytes, Clone, Copy, Debug)]
#[repr(C)]
pub struct Magic([u8; 4]);

impl Display for Magic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = match core::str::from_utf8(&self.0) {
            Ok(s) => {
                if KNOWN_MAGICS.contains(&s) {
                    s.to_string()
                } else {
                    "".to_string()
                }
            }
            Err(_) => "".to_string(),
        };
        write!(f, "{m:4}")
    }
}

/// coreboot util/amdfwtool/amdfwtool.h, 0x100 size header
#[derive(AsBytes, FromBytes, Clone, Copy, Debug)]
#[repr(C)]
pub struct PspBinarySignature {
    // 1 if the image is signed, 0 otherwise
    pub opt: u32,
    pub id: u32,
    pub param: [u8; 16],
}

impl Display for PspBinarySignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sig_short = if self.opt == 1 {
            let b = [self.param[0], self.param[1]];
            let n = u16::from_be_bytes(b);
            format!("🔐 {n:04x}")
        } else {
            "🔓".to_string()
        };
        write!(f, "{sig_short:6}")
    }
}

/// coreboot util/amdfwtool/amdfwtool.h, 0x100 size header
#[derive(AsBytes, FromBytes, Clone, Copy, Debug)]
#[repr(C)]
pub struct PspBinaryHeader {
    pub _00: [u8; 16],
    pub maybe_magic: Magic,
    pub fw_size_signed: u32,
    pub _10: [u8; 8],
    pub _18: [u8; 16],
    pub sig: PspBinarySignature,
    pub comp_opt: u32,
    pub _4c: [u8; 4],
    pub uncomp_size: u32,
    pub comp_size: u32,
    // Starting with Mendecino, fw_id is populated instead of fw_type
    pub fw_id: u16,
    pub _5a: [u8; 6],
    pub version: Version,
    pub _64: [u8; 8],
    pub size_total: u32,
    pub _70: [u8; 12],
    // fw_type will still be around for backwards compatibility
    pub fw_type: u8,
    pub fw_subtype: u8,
    pub fw_subprog: u8,
    pub reserved_7f: u8,
    pub _rest: [u8; 128],
}

impl Display for PspBinaryHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = self.sig;
        let m = self.maybe_magic;
        let v = self.version;
        write!(f, "{s} {m} {v}")
    }
}

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

// FIXME: mask per SoC generation
pub const MAPPING_MASK: usize = 0x00ff_ffff;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[repr(u8)]
pub enum PspEntryType {
    AmdPublicKey = 0x00,
    PspNonVolatileData = 0x04,
    SmuFirmware = 0x08,
    AmdSecureDebugKey = 0x09,
    OemPublicKey = 0x0a,
    SoftFuseChain = 0x0b,
    PspTrustletPublicKey = 0x0d,
    SmuFirmware2 = 0x12,
    WrappedIKEK = 0x21,
    PspTokenUnlock = 0x22,
    PspLevel2Dir = 0x40,
    DxioPhySramFirmwarePublicKey = 0x43,
    UsbPhyFirmware = 0x44,
    PspLevel2ADir = 0x48,
    BiosLevel2Dir = 0x49,
    PspLevel2BDir = 0x4a,
    PmuPublicKey = 0x4e,
    PspBootLoaderPublicKeysTable = 0x50,
    PspTrustedOSPublicKeysTable = 0x51,
    PspRpmcNvram = 0x54,
    DmcuEram = 0x58,
    DmcuIsr = 0x59,
}

// FIXME: This duplication is very tedious and prone to error.
// It is too easy to forget to add something here that was added to the enum.
impl TryFrom<u8> for PspEntryType {
    type Error = &'static str;

    fn try_from(v: u8) -> Result<PspEntryType, Self::Error> {
        match v {
            0x00 => Ok(PspEntryType::AmdPublicKey),
            0x04 => Ok(PspEntryType::PspNonVolatileData),
            0x08 => Ok(PspEntryType::SmuFirmware),
            0x09 => Ok(PspEntryType::AmdSecureDebugKey),
            0x0a => Ok(PspEntryType::OemPublicKey),
            0x0b => Ok(PspEntryType::SoftFuseChain),
            0x0d => Ok(PspEntryType::PspTrustletPublicKey),
            0x12 => Ok(PspEntryType::SmuFirmware2),
            0x21 => Ok(PspEntryType::WrappedIKEK),
            0x22 => Ok(PspEntryType::PspTokenUnlock),
            0x40 => Ok(PspEntryType::PspLevel2Dir),
            0x43 => Ok(PspEntryType::DxioPhySramFirmwarePublicKey),
            0x44 => Ok(PspEntryType::UsbPhyFirmware),
            0x48 => Ok(PspEntryType::PspLevel2ADir),
            0x49 => Ok(PspEntryType::BiosLevel2Dir),
            0x4a => Ok(PspEntryType::PspLevel2BDir),
            0x4e => Ok(PspEntryType::PmuPublicKey),
            0x50 => Ok(PspEntryType::PspBootLoaderPublicKeysTable),
            0x51 => Ok(PspEntryType::PspTrustedOSPublicKeysTable),
            0x54 => Ok(PspEntryType::PspRpmcNvram),
            0x58 => Ok(PspEntryType::DmcuEram),
            0x59 => Ok(PspEntryType::DmcuIsr),
            _ => Err("unknown PSP entry type"),
        }
    }
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
            format!("{:032b}", self.value)
        } else {
            format!(
                "{:08x} @ {:08x}",
                self.size,
                // TODO: use addressing mode
                self.value as usize & MAPPING_MASK
            )
        };
        write!(f, "{kind:02x}.{sub:02x} {desc:51} {v:20}",)
    }
}

const PSP_BIN_HEADER_SIZE: usize = core::mem::size_of::<PspBinaryHeader>();

impl PspDirectoryEntry {
    pub fn data(
        &self,
        data: &[u8],
        offset: usize,
    ) -> Result<(Option<PspBinaryHeader>, Box<[u8]>), String> {
        let value = (self.value as usize) & ADDR_MASK;
        // So far, this only holds for the Soft Fuse Chain.
        if self.size == 0xFFFF_FFFF {
            let body = value.to_le_bytes().to_vec().into_boxed_slice();
            return Ok((None, body));
        }

        let start = self.addr(offset);
        let end = start + self.size as usize;
        let len = data.len();
        // This should not, but may, occur.
        if end > len {
            let r = format!("{start:08x}:{end:08x}");
            return Err(format!("{self} invalid: {r} exceeds size {len:08x}"));
        }

        // Not all entries have the generic header, so bail out immediately.
        // We have a growing list of exceptions. This may be inaccurate.
        // TODO: In some cases, it differs across revisions/families.
        let res = if self.has_no_generic_header() {
            let body = data[start..end].to_vec().into_boxed_slice();
            (None, body)
        } else {
            let body_start = start + PSP_BIN_HEADER_SIZE;
            // NOTE: It may happen that an entry is not in the list of known
            // exceptions and there is not enough data for a header.
            if body_start > end {
                let body = data[start..end].to_vec().into_boxed_slice();
                (None, body)
            } else {
                // Best effort: Assume the start to be a generic header.
                match PspBinaryHeader::read_from_prefix(&data[start..]) {
                    Some(h) => {
                        let body = data[body_start..end].to_vec().into_boxed_slice();
                        (Some(h), body)
                    }
                    None => {
                        let body = data[start..end].to_vec().into_boxed_slice();
                        (None, body)
                    }
                }
            }
        };
        Ok(res)
    }

    pub fn addr(&self, offset: usize) -> usize {
        let v = self.value as usize;
        match self.addr_mode() {
            AddrMode::PhysAddr => v & MAPPING_MASK,
            AddrMode::FlashOffset => v & MAPPING_MASK,
            AddrMode::DirHeaderOffset => offset + (v & MAPPING_MASK),
            // TODO: PartitionOffset
            _ => v,
        }
    }

    pub fn display(&self, data: &[u8], offset: usize) -> String {
        if self.kind == PspEntryType::SoftFuseChain as u8 {
            // TODO
            let v = "";
            return format!("{self} {v:11}");
        }
        let v = if self.is_dir() {
            "📁".to_string()
        } else {
            match self.data(data, offset) {
                Ok((h, b)) => {
                    if let Some(h) = h {
                        format!("{h}")
                    } else if self.is_sig_key() {
                        let k = u16::from_be_bytes([b[4], b[5]]);
                        format!("🔑 {k:04x}")
                    } else {
                        "🚫".to_string()
                    }
                }
                _ => "🚫".to_string(),
            }
        };
        format!("{self}{v:23}")
    }

    // TODO: extend list of headerless / special entries
    pub fn has_no_generic_header(&self) -> bool {
        let k = PspEntryType::try_from(self.kind);
        matches!(
            k,
            Ok(PspEntryType::AmdPublicKey
                | PspEntryType::PspNonVolatileData
                | PspEntryType::AmdSecureDebugKey
                | PspEntryType::OemPublicKey
                | PspEntryType::PspTrustletPublicKey
                | PspEntryType::WrappedIKEK
                | PspEntryType::PspTokenUnlock
                | PspEntryType::PspLevel2Dir
                | PspEntryType::PspLevel2ADir
                | PspEntryType::PspLevel2BDir
                | PspEntryType::DxioPhySramFirmwarePublicKey
                | PspEntryType::UsbPhyFirmware
                | PspEntryType::PmuPublicKey
                | PspEntryType::PspBootLoaderPublicKeysTable
                | PspEntryType::PspTrustedOSPublicKeysTable
                | PspEntryType::PspRpmcNvram
                | PspEntryType::DmcuEram
                | PspEntryType::DmcuIsr,)
        )
    }

    pub fn is_dir(&self) -> bool {
        let k = PspEntryType::try_from(self.kind);
        matches!(
            k,
            Ok(PspEntryType::PspLevel2Dir
                | PspEntryType::PspLevel2ADir
                | PspEntryType::PspLevel2BDir)
        )
    }
    pub fn is_key(&self) -> bool {
        let k = PspEntryType::try_from(self.kind);
        matches!(
            k,
            Ok(PspEntryType::AmdPublicKey
                | PspEntryType::AmdSecureDebugKey
                | PspEntryType::OemPublicKey
                | PspEntryType::PspTrustletPublicKey
                | PspEntryType::WrappedIKEK
                | PspEntryType::PspTokenUnlock
                | PspEntryType::DxioPhySramFirmwarePublicKey
                | PspEntryType::PmuPublicKey
                | PspEntryType::PspBootLoaderPublicKeysTable
                | PspEntryType::PspTrustedOSPublicKeysTable)
        )
    }

    pub fn is_sig_key(&self) -> bool {
        let k = PspEntryType::try_from(self.kind);
        matches!(
            k,
            Ok(PspEntryType::AmdPublicKey
                | PspEntryType::OemPublicKey
                | PspEntryType::PspTrustletPublicKey
                | PspEntryType::DxioPhySramFirmwarePublicKey
                | PspEntryType::PmuPublicKey)
        )
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
            0x1C => "SoC Driver",
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
            0x51 => "PSP Trusted OS Public Keys Table",
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
            0x93 => "FW RCFG 3328A",
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
    pub addr: usize,
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
    pub fn new(data: &'a [u8], addr: usize) -> Result<Self, String> {
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
                addr,
                header,
                entries: entries.to_vec(),
            });
        }

        Err(format!("PSP directory header not found @ {addr:08x}"))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[repr(C)]
pub struct PspComboDirectory {
    pub addr: usize,
    pub header: ComboDirectoryHeader,
    pub entries: Vec<ComboDirectoryEntry>,
}

impl<'a> PspComboDirectory {
    pub fn new(data: &'a [u8], addr: usize) -> Result<Self, String> {
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
                addr,
                header,
                entries: entries.to_vec(),
            });
        }

        Err(format!("PSP combo header not found @ {addr:08x}"))
    }

    pub fn header(&self) -> ComboDirectoryHeader {
        self.header
    }

    pub fn entries(&self) -> Vec<ComboDirectoryEntry> {
        self.entries.clone()
    }
}
