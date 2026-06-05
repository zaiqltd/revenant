# SM83 CPU

I have all the authoritative details I need. The DAA algorithm, interrupt timing/cancellation, HALT bug, and EI delay are confirmed. I'll now write the spec from established hardware knowledge cross-checked against these sources.

# REVENANT — Sharp SM83 / LR35902 CPU Specification

Reference material for a cycle-accurate DMG/CGB implementation. All timing in **T-cycles** (1 T = 1 clock @ 4.194304 MHz DMG / 8.388608 MHz CGB-double) and **M-cycles** (1 M = 4 T). Every memory access is exactly **1 M-cycle (4 T)**. Internal-only operations also consume whole M-cycles.

---

## 1. Register File

| 16-bit | High (bits 15–8) | Low (bits 7–0) | Notes |
|---|---|---|---|
| `AF` | `A` (accumulator) | `F` (flags) | `F` low 4 bits **hardwired 0** — always read 0, writes to them are dropped |
| `BC` | `B` | `C` | |
| `DE` | `D` | `E` | |
| `HL` | `H` | `L` | primary memory pointer |
| `SP` | — | — | stack pointer, full 16-bit |
| `PC` | — | — | program counter, full 16-bit |

- 8-bit register encoding `r[3]`: `0=B 1=C 2=D 3=E 4=H 5=L 6=(HL) 7=A`. `6` means memory operand at `[HL]` (+1 M-cycle).
- 16-bit pair encoding `rp[2]` (group 1, BC/DE/HL/SP): `0=BC 1=DE 2=HL 3=SP`.
- 16-bit pair encoding `rp2[2]` (group 2, push/pop, AF replaces SP): `0=BC 1=DE 2=HL 3=AF`.
- Condition codes `cc[2]`: `0=NZ 1=Z 2=NC 3=C`.

### 1.1 Flag register `F` bit layout

| Bit | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
|---|---|---|---|---|---|---|---|---|
| Flag | **Z** | **N** | **H** | **C** | 0 | 0 | 0 | 0 |

- **Z** (Zero): set if result == 0.
- **N** (Subtract/BCD-N): set if last op was a subtraction. Only consumed by `DAA`.
- **H** (Half-carry): carry/borrow from bit 3→4 (8-bit) or used by `DAA`.
- **C** (Carry): carry/borrow from bit 7 (8-bit ALU), bit 15 (`ADD HL`), bit 7 (`ADD SP`/`LD HL,SP+e`), or shifted-out bit (CB rotates/shifts).
- Bits 3–0 are physically absent: `PUSH AF`/`POP AF`, `LD A,F`-equivalents never preserve them. Test ROMs (`mooneye` `daa`, `pop_timing`) assert `(F & 0x0F) == 0` always.

### 1.2 Post-boot register state (must match exactly)

| Model | A | F | B | C | D | E | H | L | SP | PC |
|---|---|---|---|---|---|---|---|---|---|---|
| DMG | `0x01` | `0xB0` (Z=1,N=0,H=1,C=1)\* | `0x00` | `0x13` | `0x00` | `0xD8` | `0x01` | `0x4D` | `0xFFFE` | `0x0100` |
| CGB | `0x11` | `0x80` (Z=1) | `0x00` | `0x00` | `0xFF` | `0x56` | `0x00` | `0x0D` | `0xFFFE` | `0x0100` |
| MGB (Pocket) | `0xFF` | `0xB0` | `0x00` | `0x13` | `0x00` | `0xD8` | `0x01` | `0x4D` | `0xFFFE` | `0x0100` |

\* DMG `F` depends on header checksum: if the cartridge header checksum byte (`0x014D`) is `0x00`, the boot ROM leaves **H and C = 0** (`F=0x80`); otherwise `F=0xB0`. CGB/AGB are unaffected. `IME=0`, `IF=0xE1`, `IE=0x00` after boot.

---

## 2. ALU Flag Effects (exhaustive)

