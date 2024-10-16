// SPDX-License-Identifier: MIT

use clap::Parser;
use romulan::amd::directory::{
    ComboDirectoryEntry, Directory, PspBackupDir, PspComboDirectory, PspDirectory,
    PspDirectoryEntry, PspEntryType,
};
use romulan::amd::{self, flash::EFS};
use romulan::intel::{self, section, volume};
use romulan::intel::{BiosFile, BiosSection, BiosSections, BiosVolume, BiosVolumes};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::{fs, io, mem, thread};
use uefi::guid::SECTION_LZMA_COMPRESS_GUID;

const K: usize = 1024;

/// Romulan the ROM analysis tool
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Print
    #[arg(required = false, short, long)]
    print: bool,

    /// Print verbosely
    #[arg(required = false, short, long)]
    verbose: bool,

    /// Print as JSON
    #[arg(required = false, short, long)]
    json: bool,

    /// Dump files
    #[arg(required = false, short, long)]
    dump: bool,

    /// File to read
    #[arg(index = 1)]
    file1: String,

    /// File to diff
    #[arg(index = 2)]
    file2: Option<String>,
}

fn dump_lzma(compressed_data: &[u8], padding: &str) {
    // For some reason, xz2 does not work with this data
    let mut child = Command::new("xz")
        .arg("--decompress")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let data = {
        let mut stdout = child.stdout.take().unwrap();
        let read_thread = thread::spawn(move || -> io::Result<Vec<u8>> {
            let mut data = Vec::<u8>::new();
            stdout.read_to_end(&mut data)?;
            Ok(data)
        });

        {
            let mut stdin = child.stdin.take().unwrap();
            let _write_result = stdin.write_all(compressed_data);
        }

        read_thread.join().unwrap().unwrap()
    };

    let status = child.wait().unwrap();
    if status.success() {
        let len = data.len() / K;
        println!("{padding}Decompressed: {len} K");

        for section in BiosSections::new(&data) {
            dump_section(&section, &format!("{padding}    "));
        }
    } else {
        println!("{padding}Error: {status}");
    }
}

fn dump_guid_defined(section_data: &[u8], padding: &str) {
    let header = plain::from_bytes::<section::GuidDefined>(section_data).unwrap();
    let data_offset = header.data_offset;
    let data = &section_data[(data_offset as usize)..];
    let guid = header.guid;
    let len = data.len() / K;
    println!("{padding}  {guid}: {len} K");

    #[allow(clippy::single_match)]
    match guid {
        SECTION_LZMA_COMPRESS_GUID => {
            let compressed_data = &section_data[mem::size_of::<section::GuidDefined>()..];
            dump_lzma(compressed_data, &format!("{padding}    "));
        }
        _ => (),
    }
}

fn dump_section(section: &BiosSection, padding: &str) {
    let header = section.header();
    let kind = header.kind();
    let data = section.data();
    let len = data.len() / K;
    println!("{padding}{kind:?}:  {len} K");

    match kind {
        section::HeaderKind::GuidDefined => {
            dump_guid_defined(data, &format!("{padding}    "));
        }
        section::HeaderKind::VolumeImage => {
            for volume in BiosVolumes::new(data) {
                dump_volume(&volume, &format!("{padding}    "));
            }
        }
        _ => (),
    }
}

fn dump_file(file: &BiosFile, polarity: bool, padding: &str) {
    let header = file.header();
    let guid = header.guid;
    let data = file.data();
    let len = data.len() / K;
    let kind = header.kind();
    let attributes = header.attributes();
    let alignment = header.alignment();
    let state = header.state(polarity);
    println!("{padding}{guid}: {len} K");
    println!("{padding}  Kind: {kind:?}");
    println!("{padding}  Attrib: {attributes:?}");
    println!("{padding}  Align: {alignment}");
    println!("{padding}  State: {state:?}");

    if header.sectioned() {
        for section in file.sections() {
            dump_section(&section, &format!("{padding}    "));
        }
    }
}

fn dump_volume(volume: &BiosVolume, padding: &str) {
    let header = volume.header();
    let guid = header.guid;
    let header_len = header.header_length;
    let len = volume.data().len() / K;
    let attributes = header.attributes();
    println!("{padding}{guid}: {header_len}, {len} K");
    println!("{padding}  Attrib: {attributes:?}");

    let polarity = attributes.contains(volume::Attributes::ERASE_POLARITY);
    for file in volume.files() {
        dump_file(&file, polarity, &format!("{padding}    "));
    }
}

