//! Flat C-ABI over the REVENANT core. wasm32 is single-threaded, so a single
//! global instance with raw-pointer buffers shared into JS is both safe and the
//! simplest possible glue (no wasm-bindgen needed).

use core::ptr::addr_of_mut;
use revenant_core::joypad::Button;
use revenant_core::GameBoy;

static mut GB: Option<GameBoy> = None;
static mut INPUT: Vec<u8> = Vec::new();
static mut AUDIO: Vec<f32> = Vec::new();
static mut SAVE: Vec<u8> = Vec::new();
static mut SCRATCH: Vec<u8> = Vec::new();

#[inline]
fn gb() -> Option<&'static mut GameBoy> {
    unsafe { (*addr_of_mut!(GB)).as_mut() }
}

/// JS calls this to get a buffer of `len` bytes to write ROM (or save) data into.
#[no_mangle]
pub extern "C" fn revenant_input_ptr(len: usize) -> *mut u8 {
    unsafe {
        let input = &mut *addr_of_mut!(INPUT);
        input.clear();
        input.resize(len, 0);
        input.as_mut_ptr()
    }
}

#[no_mangle]
pub extern "C" fn revenant_init(rom_len: usize, sample_rate: u32) -> u32 {
    unsafe {
        let input = &mut *addr_of_mut!(INPUT);
        let rom = input[..rom_len.min(input.len())].to_vec();
        let machine = GameBoy::new(rom, None, sample_rate);
        let is_cgb = machine.cgb as u32;
        *addr_of_mut!(GB) = Some(machine);
        is_cgb
    }
}

#[no_mangle]
pub extern "C" fn revenant_reset() {
    if let Some(g) = gb() {
        g.reset();
    }
}

#[no_mangle]
pub extern "C" fn revenant_run_frame() {
    if let Some(g) = gb() {
        g.run_frame();
        let a = g.take_audio();
        unsafe {
            *addr_of_mut!(AUDIO) = a;
        }
    }
}

#[no_mangle]
pub extern "C" fn revenant_framebuffer_ptr() -> *const u8 {
    match gb() {
        Some(g) => g.framebuffer().as_ptr(),
        None => core::ptr::null(),
    }
}

#[no_mangle]
pub extern "C" fn revenant_audio_ptr() -> *const f32 {
    unsafe { (*addr_of_mut!(AUDIO)).as_ptr() }
}
#[no_mangle]
pub extern "C" fn revenant_audio_len() -> usize {
    unsafe { (*addr_of_mut!(AUDIO)).len() }
}

#[no_mangle]
pub extern "C" fn revenant_set_buttons(bits: u8) {
    if let Some(g) = gb() {
        g.set_buttons(bits);
    }
}

#[no_mangle]
pub extern "C" fn revenant_set_button(button: u8, pressed: u8) {
    if let Some(g) = gb() {
        let b = match button {
            0 => Button::Right,
            1 => Button::Left,
            2 => Button::Up,
            3 => Button::Down,
            4 => Button::A,
            5 => Button::B,
            6 => Button::Select,
            _ => Button::Start,
        };
        g.set_button(b, pressed != 0);
    }
}

// ---- rewind (Boss I) ----------------------------------------------------

#[no_mangle]
pub extern "C" fn revenant_set_recording(on: u32) {
    if let Some(g) = gb() {
        g.set_recording(on != 0);
    }
}
/// Restore the machine one frame back. Returns 1 if a frame was available.
#[no_mangle]
pub extern "C" fn revenant_rewind_frame() -> u32 {
    gb().map(|g| g.rewind_frame() as u32).unwrap_or(0)
}
#[no_mangle]
pub extern "C" fn revenant_rewind_len() -> u32 {
    gb().map(|g| g.rewind_len() as u32).unwrap_or(0)
}

// ---- battery save -------------------------------------------------------