Notation: `n` = operand byte, `r` = result byte, `c_in` = incoming carry (0/1). `H` half-carry uses low nibbles; `C` full byte. `↕`=computed, `0`/`1`=forced, `-`=unchanged.

### 2.1 8-bit arithmetic/logic (A := A op n)

| Op | Result | Z | N | H | C |
|---|---|---|---|---|---|
| **ADD A,n** | `A+n` | `r==0` | `0` | `(A&0xF)+(n&0xF) > 0xF` | `A+n > 0xFF` |
| **ADC A,n** | `A+n+c_in` | `r==0` | `0` | `(A&0xF)+(n&0xF)+c_in > 0xF` | `A+n+c_in > 0xFF` |
| **SUB A,n** | `A-n` | `r==0` | `1` | `(A&0xF) < (n&0xF)` | `A < n` |
| **SBC A,n** | `A-n-c_in` | `r==0` | `1` | `(A&0xF) < (n&0xF)+c_in` | `A < n+c_in` |
| **AND A,n** | `A&n` | `r==0` | `0` | `1` | `0` |
| **OR A,n** | `A\|n` | `r==0` | `0` | `0` | `0` |
| **XOR A,n** | `A^n` | `r==0` | `0` | `0` | `0` |
| **CP A,n** | `A-n` (discarded) | `r==0` | `1` | `(A&0xF) < (n&0xF)` | `A < n` |

> `H`/`C` for ADC/SBC must use the **full** sum including carry (compare the entire `(A&0xF)+(n&0xF)+c_in` against `0xF`, not a two-stage add). `mooneye` and Blargg `cpu_instrs` 08 exercise the boundary where `c_in` pushes the nibble exactly to `0x10`.

### 2.2 8-bit INC/DEC (note: **C is preserved**)

| Op | Result | Z | N | H | C |
|---|---|---|---|---|---|
| **INC r** | `r+1` | `r==0` | `0` | `(r&0xF)==0xF` (i.e. `(old&0xF)+1>0xF`) | `-` |
| **DEC r** | `r-1` | `r==0` | `1` | `(old&0xF)==0x0` (borrow from bit 4) | `-` |

### 2.3 16-bit arithmetic

| Op | Result | Z | N | H | C |
|---|---|---|---|---|---|
| **ADD HL,rr** | `HL+rr` | `-` | `0` | `(HL&0x0FFF)+(rr&0x0FFF) > 0x0FFF` (carry bit 11→12) | `HL+rr > 0xFFFF` (carry bit 15) |
| **INC rr** | `rr+1` | `-` | `-` | `-` | `-` (no flags) |
| **DEC rr** | `rr-1` | `-` | `-` | `-` | `-` (no flags) |
| **ADD SP,e8** | `SP+(int8)e8` | `0` | `0` | `(SP&0xF)+(e8&0xF) > 0xF` | `(SP&0xFF)+(e8&0xFF) > 0xFF` |
| **LD HL,SP+e8** | `HL := SP+(int8)e8` | `0` | `0` | `(SP&0xF)+(e8&0xF) > 0xF` | `(SP&0xFF)+(e8&0xFF) > 0xFF` |

> Critical quirk: `ADD SP,e8` and `LD HL,SP+e8` compute **H and C from the low byte only**, as an *unsigned 8-bit* addition of `SP_low + (e8 as unsigned byte)`, even though `e8` is sign-extended for the actual 16-bit add. `Z` and `N` are always **0**. This is asymmetric vs `ADD HL,rr` (which uses bit-11/bit-15). Verified by Blargg/`mooneye add_sp_e_timing`.

### 2.4 Rotates on A (non-CB, the 4 "fast" forms) — **Z always 0**

| Op | Operation | Z | N | H | C |
|---|---|---|---|---|---|
| **RLCA** | A = (A<<1) \| (A>>7) | `0` | `0` | `0` | old bit 7 |
| **RRCA** | A = (A>>1) \| (A<<7) | `0` | `0` | `0` | old bit 0 |
| **RLA** | A = (A<<1) \| c_in | `0` | `0` | `0` | old bit 7 |
| **RRA** | A = (A>>1) \| (c_in<<7) | `0` | `0` | `0` | old bit 0 |

