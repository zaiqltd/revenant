//! REVENANT proof harness (brief §8).
//!
//! Two modes:
//!   scorecard run <rom> <detector> <maxframes> [refpng]   -- single-ROM child
//!   scorecard score <corpus_root> <out_dir>               -- parallel driver
//!
//! The driver shells out to *itself* (current_exe) once per ROM, so a ROM that
//! panics aborts only its own child and is recorded as a CRASH rather than taking
//! the whole run down. Children are bounded by a frame cap, so they always
//! terminate. Image tests write a framebuffer PNG and the driver diffs it with
//! tools/compare.py (palette-agnostic luminance buckets).
//!
//! Child exit codes: 0=PASS, 10=FAIL, 20=INCONCLUSIVE, 2=ERROR, anything else=CRASH.
//! The child also prints `RESULT:<STATUS>|<detail>` for the driver to capture.

use revenant_core::{GameBoy, SCREEN_H, SCREEN_W};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const EXIT_PASS: i32 = 0;
const EXIT_FAIL: i32 = 10;
const EXIT_INCONCLUSIVE: i32 = 20;
const EXIT_ERROR: i32 = 2;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("run") => run_one(&args),
        Some("score") => score_all(&args),
        _ => {
            eprintln!("usage: scorecard run <rom> <detector> <frames> [refpng]");
            eprintln!("       scorecard score <corpus_root> <out_dir>");
            std::process::exit(2);
        }
    }
}

// ===========================================================================
// CHILD: run a single ROM
// ===========================================================================

fn run_one(args: &[String]) {
    let rom_path = &args[2];
    let detector = &args[3];
    let frames: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(2000);
    let refpng = args.get(5).cloned();

    let rom = match std::fs::read(rom_path) {
        Ok(r) => r,
        Err(e) => {
            println!("RESULT:ERROR|read {e}");
            std::process::exit(EXIT_ERROR);
        }
    };
    let mut gb = GameBoy::new(rom, None, 48000);

    match detector.as_str() {
        "serial" | "screen" => detect_blargg(&mut gb, frames),
        "mooneye" => detect_mooneye(&mut gb, frames),
        "image" => detect_image(&mut gb, frames, refpng.as_deref()),
        other => {
            println!("RESULT:ERROR|unknown detector {other}");
            std::process::exit(EXIT_ERROR);
        }
    }
}

/// Blargg-style: watch BOTH the serial port and the on-screen BG tilemap for
/// "Passed"/"Failed". Blargg's font is loaded so that tilemap byte == ASCII, so
/// the screen text can be read directly out of VRAM — this is how the screen-only
/// ROMs (dmg_sound, oam_bug, halt_bug, mem_timing-2, ...) report their result.
fn detect_blargg(gb: &mut GameBoy, max_frames: u64) {
    for _ in 0..max_frames {
        gb.run_frame();
        let mut text: String = gb.serial_output().iter().map(|&b| b as char).collect();
        text.push('\n');
        text.push_str(&screen_text(gb));
        if let Some(s) = classify_blargg(&text) {
            return s; // exits the process
        }
    }
    let text = screen_text(gb);
    let tail: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let tail: String = tail.chars().take(70).collect();
    println!("RESULT:INCONCLUSIVE|screen=<{tail}>");
    std::process::exit(EXIT_INCONCLUSIVE);
}

fn classify_blargg(text: &str) -> Option<()> {
    if text.contains("Passed") {
        println!("RESULT:PASS|done");
        std::process::exit(EXIT_PASS);
    }
    if text.contains("Failed") || text.contains("Error") {
        // grab the line mentioning the failure for the detail
        let detail = text
            .lines()
            .find(|l| l.contains("Failed") || l.contains("Error"))
            .unwrap_or("")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let detail: String = detail.chars().take(60).collect();
        println!("RESULT:FAIL|{detail}");
        std::process::exit(EXIT_FAIL);
    }
    None
}

