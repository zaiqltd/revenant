## Keyboard shortcuts

Press `←` or `→` to navigate between chapters

Press `S` or `/` to search in the book

Press `?` to show this help

Press `Esc` to hide this help

- Auto
- Light
- Rust
- Coal
- Navy
- Ayu

# Pan Docs

[Print this book](https://gbdev.io/pandocs/print.html "Print this book")[Git repository](https://github.com/gbdev/pandocs "Git repository")[Suggest an edit](https://github.com/gbdev/pandocs/edit/master/src/CGB_Registers.md "Suggest an edit")

This chapter describes only Game Boy Color (GBC or CGB) registers that didn’t
fit into normal categories — most CGB registers are described in the
chapter about Video Display (Color Palettes, VRAM Bank, VRAM DMA
Transfers, and changed meaning of Bit 0 of LCDC Control register). Also,
a changed bit is noted in the chapter about the Serial/Link port.

When using any CGB registers (including those in the Video/Link
chapters), you must first unlock CGB features by changing byte 0143 in
the cartridge header. Typically, use a value of $80 for games which
support both CGB and monochrome Game Boy systems, and $C0 for games which work
on CGBs only. Otherwise, the CGB will operate in monochrome “Non CGB”
compatibility mode.

CGB hardware can be detected by examining the CPU accumulator (A-register)
directly after startup. A value of $11 indicates CGB (or GBA) hardware,
if so, CGB functions can be used (if unlocked, see above). When A=$11,
you may also examine Bit 0 of the CPUs B-Register to separate between
CGB (bit cleared) and GBA (bit set), by that detection it is possible to
use “repaired” color palette data matching for GBA displays.

One of the pitfalls of the `HDMAx` naming convention is that the register’s purpose is not immediately clear from its name, so some alternative `VDMA_*` names have been proposed, [such as `VDMA_LEN` for `HDMA5`](https://github.com/gbdev/hardware.inc/blob/8d4432e5796bffe2e13c438013285c5f11c37b99/hardware.inc#L919).

These two registers specify the address at which the transfer will read
data from. Normally, this should be either in ROM, SRAM or WRAM, thus
either in range 0000-7FF0 or A000-DFF0. \[Note: this has yet to be\
tested on Echo RAM, OAM, FEXX, IO and HRAM\]. Trying to specify a source
address in VRAM will cause garbage to be copied.

The four lower bits of this address will be ignored and treated as 0.

These two registers specify the address within 8000-9FF0 to which the
data will be copied. Only bits 12-4 are respected; others are ignored.
The four lower bits of this address will be ignored and treated as 0.

These registers are used to initiate a DMA transfer from ROM or RAM to
VRAM. The Source Start Address may be located at 0000-7FF0 or A000-DFF0,
the lower four bits of the address are ignored (treated as zero). The
Destination Start Address may be located at 8000-9FF0, the lower four
bits of the address are ignored (treated as zero), the upper 3 bits are
ignored either (destination is always in VRAM).

Writing to this register starts the transfer, the lower 7 bits of which
specify the Transfer Length (divided by $10, minus 1), that is, lengths of
$10-$800 bytes can be defined by the values $00-$7F. The upper bit
indicates the Transfer Mode:

When using this transfer method,
all data is transferred at once. The execution of the program is halted
until the transfer has completed. Note that the General Purpose DMA
blindly attempts to copy the data, even if the LCD controller is
currently accessing VRAM. So General Purpose DMA should be used only if
the Display is disabled, or during VBlank, or (for rather short blocks)
during HBlank. The execution of the program continues when the transfer
has been completed, and FF55 then contains a value of $FF.

The HBlank DMA transfers $10 bytes of
data during each HBlank, that is, at LY=0-143, no data is transferred during
VBlank (LY=144-153), but the transfer will then continue at LY=00. The
execution of the program is halted during the separate transfers, but
the program execution continues during the “spaces” between each data
block. Note that the program should not change the Destination VRAM bank
(FF4F), or the Source ROM/RAM bank (in case data is transferred from
bankable memory) until the transfer has completed! (The transfer should
be paused as described below while the banks are switched).

Upon halting the CPU (using the [halt instruction](https://gbdev.io/pandocs/Reducing_Power_Consumption.html#using-the-halt-instruction)),
the transfer will also be halted and will be resumed only when the CPU resumes execution ( [test rom](https://github.com/alloncm/MagenTests?tab=readme-ov-file#vram-dma-hblank-mode) exhibiting this behaviour).

Reading from Register FF55 returns the remaining length (divided by $10,
minus 1), a value of $FF indicates that the transfer has completed. It
is also possible to terminate an active HBlank transfer by writing zero
to Bit 7 of FF55. In that case reading from FF55 will return how many
$10 “blocks” remained (minus 1) in the lower 7 bits, but Bit 7 will
be read as “1”. Stopping the transfer doesn’t set HDMA1-4 to $FF.

WARNING

HBlank DMA should not be started (write to FF55) during a HBlank
period (STAT mode 0).

If the transfer’s destination address overflows, the transfer stops
prematurely.
The status of the registers if this happens still needs to be [investigated](https://github.com/gbdev/pandocs/issues/364).

Reading Bit 7 of FF55 can be used to confirm if the DMA transfer is
active (1=Not Active, 0=Active). This works under any circumstances -
after completion of General Purpose, or HBlank Transfer, and after
manually terminating a HBlank Transfer.

In both Normal Speed and Double Speed Mode it takes about 8 μs to
transfer a block of $10 bytes.
That is, 8 M-cycles in Normal Speed Mode [\[1\]](https://gbdev.io/pandocs/imgs/hdma_normal_speed.png),
and 16 “fast” M-cycles in Double Speed Mode [\[2\]](https://gbdev.io/pandocs/imgs/hdma_double_speed.png).
Older MBC controllers (like MBC1-3) and slower ROMs are not guaranteed to support General
Purpose or HBlank DMA, that’s because there are always 2 bytes
transferred per microsecond (even if the itself program runs it Normal
Speed Mode).

This allows for a transfer of 2280 bytes during VBlank, which is up to 142.5 tiles.

The CGB has twice the VRAM of the DMG, but it is banked and either bank
has a different purpose.

This register can be written to change VRAM banks. Only bit 0
matters, all other bits are ignored.

VRAM bank 1 is split like VRAM bank 0 ; 8000-97FF also stores tiles
(just like in bank 0), which can be accessed the same way as (and at the
same time as) bank 0 tiles. 9800-9FFF contains the attributes for the
corresponding Tile Maps.

Reading from this register will return the number of the currently
loaded VRAM bank in bit 0, and all other bits will be set to 1.

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **KEY1** | Current speed |  | Switch armed |

- **Current speed** ( _Read-only_): `0` = Normal-speed mode, `1` = Double-speed mode
- **Switch armed** ( _Read/Write_): `0` = No, `1` = Armed

This register is used to prepare the Game Boy to switch between CGB
Double Speed Mode and Normal Speed Mode. The actual speed switch is
performed by executing a `stop` instruction after Bit 0 has been set. After
that, Bit 0 will be cleared automatically, and the Game Boy will operate
at the “other” speed. The recommended speed switching procedure in
pseudocode would be:

```undefined

IF KEY1_BIT7 != DESIRED_SPEED THEN
   IE = $00       ; (FFFF) = $00
   JOYP = $30     ; (FF00) = $30
   KEY1 = $01     ; (FF4D) = $01
   STOP
ENDIF
```

The CGB is operating in Normal Speed Mode when it is first turned on. Note
that using the Double Speed Mode increases the power consumption; therefore, it
would be recommended to use Normal Speed whenever possible.

In Double Speed Mode the following will operate twice as fast as normal:

- The CPU (2.10 MHz, so 1 M-cycle = approx. 0.5 µs)
- Timer and Divider Registers
- Serial Port (Link Cable)
- DMA Transfer to OAM

And the following will keep operating as usual:

- LCD Video Controller
- HDMA Transfer to VRAM
- All Sound Timings and Frequencies

The CPU stops for 2050 M-cycles (= 8200 T-cycles) after the `stop` instruction is
executed. During this time, the CPU is in a strange state. `DIV` does not tick, so
_some_ audio events are not processed. Additionally, VRAM/OAM/… locking is “frozen”, yielding
different results depending on the [PPU mode](https://gbdev.io/pandocs/Rendering.html#ppu-modes) it’s started in:

- HBlank / VBlank (Mode 0 / Mode 1): The PPU cannot access any video memory, and produces black pixels
- OAM scan (Mode 2): The PPU can access VRAM just fine, but not OAM, leading to rendering background, but not objects (sprites)
- Rendering (Mode 3): The PPU can access everything correctly, and so rendering is not affected

TODO: confirm whether interrupts can occur (just the joypad one?) during the pause, and consequences if so

This register allows to input and output data through the CGBs built-in
Infrared Port. When reading data, bit 6 and 7 must be set (and obviously
Bit 0 must be cleared — if you don’t want to receive your own Game Boy’s
IR signal). After sending or receiving data you should reset the
register to $00 to reduce battery power consumption again.

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **RP** | Read enable |  | Receiving | Emitting |

- **Read enable** ( _Read/Write_): `0` = Disable (bit 1 reads `1`), `3` = Enable
- **Receiving** ( _Read-only_): `0` = Receiving IR signal, `1` = Normal
- **Emitting** ( _Read/Write_): `0` = LED off, `1` = LED on

Note that the receiver will adapt itself to the normal level of IR
pollution in the air, so if you would send a LED ON signal for a longer
period, then the receiver would treat that as normal (=OFF) after a
while. For example, a Philips TV Remote Control sends a series of 32 LED
ON/OFF pulses (length 10us ON, 17.5us OFF each) instead of a permanent
880us LED ON signal. Even though being generally CGB compatible, the GBA
does not include an infra-red port.

This GBC-only register (which is not officially documented) is written only by the CGB boot ROM,
as it gets locked after the bootrom finish execution (by a write to the [BANK register](https://gbdev.io/pandocs/Power_Up_Sequence.html#monochrome-models-dmg0-dmg-mgb)).

Once it is locked, the behavior of the system can’t be changed without a reset (this behavior can be observed using [this test ROM](https://github.com/alloncm/MagenTests?tab=readme-ov-file#key0-cpu-mode-register-lock-after-boot)).

As a result of the above most of the behavior is not directly testable without hardware manipulation.
Even though we can’t test its behavior directly we can inspect the disassembly of the CGB bootrom and infer the following:

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **KEY0** |  | DMG compatibility mode |  |

- **DMG compatibility mode**: `0` = Disabled (full CGB mode, for regular CGB cartridges), `1` = Enabled (for DMG only cartridges)

Research needed

It has been speculated that setting bit 3 is related to a special mode called “PGB” for controlling the LCD externally.

This mode is not well researched nor documented yet, you are welcome to help [here!](https://github.com/gbdev/pandocs/issues/581)

This register serves as a flag for which object priority mode to use. While
the DMG prioritizes objects by x-coordinate, the CGB prioritizes them by
location in OAM. This flag is set by the CGB bios after checking the game’s CGB compatibility.

OPRI has an effect if a [PGB](https://gbdev.io/pandocs/CGB_Registers.html#pgb-mode) value (`0xX8`, `0xXC`) is written to [KEY0](https://gbdev.io/pandocs/CGB_Registers.html#ff4c--key0sys-cgb-mode-only-cpu-mode-select) but STOP hasn’t been executed yet, and the write takes effect instantly.

TO BE VERIFIED

It does not have an effect, at least not an instant effect, if written to during CGB or DMG mode after the boot ROM has been unmapped.
It is not known if triggering a PSM NMI, which remaps the boot ROM, has an effect on this register’s behavior.

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **OPRI** |  | Priority mode |

- **Priority mode** ( _Read/Write_): `0` = CGB-style priority, `1` = DMG-style priority

In CGB Mode, 32 KiB of internal RAM are available.
This memory is divided into 8 banks of 4 KiB each.
Bank 0 is always available in memory at C000–CFFF, banks 1–7 can be selected into the address space at D000–DFFF.

|  | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **SVBK** |  | WRAM bank |

- **WRAM bank** ( _Read/Write_): Writing a value will map the corresponding bank to [D000–DFFF](https://gbdev.io/pandocs/HuC1.html#memory-map), except 0, which maps bank 1 instead.

These are undocumented CGB Registers. The purpose of these registers is
unknown (if any). It isn’t recommended to use them in your software,
but you could, for example, use them to check if you are running on an
emulator or on DMG hardware.

Both of these registers are fully read/write.
Their initial value is $00.

In CGB mode, this register is fully readable and writable.
Its initial value is $00.

Otherwise, this register is read-only, and locked at value $FF.

Only bits 4, 5 and 6 of this register are read/write enabled.
Their initial value is 0.