> These differ from CB `RLC/RRC/RL/RR A` **only** in that the non-CB forms force `Z=0`; the CB forms compute `Z` normally. Test ROMs check this divergence.

### 2.5 Misc accumulator ops

| Op | Operation | Z | N | H | C |
|---|---|---|---|---|---|
| **CPL** | A = ~A | `-` | `1` | `1` | `-` |
| **SCF** | set carry | `-` | `0` | `0` | `1` |
| **CCF** | flip carry | `-` | `0` | `0` | `~C` |

---

## 3. DAA — exact algorithm

`DAA` adjusts `A` after a BCD add/subtract, using `N`, `H`, `C` as inputs. **Inputs `N` is decisive for add-vs-subtract direction.**

```
adjust = 0
carry_out = C            ; preserve unless we cross 0x99 on an add
if N == 0:               ; previous op was ADD/ADC/INC
    if H == 1 or (A & 0x0F) > 0x09: adjust |= 0x06
    if C == 1 or  A        > 0x99:  adjust |= 0x60; carry_out = 1
    A = (A + adjust) & 0xFF
else:                    ; N == 1, previous op was SUB/SBC/DEC
    if H == 1:           adjust |= 0x06
    if C == 1:           adjust |= 0x60
    A = (A - adjust) & 0xFF
                         ; (carry_out stays = C; subtract never sets carry)
```

Flag results:

| Z | N | H | C |
|---|---|---|---|
| `A==0` (post-adjust) | `-` (unchanged) | **`0`** (always cleared) | `carry_out` |

> Notes that test ROMs (`mooneye daa`, Blargg 01) hinge on:
> - In the **subtract** branch, the nibble/value comparisons (`>9`, `>0x99`) are **NOT** used — only `H` and `C`. Adding them is a common bug.
> - `H` is **always cleared** to 0 regardless of branch.
> - `C` can only be **set** (never cleared) by `DAA` on the add path; on the subtract path it stays as the input `C`. `DAA` cannot clear an input carry.
> - `N` is left untouched.

---

## 4. CB-Prefixed Ops (`0xCB` + opcode)

Encoding: top 2 bits select group, then `bit/op[3]`, then `r[3]` operand. Operand `6 = [HL]` adds memory R+W cycles.

### 4.1 Rotates / shifts (all set flags identically except C source) — operand `v`, result `r`

| Op | Operation | Z | N | H | C |
|---|---|---|---|---|---|
| **RLC v** | `r=(v<<1)\|(v>>7)` | `r==0` | 0 | 0 | old bit 7 |
| **RRC v** | `r=(v>>1)\|(v<<7)` | `r==0` | 0 | 0 | old bit 0 |
| **RL v** | `r=(v<<1)\|c_in` | `r==0` | 0 | 0 | old bit 7 |
| **RR v** | `r=(v>>1)\|(c_in<<7)` | `r==0` | 0 | 0 | old bit 0 |
| **SLA v** | `r=v<<1` (bit0=0) | `r==0` | 0 | 0 | old bit 7 |
| **SRA v** | `r=(v>>1)\|(v&0x80)` (arith, bit7 kept) | `r==0` | 0 | 0 | old bit 0 |
| **SWAP v** | `r=(v<<4)\|(v>>4)` | `r==0` | 0 | 0 | **0** |
| **SRL v** | `r=v>>1` (bit7=0) | `r==0` | 0 | 0 | old bit 0 |

### 4.2 BIT / RES / SET

| Op | Operation | Z | N | H | C |
|---|---|---|---|---|---|
| **BIT b,v** | test bit `b` (no write) | `((v>>b)&1)==0` | 0 | **1** | `-` |
| **RES b,v** | `v &= ~(1<<b)` | `-` | `-` | `-` | `-` |
| **SET b,v** | `v \|= (1<<b)` | `-` | `-` | `-` | `-` |

