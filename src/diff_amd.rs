use core::convert::TryFrom;
use romulan::amd::directory::{
    BiosComboDirectory, BiosDirectory, BiosDirectoryEntry, BiosEntryType, ComboDirectoryEntry,
    Directory, PspBackupDir, PspComboDirectory, PspDirectory, PspDirectoryEntry, PspEntryType,
};
use romulan::amd::flash::{get_real_addr, EFS};
use romulan::amd::{self, directory::MAPPING_MASK};

type PspAndData<'a> = (&'a Directory, &'a [u8]);

pub enum Comparison {
    Diff,
    Same,
}

pub const BIOS_DIR_NAMES: [&str; 4] = [
    "BIOS directory for family 17 models 00 to 0f",
    "BIOS directory for family 17 models 10 to 1f",
    "BIOS directory for family 17 models 30 to 3f and family 19 models 00 to 0f",
    "BIOS directory for family 17 model 60 and later",
];

/* Printing */
fn print_psp_combo_dir(dir: &PspComboDirectory, data: &[u8]) {
    println!("PSP Combo Directory @ {:08x}", dir.addr);
    for d in &dir.entries {
        let base = MAPPING_MASK & d.directory as usize;
        println!("{d}");
        let dir = PspDirectory::new(&data[base..], base).unwrap();
        print_psp_dir(&dir.entries, base, data);
        println!();
    }
}

// Level A sample
// 00299000: 12d4 7558 ffff ffff 0200 0000 00ff ffff  ..uX............
// 00299010: 0010 0200 0009 0dbc ffff ffff ffff ffff  ................
//
// Level B sample
// 0029a000: 2784 0dd9 0100 0000 0200 0000 00ff ffff  '...............
// 0029a010: 00c0 1500 0009 0dbc ffff ffff ffff ffff  ................
// NOTE: addr is the address of the directory, needed for entries relative to
// the directory locaiton.
fn print_psp_dir(dir: &Vec<PspDirectoryEntry>, addr: usize, data: &[u8]) {
    println!("PSP Directory @ {:08x}", addr);
    for e in dir {
        let k = PspEntryType::try_from(e.kind);
        let v = e.display(data, addr);
        println!("- {v}");
        match k {
            Ok(PspEntryType::BiosLevel2Dir) => {
                println!();
                let b = e.addr(addr);
                println!("> BIOS recovery directory @ {b:08x}");
                match Directory::new(&data[b..], b) {
                    Ok(d) => print_bios_dir(&d, data),
                    Err(e) => println!("{e} @ {b:08x}"),
                }
            }
            Ok(PspEntryType::PspLevel2Dir) => {
                let b = MAPPING_MASK & e.value as usize;
                println!();
                match PspDirectory::new(&data[b..], b) {
                    Ok(d) => {
                        println!("| {d}");
                        print_psp_dir(&d.entries, b, data);
                    }
                    Err(e) => {
                        println!("Cannot parse level 2 directory @ {b:08x}: {e}");
                    }
                }
                println!();
            }
            Ok(PspEntryType::PspLevel2ADir | PspEntryType::PspLevel2BDir) => {
                let b = MAPPING_MASK & e.value as usize;
                // TODO: handle error
                let bd = PspBackupDir::new(&data[b..]).unwrap();
                let a = bd.addr as usize;
                let d = PspDirectory::new(&data[a..], a).unwrap();
                println!();
                println!("| {d} @ {:08x}", a);
                print_psp_dir(&d.entries, a, data);
                println!();
            }
            Ok(PspEntryType::SoftFuseChain) => {}
            _ => {}
        }
    }
}

pub fn print_psp_dirs(psp: &Directory, data: &[u8]) {
    match psp {
        Directory::PspCombo(d) => {
            print_psp_combo_dir(d, data);
        }
        Directory::Psp(d) => {
            println!("{d}");
            print_psp_dir(&d.entries, d.addr, data);
        }
        _ => println!("Should not happen: not a PSP directory!"),
    }
}

pub fn print_bios_simple_dir(dir: &Vec<BiosDirectoryEntry>, data: &[u8]) {
    for entry in dir {
        println!("{entry}");
        if entry.kind == BiosEntryType::BiosLevel2Dir as u8 {
            print_bios_dir_from_addr(entry.source as usize, data);
        }
    }
}

fn print_bios_combo_dir(dir: &BiosComboDirectory, data: &[u8]) {
    println!(
        "BIOS Combo Directory @ {:08x} checksum {:08x}, {} entries",
        dir.addr, dir.header.checksum, dir.header.entries
    );
    for entry in dir.entries() {
        println!();
        println!("{entry}");
        print_bios_dir_from_addr(entry.directory as usize, data);
    }
}

