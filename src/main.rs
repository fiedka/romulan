// SPDX-License-Identifier: MIT

use clap::Parser;
use romulan::amd;
use romulan::amd::directory::{Directory, PspDirectory, PspDirectoryEntry};
use romulan::intel;
use romulan::intel::{section, volume};
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
    if let Ok(_) = rom.high_assurance_platform() {
        println!("  HAP: set");
    } else {
        println!("  HAP: not set");
    }

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
    } else {
        println!("  ME: None");
    }
}

fn print_amd(rom: &amd::Rom, print_json: bool) {
    if print_json {
        // TODO: Wrap in EFS: {} or something
        if let Ok(j) = serde_json::to_string_pretty(&rom.efs()) {
            println!("{j}");
        }
    } else {
        println!("{:#?}", rom.efs())
    }
}

const MAPPING_MASK: usize = 0x00ff_ffff;

type PspAndData<'a> = (&'a Vec<Directory>, &'a [u8]);

fn diff_psps(p1: PspAndData, p2: PspAndData) {
    let (psp1, data1) = p1;
    let (psp2, data2) = p2;

    let es1 = psp1[0].get_entries().unwrap();
    let es2 = psp2[0].get_entries().unwrap();
    let ei2 = es2.iter().enumerate();

    for (i, e) in ei2 {
        let ex = es1[i];
        println!("/ {e:?}\n\\ {ex:?}");
        let b1 = MAPPING_MASK & ex.directory as usize;
        let b2 = MAPPING_MASK & e.directory as usize;

        let mut common = Vec::<(PspDirectoryEntry, PspDirectoryEntry)>::new();
        let mut only_1 = Vec::<PspDirectoryEntry>::new();
        let mut only_2 = Vec::<PspDirectoryEntry>::new();

        let d1 = PspDirectory::new(&data1[b1..]).unwrap();
        let d2 = PspDirectory::new(&data2[b2..]).unwrap();

        for de in d1.entries.iter() {
            match d2
                .entries
                .iter()
                .find(|e| e.kind == de.kind && e.sub_program == de.sub_program)
            {
                Some(e) => {
                    common.push((e.clone(), de.clone()));
                }
                None => {
                    only_1.push(de.clone());
                }
            }
        }

        for de in d2.entries.iter() {
            if let None = d1
                .entries
                .iter()
                .find(|e| e.kind == de.kind && e.sub_program == de.sub_program)
            {
                only_2.push(de.clone());
            }
        }

        for (e1, e2) in common.iter() {
            print!("{} vs {}: ", e1.display(), e2.display());
            match e1.data(&data1) {
                Ok(d1) => {
                    if let Ok(d2) = e2.data(&data2) {
                        if d1.eq(&d2) {
                            println!("same");
                        } else {
                            println!("different");
                        }
                    } else {
                        println!("cannot get other entry");
                    }
                }
                Err(e) => {
                    println!("cannot get first entry: {e}");
                }
            }
        }

        println!("\nonly in 1:");
        for e in only_1 {
            println!("- {}", e.display());
        }

        println!("\nonly in 2:");
        for e in only_2 {
            println!("- {}", e.display());
        }
        println!();
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
    } else {
        if a1.is_none() {
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
}

fn diff_efs(rom1: &amd::Rom, rom2: &amd::Rom) {
    let efs1 = rom1.efs();
    let efs2 = rom2.efs();
    // TODO: IMC, GBE, XHCI, Promontory, LP Promontory, SPI flash

    let a1 = get_real_addr(efs1.bios_17_00_0f);
    let a2 = get_real_addr(efs2.bios_17_00_0f);
    let diff = diff_addr(a1, a2);
    println!("Fam 17 Model 00-0f BIOS: {diff}");

    let a1 = get_real_addr(efs1.bios_17_10_1f);
    let a2 = get_real_addr(efs2.bios_17_10_1f);
    let diff = diff_addr(a1, a2);
    println!("Fam 17 Model 00-0f BIOS: {diff}");

    let a1 = get_real_addr(efs1.bios_17_30_3f_19_00_0f);
    let a2 = get_real_addr(efs2.bios_17_30_3f_19_00_0f);
    let diff = diff_addr(a1, a2);
    println!("Fam 17 Model 30-0f + Fam 19 Model 00-0f BIOS: {diff}");

    let a1 = get_real_addr(efs1.bios_17_60);
    let a2 = get_real_addr(efs2.bios_17_60);
    let diff = diff_addr(a1, a2);
    println!("Fam 17 Model 60+ BIOS: {diff}");

    let a1 = get_real_addr(efs1.psp_legacy);
    let a2 = get_real_addr(efs2.psp_legacy);
    let diff = diff_addr(a1, a2);
    println!("PSP legacy: {diff}");

    let a1 = get_real_addr(efs1.psp);
    let a2 = get_real_addr(efs2.psp);
    let diff = diff_addr(a1, a2);
    println!("PSP modern: {diff}");
}

fn diff_amd(rom1: &amd::Rom, rom2: &amd::Rom, verbose: bool) {
    println!();
    diff_efs(rom1, rom2);
    println!();

    if verbose {
        match rom1.psp() {
            Ok(psp1) => match rom2.psp() {
                Ok(psp2) => {
                    let psp1len = psp1.len();
                    let psp2len = psp2.len();
                    println!("{psp1len} vs {psp2len}");
                    if verbose {
                        println!("{psp1:#?}");
                        println!("{psp2:#?}");
                    }
                    let c1 = psp1[0].get_checksum().unwrap();
                    let c2 = psp2[0].get_checksum().unwrap();
                    if c1 != c2 {
                        println!("images differ: checksum {c1:04x} != {c2:04x}");
                        diff_psps((&psp1, rom1.data()), (&psp2, rom2.data()));
                    }
                }
                Err(e) => {
                    println!("PSP2: {e}");
                }
            },
            Err(e) => {
                println!("PSP1: {e}");
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
            print_amd(&rom1, print_json);
            print_amd(&rom2, print_json);
        }
        diff_amd(&rom1, &rom2, verbose);
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