> `BIT` never writes the operand back: `BIT b,[HL]` is **read-only** → 3 M-cycles (not 4). `RES`/`SET` on `[HL]` are read-modify-write → 4 M-cycles.

---

## 5. Opcode M-cycle Timing (complete)

Each entry = total M-cycles (×4 = T). Opcode fetch (1 M) is **included** in every count via the prefetch model (§9): the listed cost is the architectural instruction length in M-cycles.

### 5.1 8-bit loads

| Instruction | M | Notes |
|---|---|---|
| `LD r,r'` (both registers) | 1 | |
| `LD r,n8` | 2 | |
| `LD r,[HL]` / `LD [HL],r` | 2 | |
| `LD [HL],n8` | 3 | |
| `LD A,[BC]` / `LD A,[DE]` / `LD [BC],A` / `LD [DE],A` | 2 | |
| `LD A,[a16]` / `LD [a16],A` | 4 | |
| `LDH A,[a8]` / `LDH [a8],A` (`0xF0/0xE0`) | 3 | addr `0xFF00+a8` |
| `LDH A,[C]` / `LDH [C],A` (`0xF2/0xE2`) | 2 | addr `0xFF00+C` |
| `LD A,[HL+]` / `LD A,[HL-]` / `LD [HL+],A` / `LD [HL-],A` | 2 | HL post-inc/dec |

### 5.2 16-bit loads / stack

| Instruction | M | Notes |
|---|---|---|
| `LD rr,n16` | 3 | |
| `LD [a16],SP` (`0x08`) | 5 | writes SP low then high |
| `LD SP,HL` (`0xF9`) | 2 | extra internal M |
| `LD HL,SP+e8` (`0xF8`) | 3 | |
| `PUSH rr` | 4 | 1 internal (SP--) + 2 writes + fetch |
| `POP rr` | 3 | 2 reads + fetch; `POP AF` masks low nibble of F |

### 5.3 8-bit ALU (A,operand)

| Form | M |
|---|---|
| `OP A,r` (`ADD/ADC/SUB/SBC/AND/XOR/OR/CP`) | 1 |
| `OP A,[HL]` | 2 |
| `OP A,n8` (immediate forms `0xC6..0xFE`) | 2 |
| `INC r` / `DEC r` (register) | 1 |
| `INC [HL]` / `DEC [HL]` | 3 (read, modify, write-back) |

### 5.4 16-bit ALU

| Instruction | M |
|---|---|
| `ADD HL,rr` | 2 |
| `INC rr` / `DEC rr` | 2 |
| `ADD SP,e8` (`0xE8`) | 4 |

### 5.5 Rotate/CPL/SCF/CCF/DAA (non-CB)

| Instruction | M |
|---|---|
| `RLCA/RRCA/RLA/RRA` | 1 |
| `DAA/CPL/SCF/CCF` | 1 |
| `NOP` (`0x00`) | 1 |

### 5.6 CB-prefixed

| Form | M | Notes |
|---|---|---|
| `CB` + reg op (rotate/shift/SET/RES/BIT on `r≠[HL]`) | 2 | (CB fetch + op fetch) |
| `CB BIT b,[HL]` | 3 | read-only |
| `CB <rot/shift/SET/RES> b,[HL]` | 4 | read-modify-write |

### 5.7 Control flow (taken / not-taken differ)

| Instruction | M (taken) | M (not taken) |
|---|---|---|
| `JP n16` (`0xC3`) | 4 | — |
| `JP cc,n16` | 4 | 3 |
| `JP HL` (`0xE9`) | 1 | — (no internal cycle) |
| `JR e8` (`0x18`) | 3 | — |
| `JR cc,e8` | 3 | 2 |
| `CALL n16` (`0xCD`) | 6 | — |
| `CALL cc,n16` | 6 | 3 |
| `RET` (`0xC9`) | 4 | — |
| `RET cc` | 5 | 2 |
| `RETI` (`0xD9`) | 4 | — (sets IME=1 **immediately**, not delayed) |
| `RST n` (`0xC7..0xFF`) | 4 | — vector `= n & 0x38` |