fn print_intel(rom: &intel::Rom, _print_json: bool, verbose: bool) {
    let hap = match rom.high_assurance_platform() {
        Ok(r) => {
            if r {
                "set".to_string()
            } else {
                "not set".to_string()
            }
        }
        Err(e) => format!("unknown: {e}"),
    };
    println!("  HAP: {hap}");

    if let Ok(bios) = rom.bios() {
        let len = bios.data().len() / K;
        println!("  BIOS: {len} K");
        if verbose {
            for volume in bios.volumes() {
                dump_volume(&volume, "    ");
            }
        }
    } else {
        println!("  BIOS: None");
    }

    if let Ok(me) = rom.me() {
        let len = me.data().len() / K;
        println!("  ME: {len} K");
        let v = me.version().unwrap_or("Unknown".to_string());
        println!("    Version: {v}");
        let d = me.data();
        match me_fs_rs::parse(d) {
            Ok(fpt) => {
                println!("{:#08?}", fpt.header);
            }
            Err(e) => println!("ME parser: {e}"),
        }
    } else {
        println!("  ME: None");
    }
}

fn print_bios_dir(base: usize, data: &[u8]) {
    let b = MAPPING_MASK & base;
    match Directory::new(&data[b..]) {
        Ok(Directory::Bios(directory)) => {
            println!();
            println!("{b:08x}: BIOS Directory");
            for entry in directory.entries() {
                println!("{entry}");
                if entry.kind == 0x70 {
                    print_bios_dir(entry.source as usize, data);
                }
            }
        }
        Ok(Directory::BiosCombo(combo)) => {
            println!();
            println!("{b:08x}: BIOS Combo Directory");
            for entry in combo.entries() {
                print_bios_dir(entry.directory as usize, data);
            }
        }
        Ok(Directory::BiosLevel2(directory)) => {
            println!();
            println!("{b:08x}: BIOS Level 2 Directory");
            for entry in directory.entries() {
                println!("{entry}");
            }
        }
        Err(e) => println!("{e}"),
        _ => println!("??"),
    }
}

const BIOS_DIR_NAMES: [&str; 4] = [
    "BIOS directory for family 17 models 00 to 0f",
    "BIOS directory for family 17 models 10 to 1f",
    "BIOS directory for family 17 models 30 to 3f and family 19 models 00 to 0f",
    "BIOS directory for family 17 model 60 and later",
];

fn print_efs(efs: &EFS) {
    println!(": EFS :");
    let gen2 = efs.second_gen;
    let is_gen2 = gen2 & 0x1 == 0;
    println!(":: Second gen? {is_gen2}");
    println!(":: Firmware ::");
    let a = get_real_addr(efs.imc_fw);
    println!(" IMC Firmware                                  {a:08x?}");
    let a = get_real_addr(efs.gbe_fw);
    println!(" Gigabit ethernet firmware                     {a:08x?}");
    let a = get_real_addr(efs.xhci_fw);
    println!(" XHCI firmware                                 {a:08x?}");
    let a = get_real_addr(efs.bios_17_00_0f);
    println!(" Fam 17 Model 00-0f BIOS                       {a:08x?}");
    let a = get_real_addr(efs.bios_17_10_1f);
    println!(" Fam 17 Model 00-0f BIOS                       {a:08x?}");
    let a = get_real_addr(efs.bios_17_30_3f_19_00_0f);
    println!(" Fam 17 Model 30-0f + Fam 19 Model 00-0f BIOS  {a:08x?}");
    let a = get_real_addr(efs.bios_17_60);
    println!(" Fam 17 Model 60+ BIOS                         {a:08x?}");
    let a = get_real_addr(efs.psp_legacy);
    println!(" PSP legacy                                    {a:08x?}");
    let a = get_real_addr(efs.psp_17_00);
    println!(" PSP modern                                    {a:08x?}");
    let a = get_real_addr(efs.promontory);
    println!(" Promontory firmware                           {a:08x?}");
    let a = get_real_addr(efs.lp_promontory);
    println!(" LP Promontory firmware                        {a:08x?}");
    println!();
    println!(":: SPI flash configuration ::");
    println!(" SPI Fam 15 Models 60-6f         {}", efs.spi_cfg_15_60_6f);
    println!(" SPI Fam 17 Models 00-1f         {}", efs.spi_cfg_17_00_1f);
    println!(" SPI Fam 17 Models 30 and later  {}", efs.spi_cfg_17_30);
}

