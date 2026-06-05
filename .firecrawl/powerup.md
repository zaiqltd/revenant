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

[Print this book](https://gbdev.io/pandocs/print.html "Print this book")[Git repository](https://github.com/gbdev/pandocs "Git repository")[Suggest an edit](https://github.com/gbdev/pandocs/edit/master/src/Power_Up_Sequence.md "Suggest an edit")

When the Game Boy is powered up, the CPU actually does not start executing instructions at $0100, but actually at $0000.
A program called the _boot ROM_, burned inside the CPU, is mapped “over” the cartridge ROM at first.
This program is responsible for the boot-up animation played before control is handed over to the cartridge’s ROM.
Since the boot ROM hands off control to the game ROM at address $0100, and developers typically need not care about the boot ROM, the “start address” is usually documented as $0100 and not $0000.

9 different known official boot ROMs are known to exist:

| Name | Size (bytes) | Notes |
| --- | --- | --- |
| DMG0 | 256 | Blinks on failed checks, no ® |
| DMG | 256 |  |
| MGB | 256 | One-byte difference to DMG |
| SGB | 256 | Only forwards logo to SGB BIOS, performs no checks |
| SGB2 | 256 | Same difference to SGB than between MGB and DMG |
| CGB0 | 256 + 1792 | Does not init [wave RAM](https://gbdev.io/pandocs/Audio_Registers.html#ff30ff3f--wave-pattern-ram) |
| CGB | 256 + 1792 | Split in two parts, with the cartridge header in the middle |
| AGB0 | 256 + 1792 | Increments B register for GBA identification |
| AGB | 256 + 1792 | Fixes [“logo TOCTTOU”](https://gbdev.io/pandocs/Power_Up_Sequence.html#bypass) |

[A disassembly of all of them is available online.](https://codeberg.org/ISSOtm/gb-bootroms)

The monochrome boot ROMs read [the logo from the header](https://gbdev.io/pandocs/The_Cartridge_Header.html#0104-0133--nintendo-logo), unpack it into VRAM, and then start slowly scrolling it down.
Since reads from an absent cartridge usually return $FF, this explains why powering the console on without a cartridge scrolls a black box.
Additionally, faulty or dirty connections can cause the data read to be corrupted, resulting in a jumbled-up logo.

_Once the logo has finished scrolling_, the boot ROM plays the famous “ba-ding!” sound, and reads the logo **again**, this time comparing it to a copy it stores.
Then, it also computes the header checksum, and compares it to [the checksum stored in the header](https://gbdev.io/pandocs/The_Cartridge_Header.html#014d--header-checksum).
If either of these checks fail, the boot ROM **locks up**, and control is never passed to the cartridge ROM.

Finally, the boot ROM writes to the `BANK` register at $FF50, which unmaps the boot ROM.
The `ldh [$FF50], a` instruction being located at $00FE (and being two bytes long), [the first instruction executed from the cartridge ROM is at $0100](https://gbdev.io/pandocs/The_Cartridge_Header.html#0100-0103--entry-point).

Since the A register is used to write to $FF50, its value is passed to the cartridge ROM; the only difference between the DMG and MGB boot ROMs is that the former writes $01, and the latter uses $FF.

The DMG0 is a rare “early bird” variant of the DMG boot ROM present in few early DMGs.
The behavior of the boot ROM is globally the same, but significant portions of the code have been rearranged.

Interestingly, the DMG0 boot ROM performs both the logo and checksum checks before displaying anything.
If either verification fails, the screen is made to blink while the boot ROM locks up, alternating between solid white and solid black.

The DMG0 boot ROM also lacks the ® symbol next to the Nintendo logo.

These boot ROMs are fairly unique in that they do _not_ perform header checks.
Instead, they set up the Nintendo logo in VRAM from the header just like the monochrome boot ROMs, but then they send the entire header to the SGB BIOS via [the standard packet-transferring procedure](https://gbdev.io/pandocs/SGB_Command_Packet.html#command-packet-transfers), using packet header bytes $F1, $F3, $F5, $F7, $F9, and $FB, in that order.
(These packet IDs are otherwise invalid and never used in regular SGB operation, though it seems that not all SGB BIOS revisions filter them out.)

The boot ROM then unmaps itself and hands off execution to the cartridge ROM without performing any checks.
The SGB BIOS, the program running on the SNES, actually verifies the Nintendo logo and header checksum itself.
If either verification fails, the BIOS itself locks up, repeatedly resetting the SGB CPU within the cartridge.

As the DMG and MGB boot ROMs, the SGB and SGB2 boot ROMs write $01 and $FF respectively to $FF50, and this is also the only difference between these two boot ROMs.

The way the packet-sending routine works makes transferring a set bit _one cycle_ faster than transferring a reset bit; this means that the time taken by the SGB boot ROMs _depends on the cartridge’s header_.
The relationship between the header and the time taken is made more complex by the fact that the boot ROM waits for 4 VBlanks after transferring each packet, mostly but not entirely grouping the timings.

The color boot ROMs are much more complicated, notably because of the compatibility behavior.

The boot ROM is larger, as indicated in the table at the top: 2048 bytes total.
It still has to be mapped starting at $0000, since this is where the CPU starts, but it must also access the cartridge header at $0100-014F.
Thus, the boot ROM is actually split in two parts, a $0000-00FF one, and a $0200-08FF one.

First, the boot ROMs unpack the Nintendo logo to VRAM like the monochrome models, likely for compatibility, and copies the logo to a buffer in HRAM at the same time.
(It is speculated that HRAM was used due to it being embedded within the CPU, unlike WRAM, so that it couldn’t be tampered with.)

Then, the logo is read and decompressed _again_, but with no resizing, yielding the much smaller logo placed below the big “GAME BOY” one.
The boot ROM then sets up compatibility palettes, as described further below, and plays the logo animation with the “ba-ding!” sound.

During the logo animation, and if bit 7 of [the CGB compatibility byte](https://gbdev.io/pandocs/The_Cartridge_Header.html#0143--cgb-flag) is reset (indicating a monochrome-only game), the user is allowed to pick a palette to override the one chosen for compatibility.
Each new choice prevents the animation from ending for 30 frames, potentially delaying the checks and fade-out.

Then, like the monochrome boot ROMs, the header logo is checked _from the buffer in HRAM_, and the header checksum is verified.
For unknown reasons, however, only the first half of the logo is checked, despite the full logo being present in the HRAM buffer.

Finally, the boot ROM fades all BG palettes to white, and sets the hardware to compatibility mode.
If [the CGB compatibility byte](https://gbdev.io/pandocs/The_Cartridge_Header.html#0143--cgb-flag) indicates CGB compatibility, the byte is written directly to [`KEY0`](https://gbdev.io/pandocs/CGB_Registers.html#ff4c--key0sys-cgb-mode-only-cpu-mode-select), potentially [enabling “PGB mode”](https://gbdev.io/pandocs/CGB_Registers.html#pgb-mode);
otherwise, $04 is written to [`KEY0`](https://gbdev.io/pandocs/CGB_Registers.html#ff4c--key0sys-cgb-mode-only-cpu-mode-select) (enabling DMG compatibility mode in the CPU),
$01 is written to [`OPRI`](https://gbdev.io/pandocs/CGB_Registers.html#ff6c--opri-cgb-mode-only-object-priority-mode) (enabling [DMG OBJ priority](https://gbdev.io/pandocs/OAM.html#object-priority-and-conflicts)), and the [compatibility palettes](https://gbdev.io/pandocs/Power_Up_Sequence.html#compatibility-palettes) are written.
Additionally, the DMG logo tilemap is written [if the compatibility requests it](https://gbdev.io/pandocs/Power_Up_Sequence.html#compatibility-palettes).

Like all other boot ROMs, the last thing the color boot ROMs do is hand off execution at the same time as they unmap themselves, though they write $11 instead of $01 or $FF.

Like the DMG0 boot ROM, some early CGBs contain a different boot ROM.
Unlike DMG0 and DMG, the differences between the CGB0 and CGB boot ROM are very minor, with no change in the layout of the ROM.

The most notable change is that the CGB0 boot ROM does _not_ init [wave RAM](https://gbdev.io/pandocs/Audio_Registers.html#ff30ff3f--wave-pattern-ram).
This is known to cause, for example, a different title screen music in the game _R-Type_.

The CGB0 boot ROM also writes copies of other variables to some locations in WRAM that are not otherwise read anywhere.
It is speculated that this may be debug remnants.

The boot ROM is responsible for the automatic colorization of monochrome-only games when run on a GBC.

When in DMG compatibility mode, the [CGB palettes](https://gbdev.io/pandocs/Palettes.html#lcd-color-palettes-cgb-only) are still being used: the background uses BG palette 0 (likely because the entire [attribute map](https://gbdev.io/pandocs/Tile_Maps.html#bg-map-attributes-cgb-mode-only) is set to all zeros), and objects use OBJ palette 0 or 1 depending on bit 4 of [their attribute](https://gbdev.io/pandocs/OAM.html#byte-3--attributesflags).
[`BGP`, `OBP0`, and `OBP1`](https://gbdev.io/pandocs/Palettes.html#lcd-monochrome-palettes) actually index into the CGB palettes instead of the DMG’s shades of grey.

The boot ROM picks a compatibility palette using an ID computed using the following algorithm:

1. Check if the [old licensee code](https://gbdev.io/pandocs/The_Cartridge_Header.html#014b--old-licensee-code) is $33.


   - If yes, the [new licensee code](https://gbdev.io/pandocs/The_Cartridge_Header.html#01440145--new-licensee-code) must be used. Check that it equals the ASCII string `"01"`.
   - If not, check that it equals $01.

In effect, this checks that the licensee in the header is Nintendo.
   - If this check fails, palettes ID $00 is used.
   - Otherwise, the algorithm proceeds.
2. Compute the sum of all 16 [game title](https://gbdev.io/pandocs/The_Cartridge_Header.html#0134-0143--title) bytes, storing this as the “title checksum”.

3. Find the title checksum [in a table](https://codeberg.org/ISSOtm/gb-bootroms/src/commit/443d7f057ae06e8d1d76fa8083650cf0be2cd0ae/src/cgb.asm#L1221-L1230), and record its index within the table.

An almost-complete list of titles corresponding to the different checksums can be found in [Liji’s free CGB boot ROM reimplementation](https://github.com/LIJI32/SameBoy/blob/1d7692cff5552e296be5e1ab075c4f187f57132c/BootROMs/cgb_boot.asm#L230-L328).
   - If not found, palettes ID $00 is used.
   - If the index is 64 or below, the index is used as-is as the palettes ID, and the algorithm ends.
   - Otherwise, it must be further corrected based on the title’s fourth letter; proceed to the step below.
4. The fourth letter is searched for in [another table](https://codeberg.org/ISSOtm/gb-bootroms/src/commit/443d7f057ae06e8d1d76fa8083650cf0be2cd0ae/src/cgb.asm#L1232-L1240).
   - If the letter can’t be found, palettes ID $00 is used.
   - If the letter is found, the index obtained in the previous step is increased by 14 times the row index to get the palettes ID.
     (So, if the letter was found in the first row, the index is unchanged; if it’s found in the second row, it’s increased by 14, and so on.)

The resulting palettes ID is used to pick 3 palettes out of a table via a fairly complex mechanism.
The user can override this choice using certain button combinations during the logo animation; some of these manual choices are identical to auto-colorizations, [but others are unique](https://tcrf.net/Notes:Game_Boy_Color_Bootstrap_ROM#Manual_Select_Palette_Configurations).

Available palettes

A table of checksums (and tie-breaker fourth letters when applicable) and the corresponding palettes can be found [on TCRF](https://tcrf.net/Notes:Game_Boy_Color_Bootstrap_ROM#Assigned_Palette_Configurations).

If the ID is either $43 or $58, then the Nintendo logo’s tilemap is written to VRAM.
This is intended for games that perform some kind of animation with the Nintendo logo; it suddenly appears in the middle of the screen, though, so it may look better for homebrew not to use this mechanism.

Remnants of a functionality designed to allow switching the CGB palettes while the game is running exist in the CGB CPU.

Pokémon Stadium 2’s “GB Tower” emulator contains a very peculiar boot ROM.
It can be found at offset $015995F0 in the US release, and is only 1008 bytes long.
Its purpose is unknown.

This boot ROM does roughly the same setup as a regular CGB boot ROM, but writes to $FF50 very early, and said write is followed by a lock-up loop.
Further, the boot ROM contains a valid header, which is mostly blank save for the logo, compatibility flag (which indicates dual compatibility), and header checksum.

While it may make sense for the boot ROM to at least partially verify the ROM’s integrity via the header check, one may wonder why the logo is checked more stringently.

Caution

The following is advisory, but **is not legal advice**.
If necessary (e.g. commercial releases with logos on the boxes), consult a lawyer.

The logo check was meant to deter piracy using trademark law.
Unlike nowadays, the Game Boy’s technology was not sufficient to require Nintendo’s approval to make a game run on it, and Nintendo decided against hardware protection like the NES’ [lockout chip](https://wiki.nesdev.org/w/index.php/CIC_lockout_chip) likely for cost and/or power consumption reasons.

Instead, the boot ROM’s logo check forces each ROM intended to run on the system to contain an (encoded) copy of the Nintendo logo, which is displayed on startup.
Nintendo’s strategy was to threaten pirate developers with suing for trademark infringement.

Fortunately, [_Sega v. Accolade_](https://en.wikipedia.org/wiki/Sega_v._Accolade) ruled (in the US) that use of a trademarked logo is okay if it is _necessary_ for running programs on the console, so there is no danger for homebrew developers.

That said, if you want to explicitly mark the lack of licensing from Nintendo, you can add some text to the logo screen once the boot ROM hands off control, for example like this:

![Mockup screenshot of an endorsement disambiguation screen](https://gbdev.io/pandocs/imgs/not_licensed.png)

The Nintendo logo check has been [circumvented many times](http://fuji.drillspirits.net/?post=87), be it to avoid legal action from Nintendo or for the swag, and there are basically two ways of doing so.

One is to exploit a [TOCTTOU](https://en.wikipedia.org/wiki/TOCTTOU) vulnerability in the way the console reads the logo (doing so once to draw it, and the other time to check it), which has however been patched on later revisons of the AGB.
This requires custom hardware in the cartridge, however, and is made difficult by the timing and order of the reads varying greatly between boot ROMs.
Some implementations use a custom mapper, others use a capacitor holding some of the address lines to redirect reads to a separate region of ROM containing the modified logo.

The other way is Game Boy Color (and Advance) exclusive: for some reason, the boot ROM copies the full logo into HRAM, but only compares the first half.
Thus, a logo whose top half is correct but not the bottom half will get a pass from the CGB boot ROM.
Strangely, despite correcting the TOCTTOU vulnerability in its later revision, the CGB-AGB boot ROM does _not_ fix this mistake.

Regardless of the console you intend for your game to run on, it is prudent to rely on as little of the following as possible, barring what is mentioned elsewhere in this documentation to detect which system you are running on.
This ensures maximum compatibility, both across consoles and cartridges (especially flashcarts, which typically run their own menu code before your game), increases reliability, and is generally considered good practice.

Use it at your own risk

Some of the information below is highly volatile, due to the complexity of some of the boot ROM behaviors; thus, some of it may contain errors.
Rely on it at your own risk.

The console’s WRAM and HRAM are random on power-up.
[Different models tend to exhibit different patterns](https://web.archive.org/web/20220131221108/https://twitter.com/CasualPkPlayer/status/1409752977812852736), but they are random nonetheless, even depending on factors such as the ambient temperature.
Besides, turning the system off and on again has proven reliable enough [to carry over RAM from one game to another](https://www.youtube.com/watch?v=xayxmTLljr8), so it’s not a good idea to rely on it at all.

Emulation of uninitialized RAM is inconsistent: some emulators fill RAM with a constant on startup (typically $00 or $FF), some emulators fully randomize RAM, and others attempt to reproduce the patterns observed on hardware.
It is a good idea to enable your favorite emulator’s “break on uninitialized RAM read” exception (and if it doesn’t have one, to consider using an emulator that does).

While technically not related to power-on, it is worth noting that external RAM in the cartridge, when present, usually contains random garbage data when first powered on.
It is strongly advised for the game to put a large enough known sequence of bytes at a fixed location in SRAM, and check its presence before accessing any saved data.

| Register | DMG0 | DMG | MGB | SGB | SGB2 |
| --- | --- | --- | --- | --- | --- |
| **A** | $01 | $01 | $FF | $01 | $FF |
| **F** | Z=0 N=0 H=0 C=0 | Z=1 N=0 H=? C=?[1](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-dmg_c) | Z=1 N=0 H=? C=?[1](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-dmg_c) | Z=0 N=0 H=0 C=0 | Z=0 N=0 H=0 C=0 |
| **B** | $FF | $00 | $00 | $00 | $00 |
| **C** | $13 | $13 | $13 | $14 | $14 |
| **D** | $00 | $00 | $00 | $00 | $00 |
| **E** | $C1 | $D8 | $D8 | $00 | $00 |
| **H** | $84 | $01 | $01 | $C0 | $C0 |
| **L** | $03 | $4D | $4D | $60 | $60 |
| **PC** | $0100 | $0100 | $0100 | $0100 | $0100 |
| **SP** | $FFFE | $FFFE | $FFFE | $FFFE | $FFFE |

| Register | CGB (DMG mode) | AGB (DMG mode) | CGB | AGB |
| --- | --- | --- | --- | --- |
| **A** | $11 | $11 | $11 | $11 |
| **F** | Z=1 N=0 H=0 C=0 | Z=? N=0 H=? C=0[2](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-agbdmg_f) | Z=1 N=0 H=0 C=0 | Z=0 N=0 H=0 C=0 |
| **B** | ??[3](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-cgbdmg_b) | ??[3](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-cgbdmg_b) \+ 1 | $00 | $01 |
| **C** | $00 | $00 | $00 | $00 |
| **D** | $00 | $00 | $FF | $FF |
| **E** | $08 | $08 | $56 | $56 |
| **H** | $??[4](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-cgbdmg_hl) | $??[4](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-cgbdmg_hl) | $00 | $00 |
| **L** | $??[4](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-cgbdmg_hl) | $??[4](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-cgbdmg_hl) | $0D | $0D |
| **PC** | $0100 | $0100 | $0100 | $0100 |
| **SP** | $FFFE | $FFFE | $FFFE | $FFFE |

- **The B register is $43 or $58 (on CGB) / $44 or $59 (on AGB)**: HL = $991A
- **Neither of the above**: HL = $007C

The tables above were obtained from analysis of [the boot ROM’s disassemblies](https://codeberg.org/ISSOtm/gb-bootroms), and confirmed using Mooneye-GB tests [`acceptance/boot_regs-dmg0`](https://github.com/Gekkio/mooneye-gb/blob/ca7ff30b52fd3de4f1527397f27a729ffd848dfa/tests/acceptance/boot_regs-dmg0.s), [`acceptance/boot_regs-dmgABC`](https://github.com/Gekkio/mooneye-gb/blob/ca7ff30b52fd3de4f1527397f27a729ffd848dfa/tests/acceptance/boot_regs-dmgABC.s), [`acceptance/boot_regs-mgb`](https://github.com/Gekkio/mooneye-gb/blob/ca7ff30b52fd3de4f1527397f27a729ffd848dfa/tests/acceptance/boot_regs-mgb.s), [`acceptance/boot_regs-sgb`](https://github.com/Gekkio/mooneye-gb/blob/ca7ff30b52fd3de4f1527397f27a729ffd848dfa/tests/acceptance/boot_regs-sgb.s), [`acceptance/boot_regs-sgb2`](https://github.com/Gekkio/mooneye-gb/blob/ca7ff30b52fd3de4f1527397f27a729ffd848dfa/tests/acceptance/boot_regs-sgb2.s), [`misc/boot_regs-cgb`](https://github.com/Gekkio/mooneye-gb/blob/ca7ff30b52fd3de4f1527397f27a729ffd848dfa/tests/misc/boot_regs-cgb.s), and [`misc/boot_regs-A`](https://github.com/Gekkio/mooneye-gb/blob/ca7ff30b52fd3de4f1527397f27a729ffd848dfa/tests/misc/boot_regs-A.s), plus some extra testing.

As far as timing-sensitive values are concerned, these values are recorded at PC = $0100.

| Name | Address | DMG0 | DMG / MGB | SGB / SGB2 | CGB / AGB |
| --- | --- | --- | --- | --- | --- |
| [`P1`](https://gbdev.io/pandocs/Joypad_Input.html#ff00--p1joyp-joypad) | $FF00 | $CF | $CF | $C7 or $CF | $C7 or $CF |
| [`SB`](https://gbdev.io/pandocs/Serial_Data_Transfer_(Link_Cable).html#ff01--sb-serial-transfer-data) | $FF01 | $00 | $00 | $00 | $00 |
| [`SC`](https://gbdev.io/pandocs/Serial_Data_Transfer_(Link_Cable).html#ff02--sc-serial-transfer-control) | $FF02 | $7E | $7E | $7E | $7F |
| [`DIV`](https://gbdev.io/pandocs/Timer_and_Divider_Registers.html#ff04--div-divider-register) | $FF04 | $18 | $AB | ??[5](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-unk) | ??[6](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-unk_pad) |
| [`TIMA`](https://gbdev.io/pandocs/Timer_and_Divider_Registers.html#ff05--tima-timer-counter) | $FF05 | $00 | $00 | $00 | $00 |
| [`TMA`](https://gbdev.io/pandocs/Timer_and_Divider_Registers.html#ff06--tma-timer-modulo) | $FF06 | $00 | $00 | $00 | $00 |
| [`TAC`](https://gbdev.io/pandocs/Timer_and_Divider_Registers.html#ff07--tac-timer-control) | $FF07 | $F8 | $F8 | $F8 | $F8 |
| [`IF`](https://gbdev.io/pandocs/Interrupts.html#ff0f--if-interrupt-flag) | $FF0F | $E1 | $E1 | $E1 | $E1 |
| [`NR10`](https://gbdev.io/pandocs/Audio_Registers.html#ff10--nr10-channel-1-sweep) | $FF10 | $80 | $80 | $80 | $80 |
| [`NR11`](https://gbdev.io/pandocs/Audio_Registers.html#ff11--nr11-channel-1-length-timer--duty-cycle) | $FF11 | $BF | $BF | $BF | $BF |
| [`NR12`](https://gbdev.io/pandocs/Audio_Registers.html#ff12--nr12-channel-1-volume--envelope) | $FF12 | $F3 | $F3 | $F3 | $F3 |
| [`NR13`](https://gbdev.io/pandocs/Audio_Registers.html#ff13--nr13-channel-1-period-low-write-only) | $FF13 | $FF | $FF | $FF | $FF |
| [`NR14`](https://gbdev.io/pandocs/Audio_Registers.html#ff14--nr14-channel-1-period-high--control) | $FF14 | $BF | $BF | $BF | $BF |
| [`NR21`](https://gbdev.io/pandocs/Audio_Registers.html#sound-channel-2--pulse) | $FF16 | $3F | $3F | $3F | $3F |
| [`NR22`](https://gbdev.io/pandocs/Audio_Registers.html#sound-channel-2--pulse) | $FF17 | $00 | $00 | $00 | $00 |
| [`NR23`](https://gbdev.io/pandocs/Audio_Registers.html#sound-channel-2--pulse) | $FF18 | $FF | $FF | $FF | $FF |
| [`NR24`](https://gbdev.io/pandocs/Audio_Registers.html#sound-channel-2--pulse) | $FF19 | $BF | $BF | $BF | $BF |
| [`NR30`](https://gbdev.io/pandocs/Audio_Registers.html#ff1a--nr30-channel-3-dac-enable) | $FF1A | $7F | $7F | $7F | $7F |
| [`NR31`](https://gbdev.io/pandocs/Audio_Registers.html#ff1b--nr31-channel-3-length-timer-write-only) | $FF1B | $FF | $FF | $FF | $FF |
| [`NR32`](https://gbdev.io/pandocs/Audio_Registers.html#ff1c--nr32-channel-3-output-level) | $FF1C | $9F | $9F | $9F | $9F |
| [`NR33`](https://gbdev.io/pandocs/Audio_Registers.html#ff1d--nr33-channel-3-period-low-write-only) | $FF1D | $FF | $FF | $FF | $FF |
| [`NR34`](https://gbdev.io/pandocs/Audio_Registers.html#ff1e--nr34-channel-3-period-high--control) | $FF1E | $BF | $BF | $BF | $BF |
| [`NR41`](https://gbdev.io/pandocs/Audio_Registers.html#ff20--nr41-channel-4-length-timer-write-only) | $FF20 | $FF | $FF | $FF | $FF |
| [`NR42`](https://gbdev.io/pandocs/Audio_Registers.html#ff21--nr42-channel-4-volume--envelope) | $FF21 | $00 | $00 | $00 | $00 |
| [`NR43`](https://gbdev.io/pandocs/Audio_Registers.html#ff22--nr43-channel-4-frequency--randomness) | $FF22 | $00 | $00 | $00 | $00 |
| [`NR44`](https://gbdev.io/pandocs/Audio_Registers.html#ff23--nr44-channel-4-control) | $FF23 | $BF | $BF | $BF | $BF |
| [`NR50`](https://gbdev.io/pandocs/Audio_Registers.html#ff24--nr50-master-volume--vin-panning) | $FF24 | $77 | $77 | $77 | $77 |
| [`NR51`](https://gbdev.io/pandocs/Audio_Registers.html#ff25--nr51-sound-panning) | $FF25 | $F3 | $F3 | $F3 | $F3 |
| [`NR52`](https://gbdev.io/pandocs/Audio_Registers.html#ff26--nr52-audio-master-control) | $FF26 | $F1 | $F1 | $F0 | $F1 |
| [`LCDC`](https://gbdev.io/pandocs/LCDC.html#ff40--lcdc-lcd-control) | $FF40 | $91 | $91 | $91 | $91 |
| [`STAT`](https://gbdev.io/pandocs/STAT.html#ff41--stat-lcd-status) | $FF41 | $81 | $85 | ??[5](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-unk) | ??[6](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-unk_pad) |
| [`SCY`](https://gbdev.io/pandocs/Scrolling.html#ff42ff43--scy-scx-background-viewport-y-position-x-position) | $FF42 | $00 | $00 | $00 | $00 |
| [`SCX`](https://gbdev.io/pandocs/Scrolling.html#ff42ff43--scy-scx-background-viewport-y-position-x-position) | $FF43 | $00 | $00 | $00 | $00 |
| [`LY`](https://gbdev.io/pandocs/STAT.html#ff44--ly-lcd-y-coordinate-read-only) | $FF44 | $91 | $00 | ??[5](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-unk) | ??[6](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-unk_pad) |
| [`LYC`](https://gbdev.io/pandocs/STAT.html#ff45--lyc-ly-compare) | $FF45 | $00 | $00 | $00 | $00 |
| [`DMA`](https://gbdev.io/pandocs/OAM_DMA_Transfer.html#ff46--dma-oam-dma-source-address--start) | $FF46 | $FF | $FF | $FF | $00 |
| [`BGP`](https://gbdev.io/pandocs/Palettes.html#ff47--bgp-non-cgb-mode-only-bg-palette-data) | $FF47 | $FC | $FC | $FC | $FC |
| [`OBP0`](https://gbdev.io/pandocs/Palettes.html#ff48ff49--obp0-obp1-non-cgb-mode-only-obj-palette-0-1-data) | $FF48 | ??[7](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-obp) | ??[7](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-obp) | ??[7](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-obp) | ??[7](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-obp) |
| [`OBP1`](https://gbdev.io/pandocs/Palettes.html#ff48ff49--obp0-obp1-non-cgb-mode-only-obj-palette-0-1-data) | $FF49 | ??[7](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-obp) | ??[7](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-obp) | ??[7](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-obp) | ??[7](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-obp) |
| [`WY`](https://gbdev.io/pandocs/Window.html#ff4aff4b--wy-wx-window-y-position-x-position-plus-7) | $FF4A | $00 | $00 | $00 | $00 |
| [`WX`](https://gbdev.io/pandocs/Window.html#ff4aff4b--wy-wx-window-y-position-x-position-plus-7) | $FF4B | $00 | $00 | $00 | $00 |
| [`KEY0`](https://gbdev.io/pandocs/CGB_Registers.html#ff4c--key0sys-cgb-mode-only-cpu-mode-select) | $FF4C | — | — | — | ??[5](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-unk) |
| [`KEY1`](https://gbdev.io/pandocs/CGB_Registers.html#ff4d--key1spd-cgb-mode-only-prepare-speed-switch) | $FF4D | — | — | — | $7E[8](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-cgb_only) |
| [`VBK`](https://gbdev.io/pandocs/CGB_Registers.html#ff4f--vbk-cgb-mode-only-vram-bank) | $FF4F | — | — | — | $FE[8](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-cgb_only) |
| [`BANK`](https://gbdev.io/pandocs/Power_Up_Sequence.html#power-up-sequence) | $FF50 | — | — | — | — |
| [`HDMA1`](https://gbdev.io/pandocs/CGB_Registers.html#ff51ff52--hdma1-hdma2-cgb-mode-only-vram-dma-source-high-low-write-only) | $FF51 | — | — | — | $FF[8](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-cgb_only) |
| [`HDMA2`](https://gbdev.io/pandocs/CGB_Registers.html#ff51ff52--hdma1-hdma2-cgb-mode-only-vram-dma-source-high-low-write-only) | $FF52 | — | — | — | $FF[8](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-cgb_only) |
| [`HDMA3`](https://gbdev.io/pandocs/CGB_Registers.html#ff53ff54--hdma3-hdma4-cgb-mode-only-vram-dma-destination-high-low-write-only) | $FF53 | — | — | — | $FF[8](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-cgb_only) |
| [`HDMA4`](https://gbdev.io/pandocs/CGB_Registers.html#ff53ff54--hdma3-hdma4-cgb-mode-only-vram-dma-destination-high-low-write-only) | $FF54 | — | — | — | $FF[8](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-cgb_only) |
| [`HDMA5`](https://gbdev.io/pandocs/CGB_Registers.html#ff55--hdma5-cgb-mode-only-vram-dma-lengthmodestart) | $FF55 | — | — | — | $FF[8](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-cgb_only) |
| [`RP`](https://gbdev.io/pandocs/CGB_Registers.html#ff56--rp-cgb-mode-only-infrared-communications-port) | $FF56 | — | — | — | $3E[8](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-cgb_only) |
| [`BCPS`](https://gbdev.io/pandocs/Palettes.html#ff68--bcpsbgpi-cgb-mode-only-background-color-palette-specification--background-palette-index) | $FF68 | — | — | — | ??[9](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-compat) |
| [`BCPD`](https://gbdev.io/pandocs/Palettes.html#ff69--bcpdbgpd-cgb-mode-only-background-color-palette-data--background-palette-data) | $FF69 | — | — | — | ??[9](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-compat) |
| [`OCPS`](https://gbdev.io/pandocs/Palettes.html#ff6aff6b--ocpsobpi-ocpdobpd-cgb-mode-only-obj-color-palette-specification--obj-palette-index-obj-color-palette-data--obj-palette-data) | $FF6A | — | — | — | ??[9](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-compat) |
| [`OCPD`](https://gbdev.io/pandocs/Palettes.html#ff6aff6b--ocpsobpi-ocpdobpd-cgb-mode-only-obj-color-palette-specification--obj-palette-index-obj-color-palette-data--obj-palette-data) | $FF6B | — | — | — | ??[9](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-compat) |
| [`SVBK`](https://gbdev.io/pandocs/CGB_Registers.html#ff70--svbkwbk-cgb-mode-only-wram-bank) | $FF70 | — | — | — | $F8[8](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-cgb_only) |
| [`IE`](https://gbdev.io/pandocs/Interrupts.html#ffff--ie-interrupt-enable) | $FFFF | $00 | $00 | $00 | $00 |

The table above was obtained from Mooneye-GB tests [`acceptance/boot_hwio-dmg0`](https://github.com/Gekkio/mooneye-gb/blob/ca7ff30b52fd3de4f1527397f27a729ffd848dfa/tests/acceptance/boot_hwio-dmg0.s), [`acceptance/boot_hwio-dmgABCmgb`](https://github.com/Gekkio/mooneye-gb/blob/ca7ff30b52fd3de4f1527397f27a729ffd848dfa/tests/acceptance/boot_hwio-dmgABCmgb.s), [`acceptance/boot_hwio-S`](https://github.com/Gekkio/mooneye-gb/blob/ca7ff30b52fd3de4f1527397f27a729ffd848dfa/tests/acceptance/boot_hwio-S.s), and [`misc/boot_hwio-C`](https://github.com/Gekkio/mooneye-gb/blob/ca7ff30b52fd3de4f1527397f27a729ffd848dfa/tests/misc/boot_hwio-C.s), plus some extra testing.

* * *

1. If the [header checksum](https://gbdev.io/pandocs/The_Cartridge_Header.html#014d--header-checksum) is $00, then the carry and half-carry flags are clear; otherwise, they are both set. [↩](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-dmg_c-1) [↩2](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-dmg_c-2)

2. To determine the flags, take the B register you would have gotten on CGB[3](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-cgbdmg_b), and `inc` it.
(To be precise: an `inc b` is the last operation to touch the flags.)
The carry and direction flags are always clear, though. [↩](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-agbdmg_f-1) [↩2](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-agbdmg_f-2)

3. If the [old licensee code](https://gbdev.io/pandocs/The_Cartridge_Header.html#014b--old-licensee-code) is $01, or the old licensee code is $33 and the [new licensee code](https://gbdev.io/pandocs/The_Cartridge_Header.html#01440145--new-licensee-code) is `"01"` ($30 $31), then B is the sum of all 16 [title](https://gbdev.io/pandocs/The_Cartridge_Header.html#0134-0143--title) bytes.
Otherwise, B is $00.
As indicated by the “+ 1” in the “AGB (DMG mode)” column, if on AGB, that value is increased by 1[2](https://gbdev.io/pandocs/Power_Up_Sequence.html#footnote-agbdmg_f). [↩](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-cgbdmg_b-1) [↩2](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-cgbdmg_b-2) [↩3](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-cgbdmg_b-3)

4. There are two possible cases: [↩](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-cgbdmg_hl-1) [↩2](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-cgbdmg_hl-2) [↩3](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-cgbdmg_hl-3) [↩4](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-cgbdmg_hl-4)

5. Since this boot ROM’s duration depends on the header’s contents, a general answer can’t be given.
The value should be static for a given header, though. [↩](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-unk-1) [↩2](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-unk-2) [↩3](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-unk-3) [↩4](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-unk-4)

6. Since this boot ROM’s duration depends on the header’s contents (and the player’s inputs in compatibility mode), an answer can’t be given.
Just don’t rely on these. [↩](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-unk_pad-1) [↩2](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-unk_pad-2) [↩3](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-unk_pad-3)

7. These registers are left entirely uninitialized.
Their value tends to be most often $00 or $FF, but the value is especially not reliable if your software runs after e.g. a flashcart or multicart selection menu.
Make sure to always set those before displaying objects for the first time. [↩](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-obp-1) [↩2](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-obp-2) [↩3](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-obp-3) [↩4](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-obp-4) [↩5](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-obp-5) [↩6](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-obp-6) [↩7](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-obp-7) [↩8](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-obp-8)

8. These registers are only available in CGB Mode, and will read $FF in Non-CGB Mode. [↩](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-cgb_only-1) [↩2](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-cgb_only-2) [↩3](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-cgb_only-3) [↩4](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-cgb_only-4) [↩5](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-cgb_only-5) [↩6](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-cgb_only-6) [↩7](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-cgb_only-7) [↩8](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-cgb_only-8) [↩9](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-cgb_only-9)

9. These depend on whether compatibility mode is enabled. [↩](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-compat-1) [↩2](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-compat-2) [↩3](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-compat-3) [↩4](https://gbdev.io/pandocs/Power_Up_Sequence.html#fr-compat-4)