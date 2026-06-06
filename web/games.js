// REVENANT catalog + app controller. Lists the bundled original homebrew games,
// lets you load your own .gb/.gbc, and drives the player view + live debugger.
import { Revenant, WIDTH, HEIGHT, KEYMAP } from './revenant.js';

// Bundled games (built by `cargo run --example make<id>` -> web/<file>).
export const CATALOG = [
  { id: 'snake',    title: 'Snake',    file: 'snake.gb',    accent: '#7dff9b', tag: 'arcade',
    desc: 'Eat the food, grow longer, don’t hit the walls or yourself.', controls: 'D-Pad — steer' },
  { id: 'breakout', title: 'Breakout', file: 'breakout.gb', accent: '#37e0c8', tag: 'arcade',
    desc: 'Bounce the ball off your paddle and clear every brick.',      controls: '← → — move paddle' },
  { id: 'blocks',   title: 'Blocks',   file: 'blocks.gb',   accent: '#ff8db4', tag: 'puzzle',
    desc: 'Stack the falling blocks and clear full rows.',               controls: '← → move · ↓ drop' },
  { id: 'flap',     title: 'Flap',     file: 'flap.gb',     accent: '#5cc8ff', tag: 'arcade',
    desc: 'Flap through the gaps. One tap keeps you airborne.',          controls: 'A / ↑ — flap' },
  { id: 'blaster',  title: 'Blaster',  file: 'blaster.gb',  accent: '#ff6f6f', tag: 'shooter',
    desc: 'Move, shoot, clear the wave before it reaches you.',          controls: '← → move · A fire' },
  { id: 'pong',     title: 'Pong',     file: 'pong.gb',     accent: '#9dffe0', tag: 'vs cpu',
    desc: 'Rally against the CPU. First to seven points wins.',          controls: '↑ ↓ — paddle' },
  { id: 'crosser',  title: 'Crosser',  file: 'crosser.gb',  accent: '#9bff6f', tag: 'arcade',
    desc: 'Cross the lanes of traffic without getting hit.',             controls: 'D-Pad — hop' },
  { id: 'maze',     title: 'Maze',     file: 'maze.gb',     accent: '#c0a0ff', tag: 'puzzle',
    desc: 'Find your way through to the goal.',                          controls: 'D-Pad — move' },
  { id: 'memory',   title: 'Memory',   file: 'memory.gb',   accent: '#ffd27d', tag: 'brain',
    desc: 'Watch the sequence, then repeat it back. It grows.',          controls: 'D-Pad — pads' },
  { id: 'dodge',    title: 'Dodge',    file: 'dodge.gb',    accent: '#ffcf5c', tag: 'arcade',
    desc: 'Weave between the falling blocks. Survive as long as you can.', controls: '← → — move' },
  { id: 'mover',    title: 'Hello',    file: 'game.gb',     accent: '#c78dff', tag: 'demo',
    desc: 'A movable smiley — the very first thing this emulator ran.',   controls: 'Arrows — move' },
];

const rev = new Revenant();
let ready = false;
let picking = false; // set when a game is chosen, so the thumbnail pass stops touching the wasm

// ---- DOM ----
const $ = (s) => document.querySelector(s);
const catalogView = $('#catalog');
const playerView = $('#player');
const grid = $('#grid');
const canvas = $('#screen');
const cctx = canvas.getContext('2d', { alpha: false });
const img = cctx.createImageData(WIDTH, HEIGHT);
const titleEl = $('#nowtitle');
const ctrlEl = $('#nowcontrols');
const regBox = $('#regs');

function render(fb) { img.data.set(fb); cctx.putImageData(img, 0, 0); }

const hx = (v, w) => v.toString(16).toUpperCase().padStart(w, '0');
rev.onframe = (s) => {
  if (!s || !regBox) return;
  regBox.innerHTML =
    `<b>AF</b> ${hx(s.AF,4)} <b>BC</b> ${hx(s.BC,4)} <b>DE</b> ${hx(s.DE,4)} <b>HL</b> ${hx(s.HL,4)}` +
    ` <b>SP</b> ${hx(s.SP,4)} <b>PC</b> ${hx(s.PC,4)}<br>` +
    `<b>LY</b> ${s.LY} <b>mode</b> ${s.MODE} <b>LCDC</b> ${hx(s.LCDC,2)} <b>IF</b> ${hx(s.IF,2)} ` +
    `<b>IE</b> ${hx(s.IE,2)} <b>IME</b> ${s.IME} · <b>${s.fps} fps</b>`;
};

// ---- boot ----
async function boot() { if (!ready) { await rev.load('revenant.wasm'); ready = true; } }

function buildGrid() {
  grid.innerHTML = '';
  for (const g of CATALOG) {
    const card = document.createElement('button');
    card.className = 'card';
    card.style.setProperty('--accent', g.accent);
    card.innerHTML =
      `<div class="thumb" style="--a:${g.accent}"><canvas class="thumbc" width="160" height="144"></canvas>` +
      `<span class="thumbl">${g.title[0]}</span></div>` +
      `<div class="cbody"><div class="ctitle">${g.title} <i>${g.tag}</i></div>` +
      `<div class="cdesc">${g.desc}</div><div class="cctrl">${g.controls}</div></div>`;
    card.addEventListener('click', () => playFromUrl(g.file, g.title, g.controls));
    g._canvas = card.querySelector('.thumbc');
    grid.appendChild(card);
  }
  // bring-your-own card (always present, independent of thumbnail rendering)
  const own = document.createElement('label');
  own.className = 'card own';
  own.innerHTML = `<div class="thumb byo"><span>+</span></div><div class="cbody">` +
    `<div class="ctitle">Your ROM</div><div class="cdesc">Load any .gb / .gbc from your device.</div>` +
    `<div class="cctrl">click or drop a file</div></div><input type="file" accept=".gb,.gbc,.bin" hidden>`;
  own.querySelector('input').addEventListener('change', async (e) => {
    const f = e.target.files[0]; if (f) playBytes(new Uint8Array(await f.arrayBuffer()), f.name, 'D-Pad · X/Z · Enter/Shift');
  });
  grid.appendChild(own);
}