/// Reconstruct on-screen text from the BG tilemaps (both 0x9800 and 0x9C00),
/// interpreting each tilemap byte as an ASCII code (the Blargg font convention).
fn screen_text(gb: &GameBoy) -> String {
    let vram = gb.bus.ppu.vram_raw();
    let mut out = String::new();
    for base in [0x1800usize, 0x1C00usize] {
        for row in 0..18 {
            for col in 0..20 {
                let b = vram[base + row * 32 + col];
                out.push(if (0x20..0x7f).contains(&b) { b as char } else { ' ' });
            }
            out.push('\n');
        }
    }
    out
}

/// Mooneye / AGE / same-suite: the ROM executes `LD B,B` (0x40) once finished;
/// registers carry the Fibonacci magic on success.
fn detect_mooneye(gb: &mut GameBoy, max_frames: u64) {
    let frame_steps = revenant_core::CYCLES_PER_FRAME / 4; // ~M-cycles per frame
    for _ in 0..max_frames {
        for _ in 0..frame_steps {
            // peek the next opcode; LD B,B is the designated breakpoint
            let op = gb.bus.read(gb.cpu.pc);
            if op == 0x40 {
                let c = &gb.cpu;
                let fib = c.b == 3 && c.c == 5 && c.d == 8 && c.e == 13 && c.h == 21 && c.l == 34;
                if fib {
                    println!("RESULT:PASS|magic");
                    std::process::exit(EXIT_PASS);
                } else {
                    println!(
                        "RESULT:FAIL|regs b={:02x} c={:02x} d={:02x} e={:02x} h={:02x} l={:02x}",
                        c.b, c.c, c.d, c.e, c.h, c.l
                    );
                    std::process::exit(EXIT_FAIL);
                }
            }
            gb.step_instruction();
        }
    }
    println!("RESULT:INCONCLUSIVE|no LD B,B reached");
    std::process::exit(EXIT_INCONCLUSIVE);
}

/// Image test: run, dump framebuffer PNG to refpng-derived temp; the driver diffs.
/// Here `refpng` is actually the *output* path to write.
fn detect_image(gb: &mut GameBoy, frames: u64, out: Option<&str>) {
    let out = match out {
        Some(p) => p,
        None => {
            println!("RESULT:ERROR|image detector needs output path");
            std::process::exit(EXIT_ERROR);
        }
    };
    for _ in 0..frames {
        gb.run_frame();
    }
    let png = encode_png(gb.framebuffer(), SCREEN_W as u32, SCREEN_H as u32);
    if let Err(e) = std::fs::write(out, png) {
        println!("RESULT:ERROR|write png {e}");
        std::process::exit(EXIT_ERROR);
    }
    println!("RESULT:RENDERED|{out}");
    std::process::exit(EXIT_PASS); // driver decides pass/fail by diffing
}

// ===========================================================================
// DRIVER: enumerate corpus, fan out children, aggregate
// ===========================================================================

#[derive(Clone)]
struct Job {
    category: String,
    name: String,
    rom: PathBuf,
    detector: &'static str,
    frames: u64,
    refpng: Option<PathBuf>, // for image jobs: the reference to diff against
}

#[derive(Clone, Debug)]
struct Outcome {
    category: String,
    name: String,
    status: String, // PASS / FAIL / INCONCLUSIVE / CRASH / ERROR
    detail: String,
}