#[no_mangle]
pub extern "C" fn revenant_has_battery() -> u32 {
    gb().map(|g| g.has_battery() as u32).unwrap_or(0)
}
#[no_mangle]
pub extern "C" fn revenant_ram_dirty() -> u32 {
    gb().map(|g| g.ram_is_dirty() as u32).unwrap_or(0)
}
#[no_mangle]
pub extern "C" fn revenant_save_ram_ptr() -> *const u8 {
    if let Some(g) = gb() {
        let s = g.save_ram();
        unsafe {
            *addr_of_mut!(SAVE) = s;
            (*addr_of_mut!(SAVE)).as_ptr()
        }
    } else {
        core::ptr::null()
    }
}
#[no_mangle]
pub extern "C" fn revenant_save_ram_len() -> usize {
    unsafe { (*addr_of_mut!(SAVE)).len() }
}
#[no_mangle]
pub extern "C" fn revenant_load_ram(len: usize) {
    if let Some(g) = gb() {
        unsafe {
            let input = &*addr_of_mut!(INPUT);
            g.load_ram(&input[..len.min(input.len())]);
        }
    }
}

// ---- link cable (netplay) -----------------------------------------------

#[no_mangle]
pub extern "C" fn revenant_set_link_incoming(byte: u8) {
    if let Some(g) = gb() {
        g.set_link_incoming(byte);
    }
}
#[no_mangle]
pub extern "C" fn revenant_take_link_sent() -> i32 {
    if let Some(g) = gb() {
        match g.take_link_sent() {
            Some(b) => b as i32,
            None => -1,
        }
    } else {
        -1
    }
}
#[no_mangle]
pub extern "C" fn revenant_link_receive_external(byte: u8) -> u8 {
    gb().map(|g| g.link_receive_external(byte)).unwrap_or(0xFF)
}

// ---- debugger -----------------------------------------------------------

/// Pack the CPU registers + key state into SCRATCH and return its pointer.
/// Layout (little-endian): AF BC DE HL SP PC (u16 x6), then flags byte: IME,
/// halted, mode, ly, lcdc, IF, IE, double_speed.
#[no_mangle]
pub extern "C" fn revenant_cpu_state_ptr() -> *const u8 {
    let g = match gb() {
        Some(g) => g,
        None => return core::ptr::null(),
    };
    let c = &g.cpu;
    let mut v: Vec<u8> = Vec::with_capacity(24);
    let af = ((c.a as u16) << 8) | c.f as u16;
    let bc = ((c.b as u16) << 8) | c.c as u16;
    let de = ((c.d as u16) << 8) | c.e as u16;
    let hl = ((c.h as u16) << 8) | c.l as u16;
    for r in [af, bc, de, hl, c.sp, c.pc] {
        v.extend_from_slice(&r.to_le_bytes());
    }
    v.push(c.ime as u8);
    v.push(c.halted as u8);
    v.push(g.bus.ppu.mode);
    v.push(g.bus.ppu.ly);
    v.push(g.bus.ppu.lcdc);
    v.push(g.bus.intf);
    v.push(g.bus.inte);
    v.push(g.bus.double_speed as u8);
    unsafe {
        *addr_of_mut!(SCRATCH) = v;
        (*addr_of_mut!(SCRATCH)).as_ptr()
    }
}

#[no_mangle]
pub extern "C" fn revenant_read_mem(addr: u16) -> u8 {
    gb().map(|g| g.bus.read(addr)).unwrap_or(0xFF)
}

#[no_mangle]
pub extern "C" fn revenant_step_instruction() {
    if let Some(g) = gb() {
        g.step_instruction();
    }
}

/// Copy `len` bytes starting at `addr` into SCRATCH; return pointer.
#[no_mangle]
pub extern "C" fn revenant_dump_mem(addr: u16, len: u16) -> *const u8 {
    let g = match gb() {
        Some(g) => g,
        None => return core::ptr::null(),
    };
    let mut v = Vec::with_capacity(len as usize);
    for i in 0..len {
        v.push(g.bus.read(addr.wrapping_add(i)));
    }
    unsafe {
        *addr_of_mut!(SCRATCH) = v;
        (*addr_of_mut!(SCRATCH)).as_ptr()
    }
}