> Timing rationale (taken branches): the extra M-cycle over the not-taken case is the internal "set PC" cycle (`JP/JR/CALL`) or, for `RET cc`, the condition-check cycle precedes the pops (`RET cc` = 5 even though `RET` = 4, because the conditional has a leading internal M-cycle). These exact splits are checked by `mooneye` `call_cc_timing`, `jp_cc_timing`, `ret_cc_timing`, `rst_timing`, `push_timing`, `pop_timing`.

### 5.8 Misc / system

| Instruction | M | Notes |
|---|---|---|
| `HALT` (`0x76`) | 1 (+ stall) | see §7 |
| `STOP` (`0x10 00`) | — | see §8 |
| `DI` (`0xF3`) | 1 | IME=0 immediately |
| `EI` (`0xFB`) | 1 | IME=1 after the **following** instruction (§6) |
| Illegal opcodes `D3 DB DD E3 E4 EB EC ED F4 FC FD` | — | lock up the CPU (hang); never fetch again |

---

## 6. EI / DI Timing

- **`DI`**: `IME := 0` takes effect **immediately** (before the next instruction's interrupt-check point).
- **`EI`**: schedules `IME := 1` to take effect **after the next instruction completes**. The interrupt-check that runs at the end of `EI` itself still sees `IME=0`; the check at the end of the instruction *following* `EI` sees `IME=1`.
- Implementation: maintain a 1-step pending flag `ime_pending`. On `EI`, set `ime_pending=1`. After fetching+executing each instruction, if `ime_pending` was set *before* this instruction began, commit `IME=1` and clear pending. `DI` clears both `IME` and `ime_pending`.
- `EI` then `DI` back-to-back → no interrupt window opens (the `DI` cancels the pending set). Verified by `mooneye ei_sequence`, `ei_timing`, `di_timing`.
- `RETI` sets `IME=1` **immediately** (no one-instruction delay), unlike `EI`.

---

## 7. Interrupt Dispatch

### 7.1 Registers

| Reg | Addr | Bits used | Notes |
|---|---|---|---|
| `IF` | `0xFF0F` | 4..0 | top 3 bits read as 1 (`0xE0` mask set). R/W. |
| `IE` | `0xFFFF` | 7..0 | **all 8 bits R/W** (top 3 usable as scratch; they participate in `IE&IF` for HALT wake but have no vector). |
| `IME` | internal | — | write-only via EI/DI/RETI/dispatch; not memory-mapped. |

`IF`/`IE` bit → interrupt → vector, **priority high→low (bit 0 highest)**:

| Bit | Source | Vector |
|---|---|---|
| 0 | VBlank | `0x0040` |
| 1 | LCD STAT | `0x0048` |
| 2 | Timer | `0x0050` |
| 3 | Serial | `0x0058` |
| 4 | Joypad | `0x0060` |

### 7.2 Dispatch trigger condition

At the interrupt-check point (after each instruction completes; see prefetch §9): if `IME==1` **and** `(IF & IE & 0x1F) != 0`, dispatch the highest-priority pending bit.

### 7.3 Dispatch sequence — **5 M-cycles**

| M-cycle | Action |
|---|---|
| 1 | Internal (wait state) — `IME := 0` is committed here; no bus access |
| 2 | Internal (wait state) — no bus access |
| 3 | `SP := SP-1`; write **PC high** to `[SP]` |
| 4 | `SP := SP-1`; write **PC low** to `[SP]` |
| 5 | Compute vector from the *currently* highest pending `IF&IE` bit; clear that `IF` bit; `PC := vector` |

> Exact obscure quirks the test ROMs (`mooneye ie_push`) enforce:
> - **The vector is latched late (cycle 5), and the `IF` bit cleared is re-evaluated then.** If during M-cycle 3/4 the PC-high or PC-low push writes to `0xFFFF` (`IE`) — i.e. `SP` points at `0xFFFF`/`0x0000` — and that write changes which bits are pending, the vector can change.
> - **Interrupt cancellation**: if by M-cycle 5 `(IF & IE & 0x1F) == 0` (because the stack push to `IE` cleared all enabled-pending bits, or a write cleared `IF`), **no vector exists → `PC := 0x0000`** and no `IF` bit is cleared. This is the documented "unwanted cancel" path.
> - If multiple bits are pending, the bit *cleared* in `IF` is the one matching the vector actually taken (the highest-priority one live at cycle 5).
> - Dispatch consumes one `HALT` wakeup but is otherwise indistinguishable from a `CALL` to the vector for stack/return purposes.

### 7.4 Interrupt latency interplay

- An interrupt cannot be dispatched in the middle of an instruction; only at the check point.
- `EI`'s delayed enable means an interrupt pending during `EI` is **not** taken until after the instruction following `EI` (the classic `EI; HALT` pattern, see §7.3 HALT-bug interaction).
- Writing `IF`/`IE` to manually request/clear is honored at the next check point.

---

## 8. HALT Semantics + HALT Bug

`HALT` (`0x76`) stops CPU clocking until a wake condition. Wake condition is **`(IF & IE & 0x1F) != 0`** (independent of `IME`).

### 8.1 Three cases at the moment `HALT` executes

| Case | `IME` | `IF&IE&0x1F` at HALT | Behavior |
|---|---|---|---|
| A | 1 | any | CPU halts; on wake, the **interrupt is serviced normally** (5-M dispatch) before the post-HALT instruction. |
| B | 0 | `== 0` (none pending) | CPU halts (low power); resumes when a bit becomes pending. Because `IME=0`, **the interrupt is NOT serviced** — execution simply continues at the instruction after `HALT`. |
| C | 0 | `!= 0` (already pending) | **HALT BUG**: `HALT` exits immediately; CPU does **not** enter halt state, and **PC fails to increment** for the next fetch. |

### 8.2 HALT bug exact mechanics (Case C)

- The byte immediately after `HALT` is fetched, executed, **but PC is not advanced past it** — so that same byte is read a *second* time and executed again (a 1-byte instruction runs twice; a multi-byte instruction's opcode is its own following byte).
- This "double read" repeats if the doubled byte is itself another `HALT`.
- Special sub-cases:
  - **`EI` immediately before `HALT`** (so the EI-delay still has `IME=0` at the `HALT`): the pending interrupt **is** serviced (EI's enable commits), the handler runs, and `RETI`/`RET` returns to the `HALT`, which executes again and waits for the next interrupt. The "EI wins" over the bug.
  - **`RST` immediately after the bugged `HALT`**: the `RST` pushes a return address pointing at the `RST` **itself** (not the following byte), so a later `RET` re-executes that `RST`.
  - If `EI` precedes and `RST` follows the bugged `HALT`, the **EI path wins**.
- Implementation: model a `halt_bug` latch. When entering `HALT` with `IME=0 && (IF&IE&0x1F)!=0`, set `halt_bug=1` and do **not** halt. On the next fetch, if `halt_bug`, fetch the opcode at `PC` **without incrementing PC**, then clear `halt_bug`.

### 8.3 HALT wake/exit timing

- In Case A/B, the CPU is unclocked while halted; the timer/PPU/DMA continue. On wake (`IF&IE&0x1F != 0`), exit takes the normal fetch path; Case A then runs the 5-M dispatch.
- There is no extra "wake" M-cycle penalty beyond the standard dispatch in Case A. Verified by `mooneye halt_ime0_nointr_timing`, `halt_ime1_timing`, `ei_timing`.

---

## 9. STOP Behavior

`STOP` is encoded `0x10` followed by a byte (canonically `0x00`, i.e. `STOP 0`). Behavior is mode- and joypad-dependent and is the most edge-case-laden instruction.

### 9.1 Decision table at `STOP` execution (DMG, after reading the `0x10` opcode)

Let `IME` be irrelevant here; what matters is whether an interrupt is pending (`IF&IE&0x1F`), whether buttons are held (any `P1` line selected & pressed pulling a line low), and whether a CGB speed-switch is armed (`KEY1` bit 0 = prepare).

| Buttons held? | Interrupt pending? | KEY1 bit0 (speed armed)? | Result |
|---|---|---|---|
| No | No | No | **Enter STOP mode**: CPU + LCD stop, oscillator continues, DIV reset. Wakes on joypad. Opcode is 2 bytes (the `0x00` is consumed). |
| No | No | Yes (CGB) | **Speed switch**: toggle single/double speed, `KEY1` bit7 flips, `KEY1` bit0 clears. ~`2050` M-cycles (`8200` T) stall during switch. CPU resumes; does **not** enter stop mode. 2-byte opcode. |
| No | Yes | No | STOP is a **1-byte** opcode here (the second byte is *not* consumed) and STOP mode is **not** entered; effectively a glitchy NOP-like. |
| Yes | No | No | STOP mode **not** entered; acts as a 1-byte opcode; the LCD turns off — "HALT-like" but with display off. (DMG hardware quirk.) |
| Yes | Yes | — | STOP is a 1-byte opcode; behavior is the "stop glitch": some instances hang. Treat the second byte as a normal next opcode. |

> Practical implementation: read second byte; decide length per table. On CGB with `KEY1 & 0x01`, perform speed switch (toggle `CPU clock`, update `KEY1`, insert the long stall) and **return**. Otherwise enter true STOP only in the (no buttons, no pending IRQ) case, reset `DIV` to 0, blank/disable the LCD, and wait for a joypad line transition.

### 9.2 DIV and STOP

- Entering true STOP and the CGB speed switch both **reset the DIV counter** (the internal 16-bit divider that feeds DIV/TIMA) to 0. Wake from STOP restarts it.

### 9.3 CGB speed switch (`KEY1` @ `0xFF4D`)

| Bit | Name | Meaning |
|---|---|---|
| 7 | Current speed | 0 = single (≈4.19 MHz), 1 = double (≈8.39 MHz). Read-only. |
| 6..1 | — | unused (read 1) |
| 0 | Prepare switch | write 1, then execute `STOP` to perform the switch; auto-clears after. |

The switch sequence: ensure no IRQ pending & no buttons → `LD A,$01; LDH ($4D),A; STOP`. Stall ≈ 2050 M-cycles, then bit 7 toggles, bit 0 clears, execution continues.

---

## 10. Memory Access ↔ M-cycle Mapping & Prefetch Model

### 10.1 One access per M-cycle

- Every bus transaction (opcode fetch, operand read, memory read, memory write, push, pop) is exactly **1 M-cycle = 4 T**, occurring on a fixed phase within the M-cycle.
- Internal-only M-cycles (e.g. `INC rr`'s extra cycle, taken-branch PC set, `ADD SP` adjust, interrupt wait states) do **no** bus access but still advance the global clock 4 T (PPU/timer/DMA observe them).
- The order of sub-accesses within an instruction matters for bus-visible side effects: e.g. `LD [a16],SP` writes SP-low **before** SP-high; `PUSH` writes high **before** low (decrementing SP each time); interrupt push writes PC-high then PC-low.

### 10.2 Prefetch / overlap model (fetch-execute pipeline)

The SM83 **overlaps the opcode fetch of the next instruction with the final M-cycle of the current instruction.** Model it as:

1. The CPU holds an `IR` (instruction register) and an internal latch. The opcode currently in `IR` was fetched during the *previous* instruction's last M-cycle.
2. Decode + execute consume the instruction's operand reads/writes/internals.
3. On the instruction's **last** M-cycle, the CPU performs the **fetch of the next opcode** into `IR` (PC→bus, PC++). This is why per-instruction M-cycle counts already "include" one fetch.
4. **Interrupt check point**: between finishing an instruction and committing the prefetched opcode, the CPU samples the interrupt condition. If an interrupt is to be dispatched, the prefetched opcode is **discarded/not executed**, the dispatch (§7.3) runs, and PC is *not* advanced by the discarded fetch (the pushed PC is the address of the would-be-next instruction).
5. The `EI` delay is naturally modeled because the IME commit happens at the check point *after* the instruction following `EI`.

> Consequences test ROMs rely on:
> - `HALT` bug (§8.2) is a direct manifestation: the prefetched-opcode fetch happens with PC not incremented.
> - Interrupt timing measured to the exact T-cycle (`mooneye` `intr_timing`, `ei_timing`) requires this overlap: the dispatch's 5 M-cycles begin at the check point, replacing the discarded prefetch.
> - Self-modifying code and `EI`/`DI`/`IF` writes that land on the boundary behave correctly only with the late check point.

### 10.3 Bus conflict / OAM-DMA note (CPU-relevant)

- During OAM DMA, the CPU can only access HRAM (`0xFF80–0xFFFE`); other reads return the DMA's current source byte (open-bus-like). The instruction stream must live in HRAM during DMA. (Detailed in the DMA spec; noted here because CPU fetches are affected.)

---

## 11. Implementation Checklist (test-ROM coverage map)

| Quirk | Test ROM |
|---|---|
| All ALU flags incl. ADC/SBC nibble+carry | Blargg `cpu_instrs` 01–11, `mooneye` |
| `DAA` sub-branch ignores >9 / >0x99; H always 0 | `mooneye daa`, Blargg 01 |
| `ADD SP,e8` / `LD HL,SP+e8` low-byte H/C; Z=N=0 | `mooneye add_sp_e_timing` |
| RLCA/RRCA/RLA/RRA Z=0 vs CB Z computed | Blargg `cpu_instrs` |
| Conditional branch taken/not-taken split | `mooneye jp_cc_timing, jr_cc_timing, call_cc_timing, ret_cc_timing` |
| PUSH/POP/RST exact cycles; POP AF low-nibble mask | `mooneye push_timing, pop_timing, rst_timing` |
| EI 1-instruction delay; EI;DI cancel; RETI immediate | `mooneye ei_timing, ei_sequence, di_timing` |
| Interrupt 5-M dispatch; vector late-latch; cancel→0x0000 | `mooneye ie_push`, `intr_timing` |
| HALT cases A/B/C + halt bug; EI;HALT; HALT;RST | `mooneye halt_*`, `little-things-gb double-halt-cancel`, SameSuite `ei_delay_halt` |
| STOP speed switch / button / IRQ matrix; DIV reset | CGB speed tests, `mooneye` div/stop |

---

**Sources consulted:** [Pan Docs — HALT](https://gbdev.io/pandocs/halt.html), [Pan Docs — Interrupts](https://gbdev.io/pandocs/Interrupts.html), [Pan Docs — CPU Instruction Set](https://gbdev.io/pandocs/CPU_Instruction_Set.html), [Demystifying the SM83 DAA Instruction (ollien)](https://blog.ollien.com/posts/gb-daa/), [Game Boy CPU internals (SonoSooS gist)](https://gist.github.com/SonoSooS/c0055300670d678b5ae8433e20bea595), [mooneye-gb ie_push issue #50](https://github.com/Gekkio/mooneye-gb/issues/50), [little-things-gb double-halt-cancel](https://github.com/nitro2k01/little-things-gb/tree/main/double-halt-cancel), [SameSuite ei_delay_halt](https://github.com/LIJI32/SameSuite/blob/master/interrupt/ei_delay_halt.asm), [gb-opcodes optables](https://gbdev.io/gb-opcodes/optables/).