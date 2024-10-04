// SPDX-License-Identifier: MIT

use clap::Parser;
use romulan::amd;
use romulan::intel;
use romulan::intel::{section, volume};
use romulan::intel::{BiosFile, BiosSection, BiosSections, BiosVolume, BiosVolumes};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::{env, fs, io, mem, process, thread};
use uefi::guid::SECTION_LZMA_COMPRESS_GUID;

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
        let len = data.len() / 1024;
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
    let len = data.len() / 1024;
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
    let len = data.len() / 1024;
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
    let len = data.len() / 1024;
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
    let len = volume.data().len() / 1024;
    let attributes = header.attributes();
    println!("{padding}{guid}: {header_len}, {len} K");
    println!("{padding}  Attrib: {attributes:?}");

    let polarity = attributes.contains(volume::Attributes::ERASE_POLARITY);
    for file in volume.files() {
        dump_file(&file, polarity, &format!("{padding}    "));
    }
}

fn intel_analyze(data: &Vec<u8>) -> Result<(), String> {
    let rom = intel::Rom::new(&data)?;
    if rom.high_assurance_platform()? {
        println!("  HAP: set");
    } else {
        println!("  HAP: not set");
    }

    if let Some(bios) = rom.bios()? {
        let len = bios.data().len() / 1024;
        println!("  BIOS: {len} K");
        for volume in bios.volumes() {
            dump_volume(&volume, "    ");
        }
    } else {
        println!("  BIOS: None");
    }

    if let Some(me) = rom.me()? {
        let len = me.data().len() / 1024;
        println!("  ME: {len} K");
        let v = me.version().unwrap_or("Unknown".to_string());
        println!("    Version: {v}");
    } else {
        println!("  ME: None");
    }
    Ok(())
}

fn amd_analyze(data: &Vec<u8>, do_print: bool, print_json: bool) -> Result<(), String> {
    let rom = amd::Rom::new(&data)?;
    if do_print {
        println!("{:#?}", rom.efs())
    }
    if print_json {
        // TODO: Wrap in EFS: {} or something
        let j = serde_json::to_string_pretty(&rom.efs()).unwrap();
        println!("{j}");
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let args = Args::parse();
    let file = args.file1;
    println!("Scanning {file}");
    let data = fs::read(file).unwrap();
    let do_print = args.print || args.verbose;
    let print_json = args.json;

    if let Some(file2) = args.file2 {
        let data2 = fs::read(file2).unwrap();
        match amd_analyze(&data2, do_print, print_json) {
            Ok(_) => {}
            Err(_) => {}
        }
    } else {
        match intel_analyze(&data) {
            Ok(_) => println!("Intel inside"),
            Err(e) => println!("No Intel inside: {e}"),
        }
        match amd_analyze(&data, do_print, print_json) {
            Ok(_) => println!("AMD inside"),
            Err(e) => println!("No AMD inside: {e}"),
        }
    }

    Ok(())
}
