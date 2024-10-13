# AMD Firmware Image

## Embedded Firmware Structure (EFS)

This is the entry point to everything else, pointing to firmware for
- IMC (...)
- GbE (Gigabit ethernet)
- XHCI (USB ...)
- PSP (Arm; legacy and "modern")
- "BIOS" (x86; multiple, per family/model range)
- Promontory (two, one for low power)

It also contains a "second gen" flag plus SPI flash configuration per processor
family/model range.

Note that given a firmware image, you likely have firmware for multiple
processors/variants in it.

## PSP Firmware

The EFS has two pointers for PSP firmware:
1. legacy
2. "modern" (Fam 17 model 00 and later)

Those pointers may each lead to an immediate directory or a "combo" directory.
Combo directory entries themselves point to directories again.

### Combo Directory Entry

The first field tells whether the next one represents a PSP or SoC variant ID.
TODO: This may be related to BIOS Combo directories; to be figured out...

The PSP/SoC variant tells what processor family etc the combo entry is for.
Known variants in coreboot util/amdfwtool are incomplete.
There is no other public source as of now.

Multiple combo directory entries may refer to the same directory for different
variants of a processor that can run the same PSP code.

### How it works

Which code is being run and how it is selected will need to be determined by
the PSP mask ROM. In the case of immediate (non-combo) high level entries, it
may just take what's there and fail or bail out on error; needs investigation.