fn score_all(args: &[String]) {
    let root = PathBuf::from(args.get(2).map(|s| s.as_str()).unwrap_or("roms/gbtr"));
    let out_dir = PathBuf::from(args.get(3).map(|s| s.as_str()).unwrap_or("out"));
    std::fs::create_dir_all(&out_dir).ok();
    let tmp_dir = out_dir.join("shots");
    std::fs::create_dir_all(&tmp_dir).ok();

    let jobs = build_jobs(&root);
    eprintln!("[scorecard] {} jobs across the canonical gate set", jobs.len());

    let self_exe = std::env::current_exe().expect("current_exe");
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    eprintln!("[scorecard] running {threads}-way parallel");

    let jobs = Arc::new(jobs);
    let next = Arc::new(AtomicUsize::new(0));
    let results: Arc<Mutex<Vec<Outcome>>> = Arc::new(Mutex::new(Vec::new()));
    let done = Arc::new(AtomicUsize::new(0));
    let total = jobs.len();

    let mut handles = Vec::new();
    for _ in 0..threads {
        let jobs = jobs.clone();
        let next = next.clone();
        let results = results.clone();
        let done = done.clone();
        let self_exe = self_exe.clone();
        let tmp_dir = tmp_dir.clone();
        handles.push(std::thread::spawn(move || loop {
            let i = next.fetch_add(1, Ordering::SeqCst);
            if i >= jobs.len() {
                break;
            }
            let job = &jobs[i];
            let outcome = run_job(&self_exe, job, &tmp_dir);
            let n = done.fetch_add(1, Ordering::SeqCst) + 1;
            eprintln!("[{n}/{total}] {:<28} {:<7} {}", job.category, outcome.status, job.name);
            results.lock().unwrap().push(outcome);
        }));
    }
    for h in handles {
        h.join().ok();
    }

    let mut results = Arc::try_unwrap(results).unwrap().into_inner().unwrap();
    results.sort_by(|a, b| (a.category.clone(), a.name.clone()).cmp(&(b.category.clone(), b.name.clone())));
    write_reports(&results, &out_dir);
}

fn run_job(self_exe: &Path, job: &Job, tmp_dir: &Path) -> Outcome {
    let mut cmd = Command::new(self_exe);
    cmd.arg("run").arg(&job.rom).arg(job.detector).arg(job.frames.to_string());

    let shot = tmp_dir.join(format!("{}__{}.png", job.category, sanitize(&job.name)));
    if job.detector == "image" {
        cmd.arg(&shot);
    }
    let output = match cmd.output() {
        Ok(o) => o,
        Err(e) => {
            return Outcome {
                category: job.category.clone(),
                name: job.name.clone(),
                status: "ERROR".into(),
                detail: format!("spawn {e}"),
            }
        }
    };
    let code = output.status.code();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = stdout
        .lines()
        .find(|l| l.starts_with("RESULT:"))
        .map(|l| l.trim_start_matches("RESULT:").to_string())
        .unwrap_or_default();

    // For image jobs, the child only renders; the driver diffs here.
    if job.detector == "image" {
        if !shot.exists() {
            return Outcome {
                category: job.category.clone(),
                name: job.name.clone(),
                status: "CRASH".into(),
                detail: format!("no render (exit {:?})", code),
            };
        }
        let Some(refp) = &job.refpng else {
            return Outcome {
                category: job.category.clone(),
                name: job.name.clone(),
                status: "ERROR".into(),
                detail: "no reference".into(),
            };
        };
        return diff_image(job, &shot, refp);
    }

    let status = match code {
        Some(EXIT_PASS) => "PASS",
        Some(EXIT_FAIL) => "FAIL",
        Some(EXIT_INCONCLUSIVE) => "INCONCLUSIVE",
        Some(EXIT_ERROR) => "ERROR",
        _ => "CRASH",
    };
    Outcome {
        category: job.category.clone(),
        name: job.name.clone(),
        status: status.into(),
        detail: detail.splitn(2, '|').nth(1).unwrap_or("").to_string(),
    }
}

fn diff_image(job: &Job, shot: &Path, refp: &Path) -> Outcome {
    let out = Command::new("python")
        .arg("tools/compare.py")
        .arg(shot)
        .arg(refp)
        .output();
    match out {
        Ok(o) => {
            let txt = String::from_utf8_lossy(&o.stdout);
            let line = txt.lines().find(|l| l.contains("differing")).unwrap_or("").trim();
            let status = if o.status.success() { "PASS" } else { "FAIL" };
            Outcome {
                category: job.category.clone(),
                name: job.name.clone(),
                status: status.into(),
                detail: line.to_string(),
            }
        }
        Err(e) => Outcome {
            category: job.category.clone(),
            name: job.name.clone(),
            status: "ERROR".into(),
            detail: format!("python {e}"),
        },
    }
}

