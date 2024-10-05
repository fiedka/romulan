// SPDX-License-Identifier: MIT

use alloc::string::String;
use alloc::vec::Vec;
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

    pub fn psp(&self) -> Result<Vec<directory::Directory>, String> {
        let mut dirs = Vec::<directory::Directory>::new();

        // TODO: Better deal with the mapping cleanly.
        // let b1 = self.efs.psp_legacy as usize & 0x00ff_ffff;
        let b2 = self.efs.psp as usize;

        for b in [b2] {
            if b == 0 {
                continue;
            }
            let s = &self.data[b..];
            match directory::Directory::new(s) {
                Ok(d) => {
                    dirs.push(d);
                }
                Err(e) => {
                    return Err(format!("0x{b:08x}: {e}"));
                }
            }
        }
        Ok(dirs)
    }
}
