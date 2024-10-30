use clap::Parser;
use romulan::amd::directory::PspBinaryHeader;
use std::{fs, io};
use zerocopy::FromBytes;

/// Parse a PSP binary's header
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Print verbosely
    #[arg(required = false, short, long)]
    verbose: bool,

    /// File to read
    #[arg(index = 1)]
    file: String,
}

fn main() -> io::Result<()> {
    let args = Args::parse();
    let file = args.file;
    let data = fs::read(file.clone()).unwrap();
    // let verbose = args.verbose;

    let len = data.len();
    if len > 256 {
        if let Some(h) = PspBinaryHeader::read_from_prefix(&data[..]) {
            println!("{file:50} {h}");
        } else {
            println!("{file:50} cannot parse header");
        }
    } else {
        println!("{file:50} too small to have a header ({len})");
    }

    Ok(())
}
