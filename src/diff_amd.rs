use romulan::amd;
use romulan::amd::directory::{
    ComboDirectoryEntry, Directory, PspBackupDir, PspComboDirectory, PspDirectory,
    PspDirectoryEntry, PspEntryType,
};
use romulan::amd::flash::{get_real_addr, EFS};

const MAPPING_MASK: usize = 0x00ff_ffff;

type PspAndData<'a> = (&'a Directory, &'a [u8]);

pub enum Comparison {
    Diff,
    Same,
}

/* Printing */
fn print_psp_combo_dir(dir: &PspComboDirectory, data: &[u8]) {
    for d in &dir.entries {
        let base = MAPPING_MASK & d.directory as usize;
        println!("dir @ {base:08x} {d}");
        let dir = PspDirectory::new(&data[base..]).unwrap();
        print_psp_dir(&dir.entries, data);
        println!();
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

pub fn print_psp_dirs(psp: &Directory, data: &[u8]) {
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

pub fn print_bios_dir(base: usize, data: &[u8]) {
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

/* Diffing */
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
                        print_psp_dir(dir, rom2.data());
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

pub fn diff_bios(rom1: &amd::Rom, rom2: &amd::Rom, verbose: bool) {
    println!("TODO: diff BIOS directories");
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