fn print_bios_level2_dir(dir: &BiosDirectory) {
    println!("BIOS Level 2 Directory @ {:08x}", dir.addr);
    for entry in dir.entries() {
        println!("{entry}");
    }
}

fn print_bios_dir(dir: &Directory, data: &[u8]) {
    match dir {
        Directory::Bios(d) => print_bios_simple_dir(&d.entries, data),
        Directory::BiosCombo(d) => print_bios_combo_dir(d, data),
        Directory::BiosLevel2(d) => print_bios_level2_dir(d),
        _ => println!("??"),
    }
}

pub fn print_bios_dir_from_addr(base: usize, data: &[u8]) {
    let b = MAPPING_MASK & base;
    match Directory::new(&data[b..], b) {
        Ok(Directory::Bios(d)) => {
            println!("BIOS Directory @ {b:08x}");
            print_bios_simple_dir(&d.entries, data);
        }
        Ok(Directory::BiosCombo(d)) => {
            println!();
            print_bios_combo_dir(&d, data);
        }
        Ok(Directory::BiosLevel2(d)) => {
            println!();
            print_bios_level2_dir(&d);
        }
        Err(e) => println!("{e}"),
        _ => println!("??"),
    }
}

/* Diffing */
fn diff_psp_entry(
    e1: &PspDirectoryEntry,
    e2: &PspDirectoryEntry,
    dir_addr1: usize,
    dir_addr2: usize,
    data1: &[u8],
    data2: &[u8],
    verbose: bool,
) -> Result<Comparison, String> {
    if verbose {
        println!("1: {e1:#08x?}");
        println!("2: {e2:#08x?}");
    }
    match e1.data(data1, dir_addr1) {
        Ok((_h1, d1)) => match e2.data(data2, dir_addr2) {
            Ok((_h2, d2)) => {
                if d1.eq(&d2) {
                    Ok(Comparison::Same)
                } else {
                    Ok(Comparison::Diff)
                }
            }
            Err(e) => Err(format!("2: could not get data for {e2}: {e}")),
        },
        Err(e) => Err(format!("1: could not get data for {e1}: {e}")),
    }
}

fn diff_psp_dirs(
    dir1: &PspDirectory,
    dir2: &PspDirectory,
    data1: &[u8],
    data2: &[u8],
    verbose: bool,
) {
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
            // TODO: addressing mode!?
            let a1 = dir1.addr & MAPPING_MASK;
            let a2 = dir2.addr & MAPPING_MASK;
            let v1 = e1.display(data1, a1);
            let v2 = e2.display(data2, a2);
            match diff_psp_entry(e1, e2, a1, a2, data1, data2, verbose) {
                Ok(r) => match r {
                    Comparison::Same => println!("{v1}  🟰  {v2}"),
                    Comparison::Diff => println!("{v1}  ❌  {v2}"),
                },
                Err(e) => {
                    println!("{v1}  ⚠️  {v2}");
                    println!("   {e}");
                }
            };
            // TODO: cleaner...
            if e1.kind == PspEntryType::PspLevel2Dir as u8 {
                let b1 = MAPPING_MASK & e1.value as usize;
                let d1 = PspDirectory::new(&data1[b1..], b1).unwrap();
                let b2 = MAPPING_MASK & e2.value as usize;
                let d2 = PspDirectory::new(&data2[b2..], b2).unwrap();
                println!("> SUB DIR");
                diff_psp_dirs(&d1, &d2, data1, data2, verbose);
                println!("< SUB DIR");
            }
            if e1.kind == PspEntryType::PspLevel2ADir as u8
                || e1.kind == PspEntryType::PspLevel2BDir as u8
            {
                println!();
                let b1 = MAPPING_MASK & e1.value as usize;
                // FIXME: handle error
                let bd1 = PspBackupDir::new(&data1[b1..]).unwrap();
                let a1 = bd1.addr as usize;
                let d1 = PspDirectory::new(&data1[a1..], a1).unwrap();
                let b2 = MAPPING_MASK & e2.value as usize;
                let bd2 = PspBackupDir::new(&data2[b2..]).unwrap();
                let a2 = bd2.addr as usize;
                let d2 = PspDirectory::new(&data2[a2..], a2).unwrap();
                println!("> SUB DIR");
                diff_psp_dirs(&d1, &d2, data1, data2, verbose);
                println!("< SUB DIR");
            }
            if e1.kind == PspEntryType::BiosLevel2Dir as u8 {
                println!();
                let a1 = e1.addr(dir1.addr);
                let a2 = e2.addr(dir2.addr);
                let bd1 = Directory::new(&data1[a1..], a1);
                let bd2 = Directory::new(&data2[a2..], a2);
                diff_bioses(&bd1, &bd2, data1, data2, verbose);
            }
        }
        println!();
    }

    if !only_1.is_empty() {
        println!("entries only in 1:");
        print_psp_dir(&only_1, dir1.addr, data1);
        println!();
    }

    if !only_2.is_empty() {
        println!("entries only in 2:");
        print_psp_dir(&only_2, dir1.addr, data2);
        println!();
    }
}

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
        Directory::PspCombo(_) => {
            // TODO: check other dir here and factor out diff_psp_combo_dirs
        }
        Directory::Psp(d1) => match psp2 {
            Directory::Psp(d2) => {
                diff_psp_dirs(d1, d2, data1, data2, verbose);
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
        let b1 = MAPPING_MASK & e1.directory as usize;
        let d1 = PspDirectory::new(&data1[b1..], b1).unwrap();

        let b2 = MAPPING_MASK & e2.directory as usize;
        let d2 = PspDirectory::new(&data2[b2..], b2).unwrap();

        diff_psp_dirs(&d1, &d2, data1, data2, verbose);
    }

    if !only_1.is_empty() {
        println!("> Combo dir entries only in 1:");
        for e in only_1 {
            println!("> Combo dir {e}");
            let b = MAPPING_MASK & e.directory as usize;
            let d = PspDirectory::new(&data1[b..], b).unwrap();
            print_psp_dir(&d.entries, b, data1);
        }
    }
    if !only_2.is_empty() {
        println!("> Combo dir entries only in 2:");
        for e in only_2 {
            println!("> Combo dir {e}");
            let b = MAPPING_MASK & e.directory as usize;
            let d = PspDirectory::new(&data2[b..], b).unwrap();
            print_psp_dir(&d.entries, b, data2);
        }
    }
}