fn print_amd(rom: &amd::Rom, print_json: bool) {
    if print_json {
        // TODO: Wrap in EFS: {} or something
        if let Ok(j) = serde_json::to_string_pretty(&rom.efs()) {
            println!("{j}");
        }
    } else {
        let data = rom.data();
        let efs = rom.efs();

        println!();
        print_efs(&efs);
        println!();
        println!(": Directories :");
        println!();
        println!(":: BIOS ::");
        let dirs = [
            efs.bios_17_00_0f,
            efs.bios_17_10_1f,
            efs.bios_17_30_3f_19_00_0f,
            efs.bios_17_60,
        ];
        for (i, dir) in dirs.iter().enumerate() {
            println!();
            println!("=== {} ===", BIOS_DIR_NAMES[i]);
            if *dir != 0x0000_0000 && *dir != 0xffff_ffff {
                print_bios_dir(*dir as usize, data);
            } else {
                println!();
                println!("no BIOS dir @ {dir:08x}");
            }
        }
        match rom.psp_legacy() {
            Ok(psp) => {
                println!();
                println!("# legacy PSP {psp}");
                print_psp_dirs(&psp, data);
            }
            Err(e) => {
                println!();
                println!("# legacy PSP: {e}");
            }
        }
        match rom.psp_17_00() {
            Ok(psp) => {
                println!();
                println!("# Fam 17 PSP {psp}");
                print_psp_dirs(&psp, data);
            }
            Err(e) => {
                println!();
                println!("# Fam 17 PSP: {e}");
            }
        }
    }
}

fn print_psp_dir(dir: &Vec<PspDirectoryEntry>, data: &[u8]) {
    for e in dir {
        println!("- {e}");
        if e.kind == PspEntryType::PspLevel2Dir as u8 {
            let b = MAPPING_MASK & e.value as usize;
            println!();
            match PspDirectory::new(&data[b..]) {
                Ok(d) => {
                    println!("| {d}");
                    print_psp_dir(&d.entries, data);
                }
                Err(e) => {
                    println!("Cannot parse level 2 directory @ {b:08x}: {e}");
                }
            }
            println!();
        }
        // Level A sample
        // 00299000: 12d4 7558 ffff ffff 0200 0000 00ff ffff  ..uX............
        // 00299010: 0010 0200 0009 0dbc ffff ffff ffff ffff  ................
        //
        // Level B sample
        // 0029a000: 2784 0dd9 0100 0000 0200 0000 00ff ffff  '...............
        // 0029a010: 00c0 1500 0009 0dbc ffff ffff ffff ffff  ................
        //
        // PSP Level 2 A dir, Level 2 B dir
        if e.kind == PspEntryType::PspLevel2ADir as u8
            || e.kind == PspEntryType::PspLevel2BDir as u8
        {
            let b = MAPPING_MASK & e.value as usize;
            let bd = PspBackupDir::new(&data[b..]).unwrap();
            let a = bd.addr as usize;
            let d = PspDirectory::new(&data[a..]).unwrap();
            println!();
            println!("| {d}");
            print_psp_dir(&d.entries, data);
            println!();
        }
    }
}

pub enum Comparison {
    Diff,
    Same,
}

fn diff_psp_entry(
    e1: &PspDirectoryEntry,
    e2: &PspDirectoryEntry,
    data1: &[u8],
    data2: &[u8],
) -> Result<Comparison, String> {
    let d1 = e1.data(data1).unwrap();
    let d2 = e2.data(data2).unwrap();
    if d1.eq(&d2) {
        Ok(Comparison::Same)
    } else {
        Ok(Comparison::Diff)
    }
}