// ---- corpus enumeration ---------------------------------------------------

fn build_jobs(root: &Path) -> Vec<Job> {
    let mut jobs = Vec::new();

    // 1. Blargg (serial) -- the brief's Tier 1 + Tier 3 named gates.
    let blargg: &[(&str, &str, u64)] = &[
        ("cpu_instrs", "blargg/cpu_instrs/cpu_instrs.gb", 4000),
        ("instr_timing", "blargg/instr_timing/instr_timing.gb", 2000),
        ("mem_timing", "blargg/mem_timing/mem_timing.gb", 2000),
        ("mem_timing-2", "blargg/mem_timing-2/mem_timing.gb", 2000),
        ("halt_bug", "blargg/halt_bug.gb", 3000),
        ("oam_bug", "blargg/oam_bug/oam_bug.gb", 6000),
        ("dmg_sound", "blargg/dmg_sound/dmg_sound.gb", 12000),
        ("cgb_sound", "blargg/cgb_sound/cgb_sound.gb", 12000),
        ("interrupt_time", "blargg/interrupt_time/interrupt_time.gb", 3000),
    ];
    for (name, rel, frames) in blargg {
        let p = root.join(rel);
        if p.exists() {
            jobs.push(Job {
                category: "blargg".into(),
                name: (*name).into(),
                rom: p,
                detector: "screen",
                frames: *frames,
                refpng: None,
            });
        }
    }
    // Also the 11 individual cpu_instrs sub-tests, if present.
    let ci = root.join("blargg/cpu_instrs/individual");
    if ci.is_dir() {
        for f in list_roms(&ci) {
            let name = format!("cpu_instrs/{}", stem(&f));
            jobs.push(Job { category: "blargg".into(), name, rom: f, detector: "screen", frames: 1500, refpng: None });
        }
    }

    // 2. Mooneye acceptance + emulator-only (register magic).
    for sub in ["acceptance", "emulator-only"] {
        let dir = root.join("mooneye-test-suite").join(sub);
        for f in list_roms_rec(&dir) {
            if skip_manual(&f) { continue; }
            let name = rel_name(&dir, &f);
            jobs.push(Job { category: format!("mooneye/{sub}"), name, rom: f, detector: "mooneye", frames: 1200, refpng: None });
        }
    }

    // 3. same-suite (register magic).
    let ss = root.join("same-suite");
    for f in list_roms_rec(&ss) {
        let name = rel_name(&ss, &f);
        jobs.push(Job { category: "same-suite".into(), name, rom: f, detector: "mooneye", frames: 3000, refpng: None });
    }

    // 4. AGE test roms: register magic, except screenshot-based ones (sibling png).
    let age = root.join("age-test-roms");
    for f in list_roms_rec(&age) {
        let name = rel_name(&age, &f);
        if let Some(refp) = sibling_ref(&f) {
            jobs.push(Job { category: "age".into(), name, rom: f, detector: "image", frames: 200, refpng: Some(refp) });
        } else {
            jobs.push(Job { category: "age".into(), name, rom: f, detector: "mooneye", frames: 1500, refpng: None });
        }
    }

    // 5. acid2 (image vs the staged references).
    for (name, rel, refrel) in [
        ("dmg-acid2", "dmg-acid2/dmg-acid2.gb", "../tests/dmg-acid2-ref.png"),
        ("cgb-acid2", "cgb-acid2/cgb-acid2.gbc", "../tests/cgb-acid2-ref.png"),
    ] {
        let p = root.join(rel);
        let r = root.join(refrel);
        if p.exists() && r.exists() {
            jobs.push(Job { category: "acid2".into(), name: name.into(), rom: p, detector: "image", frames: 60, refpng: Some(r) });
        }
    }
    // cgb-acid-hell (sibling png if present)
    let hell_dir = root.join("cgb-acid-hell");
    for f in list_roms_rec(&hell_dir) {
        if let Some(refp) = sibling_ref(&f) {
            jobs.push(Job { category: "acid2".into(), name: format!("hell/{}", stem(&f)), rom: f, detector: "image", frames: 60, refpng: Some(refp) });
        }
    }

    // 6. mealybug-tearoom PPU (Tier S, image-exact vs *_dmg_blob / *_cgb_c).
    let mb = root.join("mealybug-tearoom-tests/ppu");
    for f in list_roms_rec(&mb) {
        if let Some(refp) = mealybug_ref(&f) {
            jobs.push(Job { category: "mealybug".into(), name: stem(&f), rom: f, detector: "image", frames: 30, refpng: Some(refp) });
        }
    }

    // 7. scribbltests (image, sibling png).
    let scr = root.join("scribbltests");
    for f in list_roms_rec(&scr) {
        if let Some(refp) = sibling_ref(&f) {
            jobs.push(Job { category: "scribbl".into(), name: stem(&f), rom: f, detector: "image", frames: 60, refpng: Some(refp) });
        }
    }

    jobs
}