pub fn diff_psp(rom1: &amd::Rom, rom2: &amd::Rom, verbose: bool) {
    match rom1.psp_legacy() {
        Ok(psp1) => match rom2.psp_legacy() {
            Ok(psp2) => {
                diff_psps((&psp1, rom1.data()), (&psp2, rom2.data()), verbose);
            }
            Err(e) => {
                // FIXME: find a better interface
                match psp1.get_psp_entries() {
                    Ok(dir) => {
                        let a = rom2.efs().psp_legacy as usize;
                        println!("# legacy PSP 1 @ {a:08x}:");
                        print_psp_dir(dir, a, rom1.data());
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
                        let a = rom2.efs().psp_legacy as usize;
                        println!("# legacy PSP 2 @ {a:08x}:");
                        print_psp_dir(dir, a, rom2.data());
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

fn diff_bios_entry(
    e1: &BiosDirectoryEntry,
    e2: &BiosDirectoryEntry,
    dir_addr1: usize,
    dir_addr2: usize,
    data1: &[u8],
    data2: &[u8],
    verbose: bool,
) -> Result<Comparison, String> {
    if verbose {
        println!("1: {e1:#08x?}");
        println!("2: {e2:#08x?}");
    }
    match e1.data(data1, dir_addr1) {
        Ok(d1) => match e2.data(data2, dir_addr2) {
            Ok(d2) => {
                if d1.eq(&d2) {
                    Ok(Comparison::Same)
                } else {
                    Ok(Comparison::Diff)
                }
            }
            Err(e) => Err(format!("2: {e}")),
        },
        Err(e) => Err(format!("1: {e}")),
    }
}

pub fn diff_bios_simple_dir_entries(
    dir1: &BiosDirectory,
    dir2: &BiosDirectory,
    data1: &[u8],
    data2: &[u8],
    verbose: bool,
) {
    let mut common = Vec::<(BiosDirectoryEntry, BiosDirectoryEntry)>::new();
    let mut only_1 = Vec::<BiosDirectoryEntry>::new();
    let mut only_2 = Vec::<BiosDirectoryEntry>::new();

    let es1 = &dir1.entries;
    let es2 = &dir2.entries;

    // TODO: is kind + sub program + flags correct for uniqueness?
    for de in es1.iter() {
        match es2
            .iter()
            .find(|e| e.kind == de.kind && e.sub_program == de.sub_program && e.flags == de.flags)
        {
            Some(e) => common.push((*de, *e)),
            None => only_1.push(*de),
        }
    }
    for de in es2.iter() {
        if !es1
            .iter()
            .any(|e| e.kind == de.kind && e.sub_program == de.sub_program && e.flags == de.flags)
        {
            only_2.push(*de);
        }
    }

    if !common.is_empty() {
        println!("common:");
        for (e1, e2) in common.iter() {
            if e1.kind == BiosEntryType::BiosLevel2Dir as u8 {
                let b1 = MAPPING_MASK & e1.source as usize;
                let b2 = MAPPING_MASK & e2.source as usize;
                let d1 = Directory::new(&data1[b1..], b1);
                let d2 = Directory::new(&data2[b2..], b2);
                println!("diffing level 2 directories:");
                diff_bioses(&d1, &d2, data1, data2, verbose);
            } else {
                match diff_bios_entry(e1, e2, dir1.addr, dir2.addr, data1, data2, verbose) {
                    Ok(r) => match r {
                        Comparison::Same => println!("{e1}  🟰  {e2}"),
                        Comparison::Diff => println!("{e1}  ❌  {e2}"),
                    },
                    Err(e) => {
                        println!("{e1}  ⚠️  {e2}");
                        println!("   {e}");
                    }
                }
            }
        }
    }

    if !only_1.is_empty() {
        println!("entries only in 1:");
        print_bios_simple_dir(&only_1, data1);
        println!();
    }

    if !only_2.is_empty() {
        println!("entries only in 2:");
        print_bios_simple_dir(&only_2, data2);
        println!();
    }
}

// TODO: support combo dirs
pub fn diff_bios_simple_dirs(
    dir1: &Directory,
    dir2: &Directory,
    data1: &[u8],
    data2: &[u8],
    verbose: bool,
) {
    match dir1 {
        Directory::Bios(d1) | Directory::BiosLevel2(d1) => match dir2 {
            Directory::Bios(d2) | Directory::BiosLevel2(d2) => {
                let c1 = d1.header.checksum;
                let c2 = d2.header.checksum;
                println!("checksums {c1:08x} {c2:08x}");

                diff_bios_simple_dir_entries(d1, d2, data1, data2, verbose);
            }
            _ => todo!(),
        },
        Directory::BiosCombo(d1) => match dir2 {
            Directory::BiosCombo(d2) => {
                println!("TODO: diff BIOS combo dirs");
                print_bios_dir_from_addr(d1.addr, data1);
                print_bios_dir_from_addr(d2.addr, data2);
            }
            _ => todo!(),
        },

        _ => todo!(),
    }
}

// TODO: align with PSP diffing?
fn diff_bioses(
    b1: &Result<Directory, String>,
    b2: &Result<Directory, String>,
    data1: &[u8],
    data2: &[u8],
    verbose: bool,
) {
    match b1 {
        Ok(bios_dir1) => match b2 {
            Ok(bios_dir2) => {
                diff_bios_simple_dirs(bios_dir1, bios_dir2, data1, data2, verbose);
            }
            Err(e) => {
                println!("BIOS dir 1:");
                print_bios_dir(bios_dir1, data1);
                println!("BIOS dir 2: {e}");
            }
        },
        Err(e) => {
            println!("BIOS dir 1: {e}");
            match b2 {
                Ok(bios_dir2) => {
                    println!("BIOS dir 2:");
                    print_bios_dir(bios_dir2, data2);
                }
                Err(e) => {
                    println!("BIOS dir 2: {e}");
                }
            }
        }
    }
}

pub fn diff_bios(rom1: &amd::Rom, rom2: &amd::Rom, verbose: bool) {
    println!("NOTE: not yet complete, missing combo directory support");
    let data1 = rom1.data();
    let data2 = rom2.data();

    let b1 = rom1.bios_17_00_0f();
    let b2 = rom2.bios_17_00_0f();
    println!();
    println!("diffing {}", BIOS_DIR_NAMES[0]);
    diff_bioses(&b1, &b2, data1, data2, verbose);

    let b1 = rom1.bios_17_10_1f();
    let b2 = rom2.bios_17_10_1f();
    println!();
    println!("diffing {}", BIOS_DIR_NAMES[1]);
    diff_bioses(&b1, &b2, data1, data2, verbose);

    let b1 = rom1.bios_17_30_3f_19_00_0f();
    let b2 = rom2.bios_17_30_3f_19_00_0f();
    println!();
    println!("diffing {}", BIOS_DIR_NAMES[2]);
    diff_bioses(&b1, &b2, data1, data2, verbose);

    let b1 = rom1.bios_17_60();
    let b2 = rom2.bios_17_60();
    println!();
    println!("diffing {}", BIOS_DIR_NAMES[3]);
    diff_bioses(&b1, &b2, data1, data2, verbose);
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
pub fn diff_efs(efs1: &EFS, efs2: &EFS) {
    let gen1 = efs1.gen;
    let gen2 = efs2.gen;
    println!("{gen1} vs {gen2}");

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
