// SPDX-License-Identifier: MIT

use alloc::string::String;
use core::mem;
use serde::{Deserialize, Serialize};
use zerocopy::AsBytes;
use zerocopy::LayoutVerified;

pub mod directory;
pub mod flash;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Rom<'a> {
    data: &'a [u8],
    efs: flash::EFS,
}

const MAGIC: u32 = 0x55aa55aa;
const STEP_SIZE: usize = 0x1000;

const MAPPING_MASK: usize = 0x00ff_ffff;

fn get_dir(addr: usize, data: &[u8]) -> Result<directory::Directory, String> {
    // TODO: Better deal with the mapping cleanly.
    let base = addr & MAPPING_MASK;
    if base == 0 || base == MAPPING_MASK {
        return Err(format!("0x{base:08x}: empty"));
    }
    match directory::Directory::new(&data[base..]) {
        Ok(d) => Ok(d),
        Err(e) => Err(format!("0x{base:08x}: {e}")),
    }
}

impl<'a> Rom<'a> {
    pub fn new(data: &'a [u8]) -> Result<Rom, String> {
        let mut i = 0;
        // TODO: Can we just iterate over chunks? The last one may be too short.
        /*
        for block in data.chunks(0x1000) {
        }
        */
        // TODO: Handle errors?
        while i + mem::size_of::<flash::EFS>() <= data.len() {
            let first4 = &data[i..i + 4];
            if first4.eq(MAGIC.as_bytes()) {
                let lv: LayoutVerified<_, flash::EFS> =
                    LayoutVerified::new_unaligned_from_prefix(&data[i..])
                        .unwrap()
                        .0;
                return Ok(Rom {
                    data: &data,
                    efs: *lv,
                    // .map_err(|err| format!("EFS invalid: {:?}", err))?,
                });
            }

            i += STEP_SIZE;
        }

        Err(format!("Embedded Firmware Structure not found"))
    }

    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    pub fn efs(&self) -> flash::EFS {
        self.efs
    }

    pub fn psp_legacy(&self) -> Result<directory::Directory, String> {
        get_dir(self.efs.psp_legacy as usize, &self.data)
    }

    pub fn psp(&self) -> Result<directory::Directory, String> {
        get_dir(self.efs.psp as usize, &self.data)
    }
}