fn diff_psp_dirs(dir1: &PspDirectory, dir2: &PspDirectory, data1: &[u8], data2: &[u8]) {
    let mut common = Vec::<(PspDirectoryEntry, PspDirectoryEntry)>::new();
    let mut only_1 = Vec::<PspDirectoryEntry>::new();
    let mut only_2 = Vec::<PspDirectoryEntry>::new();

    for de in dir1.entries.iter() {
        match dir2
            .entries
            .iter()
            .find(|e| e.kind == de.kind && e.sub_program == de.sub_program)
        {
            Some(e) => {
                common.push((*de, *e));
            }
            None => {
                only_1.push(*de);
            }
        }
    }

    for de in dir2.entries.iter() {
        if !dir1
            .entries
            .iter()
            .any(|e| e.kind == de.kind && e.sub_program == de.sub_program)
        {
            only_2.push(*de);
        }
    }

    if !common.is_empty() {
        println!("common:");
        for (e1, e2) in common.iter() {
            match diff_psp_entry(e1, e2, data1, data2) {
                Ok(r) => match r {
                    Comparison::Same => println!("= {e1} vs {e2}"),
                    Comparison::Diff => println!("≠ {e1} vs {e2}"),
                },
                Err(e) => println!("⚠️ {e1} vs {e2}: {e}"),
            };
            // TODO: cleaner...
            if e1.kind == 0x40 {
                let b1 = MAPPING_MASK & e1.value as usize;
                let d1 = PspDirectory::new(&data1[b1..]).unwrap();
                let b2 = MAPPING_MASK & e2.value as usize;
                let d2 = PspDirectory::new(&data2[b2..]).unwrap();
                println!("> SUB DIR");
                diff_psp_dirs(&d1, &d2, data1, data2);
                println!("< SUB DIR");
            }
            if e1.kind == PspEntryType::PspLevel2ADir as u8
                || e1.kind == PspEntryType::PspLevel2BDir as u8
            {
                println!();
                let b1 = MAPPING_MASK & e1.value as usize;
                let bd1 = PspBackupDir::new(&data1[b1..]).unwrap();
                let a1 = bd1.addr as usize;
                let d1 = PspDirectory::new(&data1[a1..]).unwrap();
                let b2 = MAPPING_MASK & e2.value as usize;
                let bd2 = PspBackupDir::new(&data2[b2..]).unwrap();
                let a2 = bd2.addr as usize;
                let d2 = PspDirectory::new(&data2[a2..]).unwrap();
                println!("> SUB DIR");
                diff_psp_dirs(&d1, &d2, data1, data2);
                println!("< SUB DIR");
            }
        }
        println!();
    }

    if !only_1.is_empty() {
        println!("entries only in 1:");
        print_psp_dir(&only_1, data1);
        println!();
    }

    if !only_2.is_empty() {
        println!("entries only in 2:");
        print_psp_dir(&only_2, data2);
        println!();
    }
}

const MAPPING_MASK: usize = 0x00ff_ffff;

type PspAndData<'a> = (&'a Directory, &'a [u8]);

fn diff_psps(p1: PspAndData, p2: PspAndData, verbose: bool) {
    let (psp1, data1) = p1;
    let (psp2, data2) = p2;

    if *psp1 != *psp2 {
        println!("PSP 1 and 2 are of different kinds, won't diff");
        print_psp_dirs(psp1, data1);
        print_psp_dirs(psp2, data2);
        return;
    }

    // FIXME: find a better interface?
    match psp1 {
        Directory::PspCombo(_) => {}
        Directory::Psp(d1) => match psp2 {
            Directory::Psp(d2) => {
                diff_psp_dirs(d1, d2, data1, data2);
                return;
            }
            // NOTE: We checked above that psp1 and psp2 are of the same kind.
            _ => unreachable!(),
        },
        _ => unreachable!(),
    }

    let es1 = psp1.get_combo_entries().unwrap();
    let es2 = psp2.get_combo_entries().unwrap();

    let l1 = es1.len();
    let l2 = es2.len();

    let c1 = psp1.get_checksum();
    let c2 = psp2.get_checksum();

    let cs = if c1 != c2 {
        format!("differ: {c1:04x} {c2:04x}")
    } else {
        "equal".to_string()
    };

    if l1 != l2 {
        println!("{psp1} vs {psp2}: different number of entries: {l1} vs {l2}");
        println!();
    } else {
        println!("Comparing {psp1} vs {psp2}, {l1} entries each, checksums {cs}");
        println!();
    }

    if false {
        match psp1.get_combo_header() {
            Ok(h) => println!("{h}"),
            Err(e) => println!("{e}"),
        }
        println!();
        match psp2.get_combo_header() {
            Ok(h) => println!("{h}"),
            Err(e) => println!("{e}"),
        }
        println!();
    }

    let mut common = Vec::<(ComboDirectoryEntry, ComboDirectoryEntry)>::new();
    let mut only_1 = Vec::<ComboDirectoryEntry>::new();
    let mut only_2 = Vec::<ComboDirectoryEntry>::new();

    for de in es1.iter() {
        match es2
            .iter()
            .find(|e| e.id_select == de.id_select && e.id == de.id)
        {
            Some(e) => common.push((*de, *e)),
            None => only_1.push(*de),
        }
    }
    for de in es2.iter() {
        if !es1
            .iter()
            .any(|e| e.id_select == de.id_select && e.id == de.id)
        {
            only_2.push(*de);
        }
    }

    for (e1, e2) in common {
        println!("> Combo dir {e1} vs {e2}");
        // TODO: handle error
        if verbose {
            let b1 = MAPPING_MASK & e1.directory as usize;
            let d1 = PspDirectory::new(&data1[b1..]).unwrap();

            let b2 = MAPPING_MASK & e2.directory as usize;
            let d2 = PspDirectory::new(&data2[b2..]).unwrap();

            diff_psp_dirs(&d1, &d2, data1, data2);
        }
    }

    if !only_1.is_empty() {
        println!("> Combo dir entries only in 1:");
        for e in only_1 {
            println!("> Combo dir {e}");
            let b = MAPPING_MASK & e.directory as usize;
            let d = PspDirectory::new(&data1[b..]).unwrap();
            print_psp_dir(&d.entries, data1);
        }
    }
    if !only_2.is_empty() {
        println!("> Combo dir entries only in 2:");
        for e in only_2 {
            println!("> Combo dir {e}");
            let b = MAPPING_MASK & e.directory as usize;
            let d = PspDirectory::new(&data2[b..]).unwrap();
            print_psp_dir(&d.entries, data1);
        }
    }
}