fn skip_manual(p: &Path) -> bool {
    let s = p.to_string_lossy();
    s.contains("manual-only") || s.contains("utils")
}

/// mealybug refs are `<stem>_dmg_blob.png` (DMG) or `<stem>_cgb_c.png` (CGB),
/// chosen by the ROM's CGB flag.
fn mealybug_ref(rom: &Path) -> Option<PathBuf> {
    let dir = rom.parent()?;
    let stem = rom.file_stem()?.to_string_lossy().to_string();
    let cgb = std::fs::read(rom).ok().and_then(|r| r.get(0x143).copied()).map(|b| b & 0x80 != 0).unwrap_or(false);
    let candidates = if cgb {
        vec![format!("{stem}_cgb_c.png"), format!("{stem}_cgb_d.png"), format!("{stem}_cgb.png")]
    } else {
        vec![format!("{stem}_dmg_blob.png"), format!("{stem}_dmg.png")]
    };
    for c in candidates {
        let p = dir.join(&c);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// A reference image sharing the rom's stem (possibly with a -dmg / -cgb suffix).
fn sibling_ref(rom: &Path) -> Option<PathBuf> {
    let dir = rom.parent()?;
    let stem = rom.file_stem()?.to_string_lossy().to_string();
    let cgb = std::fs::read(rom).ok().and_then(|r| r.get(0x143).copied()).map(|b| b & 0x80 != 0).unwrap_or(false);
    let order: Vec<String> = if cgb {
        vec![format!("{stem}-cgb.png"), format!("{stem}-cgb-dmg.png"), format!("{stem}.png"), format!("{stem}-dmg.png")]
    } else {
        vec![format!("{stem}-dmg.png"), format!("{stem}-cgb-dmg.png"), format!("{stem}.png"), format!("{stem}-cgb.png")]
    };
    for c in order {
        let p = dir.join(&c);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

// ---- small fs helpers -----------------------------------------------------

fn list_roms(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_rom(p))
        .collect();
    v.sort();
    v
}

fn list_roms_rec(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.retain(|p| is_rom(p));
    out.sort();
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else {
                out.push(p);
            }
        }
    }
}

fn is_rom(p: &Path) -> bool {
    matches!(p.extension().and_then(|e| e.to_str()), Some("gb") | Some("gbc"))
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

fn rel_name(base: &Path, p: &Path) -> String {
    p.strip_prefix(base).unwrap_or(p).with_extension("").to_string_lossy().replace('\\', "/")
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect()
}

// ---- report writers -------------------------------------------------------

fn write_reports(results: &[Outcome], out_dir: &Path) {
    use std::collections::BTreeMap;
    let mut by_cat: BTreeMap<String, (usize, usize)> = BTreeMap::new(); // (pass, total)
    for r in results {
        let e = by_cat.entry(r.category.clone()).or_insert((0, 0));
        e.1 += 1;
        if r.status == "PASS" {
            e.0 += 1;
        }
    }
    let total_pass: usize = by_cat.values().map(|(p, _)| p).sum();
    let total_all: usize = by_cat.values().map(|(_, t)| t).sum();

    // JSON
    let mut json = String::from("{\n");
    json.push_str(&format!("  \"total\": {{\"pass\": {total_pass}, \"of\": {total_all}}},\n"));
    json.push_str("  \"categories\": {\n");
    let cats: Vec<_> = by_cat.iter().collect();
    for (i, (cat, (p, t))) in cats.iter().enumerate() {
        let comma = if i + 1 < cats.len() { "," } else { "" };
        json.push_str(&format!("    \"{cat}\": {{\"pass\": {p}, \"of\": {t}}}{comma}\n"));
    }
    json.push_str("  },\n  \"results\": [\n");
    for (i, r) in results.iter().enumerate() {
        let comma = if i + 1 < results.len() { "," } else { "" };
        json.push_str(&format!(
            "    {{\"category\": \"{}\", \"name\": \"{}\", \"status\": \"{}\", \"detail\": {}}}{comma}\n",
            r.category, r.name, r.status, json_str(&r.detail)
        ));
    }
    json.push_str("  ]\n}\n");
    std::fs::write(out_dir.join("scorecard.json"), &json).ok();

    // Markdown
    let mut md = String::new();
    md.push_str("# REVENANT — Accuracy Scorecard\n\n");
    md.push_str(&format!("**Canonical gate set: passes {total_pass}/{total_all}**\n\n"));
    md.push_str("| Category | Pass | Total |\n|---|---:|---:|\n");
    for (cat, (p, t)) in &by_cat {
        md.push_str(&format!("| {cat} | {p} | {t} |\n"));
    }
    md.push_str("\n<details><summary>Per-ROM detail</summary>\n\n");
    md.push_str("| Category | ROM | Status | Detail |\n|---|---|---|---|\n");
    for r in results {
        let icon = match r.status.as_str() {
            "PASS" => "✅",
            "FAIL" => "❌",
            "INCONCLUSIVE" => "⚠️",
            "CRASH" => "💥",
            _ => "·",
        };
        md.push_str(&format!("| {} | {} | {icon} {} | {} |\n", r.category, r.name, r.status, r.detail.replace('|', "\\|")));
    }
    md.push_str("\n</details>\n");
    std::fs::write(out_dir.join("SCORECARD.md"), &md).ok();

    // Console summary
    eprintln!("\n================ SCORECARD ================");
    for (cat, (p, t)) in &by_cat {
        eprintln!("  {cat:<22} {p:>3}/{t}");
    }
    eprintln!("  {:<22} {total_pass:>3}/{total_all}", "TOTAL");
    eprintln!("===========================================");
    eprintln!("wrote {}/scorecard.json + SCORECARD.md", out_dir.display());
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ---- PNG writer (store-mode, same as the screenshot example) --------------

fn encode_png(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut raw = Vec::with_capacity((h * (w * 4 + 1)) as usize);
    for y in 0..h {
        raw.push(0);
        let row = &rgba[(y * w * 4) as usize..((y + 1) * w * 4) as usize];
        raw.extend_from_slice(row);
    }
    let zlib = zlib_store(&raw);
    let mut png = Vec::new();
    png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    write_chunk(&mut png, b"IHDR", &ihdr);
    write_chunk(&mut png, b"IDAT", &zlib);
    write_chunk(&mut png, b"IEND", &[]);
    png
}

fn write_chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in tag.iter().chain(data) {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    out.extend_from_slice(&(crc ^ 0xFFFF_FFFF).to_be_bytes());
}

fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let mut i = 0;
    while i < data.len() {
        let chunk = (data.len() - i).min(0xFFFF);
        let last = i + chunk >= data.len();
        out.push(if last { 1 } else { 0 });
        out.extend_from_slice(&(chunk as u16).to_le_bytes());
        out.extend_from_slice(&(!(chunk as u16)).to_le_bytes());
        out.extend_from_slice(&data[i..i + chunk]);
        i += chunk;
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    out.extend_from_slice(&((b << 16) | a).to_be_bytes());
    out
}