// Render each game's title screen into its catalog card (sequential — shares the
// one wasm instance; runs before the player needs it, and play re-loads anyway).
async function renderThumbs() {
  await boot();
  for (const g of CATALOG) {
    if (picking) return;            // user picked a game -> stop touching the shared wasm
    try {
      const bytes = new Uint8Array(await (await fetch(g.file)).arrayBuffer());
      if (picking) return;
      rev.loadRom(bytes);
      for (let i = 0; i < 36; i++) rev.ex.revenant_run_frame(); // settle on the title
      const ptr = rev.ex.revenant_framebuffer_ptr();
      const fb = new Uint8Array(rev.ex.memory.buffer, ptr, 160 * 144 * 4);
      const tctx = g._canvas.getContext('2d');
      const im = tctx.createImageData(160, 144);
      im.data.set(fb);
      tctx.putImageData(im, 0, 0);
      g._canvas.classList.add('ready');
    } catch (_) { /* keep the letter fallback */ }
    await new Promise((r) => setTimeout(r, 0));
  }
  rev.romLoaded = false; // force a clean (re)load when a game is actually picked
}

async function playFromUrl(file, title, controls) {
  try {
    const bytes = new Uint8Array(await (await fetch(file)).arrayBuffer());
    playBytes(bytes, title, controls);
  } catch (e) { alert(`Couldn’t load ${file}.\nBuild the ROMs first: cargo run --example make<game>`); }
}

async function playBytes(bytes, title, controls) {
  picking = true;            // stop the thumbnail pass from racing on the shared wasm
  await boot();
  rev.initAudio();
  const cgb = rev.loadRom(bytes);
  titleEl.textContent = title + '  ·  ' + (cgb ? 'GBC' : 'GB');
  ctrlEl.textContent = controls || '';
  catalogView.hidden = true; playerView.hidden = false;
  $('#pause').textContent = 'Pause';
  rev.start(render);
  canvas.focus?.();
}

function backToCatalog() { rev.stop(); playerView.hidden = true; catalogView.hidden = false; }

// ---- player controls ----
$('#back').addEventListener('click', backToCatalog);
$('#reset').addEventListener('click', () => rev.reset());
$('#pause').addEventListener('click', () => { $('#pause').textContent = rev.togglePause() ? 'Pause' : 'Resume'; });
const muteBtn = $('#mute');
muteBtn.addEventListener('click', () => { const m = !rev.muted; rev.setMuted(m); muteBtn.textContent = m ? '🔇' : '🔊'; });
$('#full')?.addEventListener('click', () => {
  const el = $('#lcdwrap');
  if (!document.fullscreenElement) el.requestFullscreen?.(); else document.exitFullscreen?.();
});

// rewind (Boss I): hold R or the on-screen ⟲ button to run the game backwards
const rewindOverlay = $('#rewind-ind');
function setRewind(on) {
  rev.setRewind(on);
  if (rewindOverlay) rewindOverlay.hidden = !rev.rewinding;
}
const rewBtn = $('#rewind');
['mousedown', 'touchstart'].forEach((t) => rewBtn?.addEventListener(t, (e) => { e.preventDefault(); setRewind(true); }));
['mouseup', 'mouseleave', 'touchend'].forEach((t) => rewBtn?.addEventListener(t, () => setRewind(false)));

// keyboard
addEventListener('keydown', (e) => {
  if (playerView.hidden) return;
  if (e.code === 'Escape') { backToCatalog(); return; }
  if (e.code === 'KeyR') { setRewind(true); e.preventDefault(); return; }
  if (e.code in KEYMAP) { rev.setButton(KEYMAP[e.code], true); e.preventDefault(); }
});
addEventListener('keyup', (e) => {
  if (e.code === 'KeyR') { setRewind(false); return; }
  if (e.code in KEYMAP) { rev.setButton(KEYMAP[e.code], false); e.preventDefault(); }
});

// touch / on-screen buttons (data-btn = joypad bit)
document.querySelectorAll('[data-btn]').forEach((el) => {
  const bit = +el.dataset.btn;
  const dn = (e) => { e.preventDefault(); rev.setButton(bit, true); el.classList.add('held'); };
  const up = (e) => { e.preventDefault(); rev.setButton(bit, false); el.classList.remove('held'); };
  el.addEventListener('touchstart', dn, { passive: false }); el.addEventListener('touchend', up);
  el.addEventListener('mousedown', dn); el.addEventListener('mouseup', up); el.addEventListener('mouseleave', up);
});

// drag & drop anywhere
['dragover', 'drop'].forEach((t) => addEventListener(t, (e) => {
  e.preventDefault();
  if (t === 'drop' && e.dataTransfer.files[0]) e.dataTransfer.files[0].arrayBuffer()
    .then((b) => playBytes(new Uint8Array(b), e.dataTransfer.files[0].name, 'D-Pad · X/Z · Enter/Shift'));
}));

// build the catalog, render live title-screen thumbnails, honour ?rom= deep links
buildGrid();
(async () => {
  const r = new URLSearchParams(location.search).get('rom');
  if (r) { await boot(); const g = CATALOG.find((x) => x.file === r); playFromUrl(r, g?.title || r, g?.controls || ''); }
  else { await renderThumbs(); }
})();