fn get_real_addr(addr: u32) -> Option<u32> {
    if addr == 0x0000_0000 || addr == 0xffff_ffff {
        None
    } else {
        Some(addr)
    }
}

fn diff_addr(a1: Option<u32>, a2: Option<u32>) -> String {
    if a1.is_none() && a2.is_none() {
        "both empty".to_string()
    } else if a1.is_none() {
        let a = a2.unwrap();
        format!("first is empty, other is {a:08x}")
    } else if a2.is_none() {
        let a = a1.unwrap();
        format!("first is {a:08x}, other is empty")
    } else {
        let a1 = a1.unwrap();
        let a2 = a2.unwrap();
        if a1 != a2 {
            format!("first is {a1:08x}, other is {a2:08x}")
        } else {
            format!("both equal {a1:08x}")
        }
    }
}

// TODO: SPI flash configuration
fn diff_efs(rom1: &amd::Rom, rom2: &amd::Rom) {
    let efs1 = rom1.efs();
    let efs2 = rom2.efs();

    let a1 = get_real_addr(efs1.imc_fw);
    let a2 = get_real_addr(efs2.imc_fw);
    let diff = diff_addr(a1, a2);
    println!("IMC Firmware                                  {diff}");

    let a1 = get_real_addr(efs1.gbe_fw);
    let a2 = get_real_addr(efs2.gbe_fw);
    let diff = diff_addr(a1, a2);
    println!("Gigabit ethernet firmware                     {diff}");

    let a1 = get_real_addr(efs1.xhci_fw);
    let a2 = get_real_addr(efs2.xhci_fw);
    let diff = diff_addr(a1, a2);
    println!("XHCI firmware                                 {diff}");

    let a1 = get_real_addr(efs1.bios_17_00_0f);
    let a2 = get_real_addr(efs2.bios_17_00_0f);
    let diff = diff_addr(a1, a2);
    println!("Fam 17 Model 00-0f BIOS                       {diff}");

    let a1 = get_real_addr(efs1.bios_17_10_1f);
    let a2 = get_real_addr(efs2.bios_17_10_1f);
    let diff = diff_addr(a1, a2);
    println!("Fam 17 Model 00-0f BIOS                       {diff}");

    let a1 = get_real_addr(efs1.bios_17_30_3f_19_00_0f);
    let a2 = get_real_addr(efs2.bios_17_30_3f_19_00_0f);
    let diff = diff_addr(a1, a2);
    println!("Fam 17 Model 30-0f + Fam 19 Model 00-0f BIOS  {diff}");

    let a1 = get_real_addr(efs1.bios_17_60);
    let a2 = get_real_addr(efs2.bios_17_60);
    let diff = diff_addr(a1, a2);
    println!("Fam 17 Model 60+ BIOS                         {diff}");

    let a1 = get_real_addr(efs1.psp_legacy);
    let a2 = get_real_addr(efs2.psp_legacy);
    let diff = diff_addr(a1, a2);
    println!("PSP legacy                                    {diff}");

    let a1 = get_real_addr(efs1.psp_17_00);
    let a2 = get_real_addr(efs2.psp_17_00);
    let diff = diff_addr(a1, a2);
    println!("PSP modern                                    {diff}");

    let a1 = get_real_addr(efs1.promontory);
    let a2 = get_real_addr(efs2.promontory);
    let diff = diff_addr(a1, a2);
    println!("Promontory firmware                           {diff}");

    let a1 = get_real_addr(efs1.lp_promontory);
    let a2 = get_real_addr(efs2.lp_promontory);
    let diff = diff_addr(a1, a2);
    println!("LP Promontory firmware                        {diff}");
}

fn print_psp_combo_dir(dir: &PspComboDirectory, data: &[u8]) {
    for d in &dir.entries {
        let base = MAPPING_MASK & d.directory as usize;
        println!("dir @ {base:08x} {d}");
        let dir = PspDirectory::new(&data[base..]).unwrap();
        print_psp_dir(&dir.entries, data);
        println!();
    }
}

fn print_psp_dirs(psp: &Directory, data: &[u8]) {
    match psp {
        Directory::PspCombo(d) => {
            print_psp_combo_dir(d, data);
        }
        Directory::Psp(d) => {
            println!("{d}");
            print_psp_dir(&d.entries, data);
        }
        _ => println!("Should not happen: not a PSP directory!"),
    }
}

fn diff_psp(rom1: &amd::Rom, rom2: &amd::Rom, verbose: bool) {
    match rom1.psp_legacy() {
        Ok(psp1) => match rom2.psp_legacy() {
            Ok(psp2) => {
                diff_psps((&psp1, rom1.data()), (&psp2, rom2.data()), verbose);
            }
            Err(e) => {
                // FIXME: find a better interface
                match psp1.get_psp_entries() {
                    Ok(dir) => {
                        println!("# legacy PSP 1:");
                        print_psp_dir(dir, rom1.data());
                    }
                    Err(e) => println!("# legacy PSP 1: {e}"),
                }
                println!("# legacy PSP 2: {e}");
            }
        },
        Err(e) => {
            println!("# legacy PSP 1: {e}");
            match rom2.psp_legacy() {
                Ok(psp2) => match psp2.get_psp_entries() {
                    Ok(dir) => {
                        println!("# legacy PSP 2:");
                        print_psp_dir(&dir, rom2.data());
                    }
                    Err(e) => println!("# legacy PSP 2: {e}"),
                },
                Err(e) => println!("# legacy PSP 2: {e}"),
            }
        }
    }
    println!();

    // modern PSP
    match rom1.psp_17_00() {
        Ok(psp1) => match rom2.psp_17_00() {
            Ok(psp2) => {
                diff_psps((&psp1, rom1.data()), (&psp2, rom2.data()), verbose);
            }
            Err(e) => {
                println!("# PSP 1:");
                print_psp_dirs(&psp1, rom1.data());
                println!("# PSP 2: {e}");
            }
        },
        Err(e) => {
            println!("# PSP 1: {e}");
            match rom2.psp_17_00() {
                Ok(psp2) => {
                    println!("# PSP 2:");
                    print_psp_dirs(&psp2, rom2.data());
                }
                Err(e) => println!("# PSP 2: {e}"),
            }
        }
    }
}

fn main() -> io::Result<()> {
    let args = Args::parse();
    let file1 = args.file1;
    let data1 = fs::read(file1.clone()).unwrap();
    let verbose = args.verbose;
    let do_print = args.print || verbose;
    let print_json = args.json;

    if let Some(file2) = args.file2 {
        println!("Diffing {file1} vs {file2}");
        let data2 = fs::read(file2).unwrap();
        let rom1 = amd::Rom::new(&data1).unwrap();
        let rom2 = amd::Rom::new(&data2).unwrap();
        if verbose {
            println!(": Image 1 :");
            print_amd(&rom1, print_json);
            println!(": Image 2 :");
            print_amd(&rom2, print_json);
        }
        println!();
        diff_efs(&rom1, &rom2);
        println!();
        diff_psp(&rom1, &rom2, verbose);
    } else {
        println!("Scanning {file1}");
        match intel::Rom::new(&data1) {
            Ok(rom) => {
                if do_print {
                    println!("Intel inside");
                    print_intel(&rom, print_json, verbose);
                }
            }
            Err(e) => println!("No Intel inside: {e}"),
        }
        match amd::Rom::new(&data1) {
            Ok(rom) => {
                println!("AMD inside");
                if do_print {
                    print_amd(&rom, print_json);
                }
            }
            Err(e) => println!("No AMD inside: {e}"),
        }
    }

    Ok(())
}
