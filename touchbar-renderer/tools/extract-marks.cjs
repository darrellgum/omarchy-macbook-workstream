#!/usr/bin/env node
// Extract Grok Bot avatar mark geometry from the locally installed app into a JSON cache for t1-dash.
// Nothing from the app is redistributed: this runs on the user's machine against their own install.
// usage: extract-marks.cjs [app.asar] [out.json]
// Runs under node, or under the app's own Electron binary with ELECTRON_RUN_AS_NODE=1.
'use strict';
process.noAsar = true; // under Electron, read the archive as a plain file
const fs = require('fs'), path = require('path'), os = require('os');
const asarPath = process.argv[2] || '/opt/Grok Bot/resources/app.asar';
const outPath = process.argv[3] || path.join(process.env.XDG_CACHE_HOME || path.join(os.homedir(), '.cache'), 't1-dash', 'marks.json');
const SHAPES = ['blob','pebble','bean','egg','squircle','tablet','capsule','cylinder','hex','gem','crystal','wedge','shield','dome','arch','cloud','teardrop','leaf'];
const POSES = { neutral: null, tilt_l: { turn: 9, tilt: -8, roll: 22 }, tilt_r: { turn: 27, tilt: -20, roll: 36 }, glance_l: { turn: 5, tilt: -14, roll: 29 }, glance_r: { turn: 31, tilt: -14, roll: 29 } };
const fail = (m) => { console.error('extract-marks: ' + m); process.exit(1); };

// --- read JS bundles out of the asar archive (Chromium pickle header + JSON index) ---
const fd = fs.openSync(asarPath, 'r');
const head = Buffer.alloc(16); fs.readSync(fd, head, 0, 16, 0);
const headerSize = head.readUInt32LE(4), jsonLen = head.readUInt32LE(12);
const hb = Buffer.alloc(jsonLen); fs.readSync(fd, hb, 0, jsonLen, 16);
const index = JSON.parse(hb.toString('utf8')); const base = 8 + headerSize;
const bundles = [];
(function walk(node, p) {
  for (const [name, e] of Object.entries(node.files || {})) {
    const q = p ? p + '/' + name : name;
    if (e.files) walk(e, q);
    else if (/\.js$/.test(name) && !e.unpacked && e.size > 100000 && q.includes('renderer')) bundles.push([q, e]);
  }
})(index, '');
const readEntry = (e) => { const b = Buffer.alloc(e.size); fs.readSync(fd, b, 0, e.size, base + Number(e.offset)); return b.toString('utf8'); };
const write = (out) => {
  fs.mkdirSync(path.dirname(outPath), { recursive: true });
  const tmp = outPath + '.tmp'; fs.writeFileSync(tmp, JSON.stringify(out)); fs.renameSync(tmp, outPath);
  console.log('extract-marks: wrote ' + outPath + ' (' + Object.keys(out.shapes).length + ' shapes, format ' + (out.format || 1) + ')');
};
(function main() {
let src = null;
for (const [, e] of bundles) {
  const s = readEntry(e); if (s.includes('black:{lightFrom:"#') && s.includes('radial-gradient(115% 90%')) { src = s; break; }
}
if (!src) { motionFormat().catch((e) => fail(e.message)); return; }

// --- cut the geometry module: from the color table to the end of the gradient helper ---
const anchor = src.indexOf('black:{lightFrom:"#');
const start = src.lastIndexOf('const ', anchor);
const tail = src.indexOf('radial-gradient(115% 90%', anchor);
if (start < 0 || tail < 0) fail('mark module layout not recognized; this Grok Bot version may use a newer avatar format');
const compiles = (code) => { try { new Function(code); return true; } catch { return false; } };
let slice = null;
for (let i = tail; i < tail + 4000 && i < src.length; i++) {
  if (src[i] === '}' && compiles(src.slice(start, i + 1))) { slice = src.slice(start, i + 1); break; }
}
if (!slice) fail('could not delimit module');

// --- evaluate it with free identifiers stubbed, and capture every binding it declares ---
const names = [...new Set(slice.match(/[A-Za-z_$][\w$]*/g))].filter((n) => /^[\w$]{1,4}$/.test(n));
const scope = new Proxy({}, { // free names resolve to the shape list; the module's own declarations still shadow this
  has: (_, k) => typeof k === 'string' && !(k in globalThis), get: () => SHAPES });
let vals;
try {
  vals = new Function('scope', 'with(scope){' + slice + ';\nreturn (function(){const o={};for(const n of ' + JSON.stringify(names) + '){try{o[n]=eval(n)}catch(e){}}return o})()}')(scope);
} catch (e) { fail('evaluating module: ' + e.message); }
const all = Object.values(vals);
const pick = (f) => all.find((v) => { try { return f(v); } catch { return false; } });
const Yo = pick((v) => v && typeof v === 'object' && typeof v.blob?.path === 'string' && typeof v.leaf?.path === 'string');
const colors = pick((v) => v && typeof v === 'object' && typeof v.black?.light === 'string' && typeof v.black?.dark === 'string');
const TA = pick((v) => typeof v === 'function' && typeof v('blob')?.scale === 'number' && 'pose' in v('blob'));
const td = pick((v) => Array.isArray(v) && Array.isArray(v[0]) && v[0].length === 2 && typeof v[0][0] === 'object');
if (!Yo || !colors || !TA || !td) fail('geometry pieces not recognized (shapes ' + !!Yo + ', colors ' + !!colors + ', tune ' + !!TA + ', eyes ' + !!td + ')');
const fns = all.filter((v) => typeof v === 'function');
const ta0 = TA('blob');
const eyesFn = fns.find((f) => { try { const r = f('blob', ta0); return Array.isArray(r) && r.length === 2 && typeof r[0] === 'object'; } catch { return false; } });
if (!eyesFn) fail('eye placement not recognized');
const [c0, u0] = eyesFn('blob', ta0);
// the eye path builder is the one whose output depends on the eye placement (left vs right differ)
const pathFn = fns.find((f) => { try { const s = f(td[0][0], c0); return typeof s === 'string' && /^M/.test(s.trim()) && s !== f(td[0][0], u0); } catch { return false; } });
if (!pathFn) fail('eye path not recognized');
const center = all.find((v) => typeof v === 'number' && v > 100 && v < 130 && !Number.isInteger(v)) ?? 114.2705;

const out = { center, viewbox: [-15, -15, 259, 259], colors: {}, shapes: {} };
for (const c of Object.keys(colors)) out.colors[c] = colors[c];
for (const s of SHAPES) {
  if (!Yo[s]) continue;
  const ta = TA(s); const e = { scale: ta.scale, body: Yo[s].path, eyes: {} };
  for (const [n, p] of Object.entries(POSES)) {
    const o = p ? { ...ta, pose: p } : ta; const [c, u] = eyesFn(s, o); const [l, r] = td[0];
    e.eyes[n] = [pathFn(l, c), pathFn(r, u)];
  }
  out.shapes[s] = e;
}
if (Object.keys(out.shapes).length < 10) fail('too few shapes extracted');
write(out);

})();

// ---------- newer app versions: "motion" marks (keyframed 3D bodies and eyes, split ES-module bundle) ----------
// The app's own player is run headlessly and the 2D outlines it produces are sampled per frame, so the
// bar shows the same idle and working motion as the app. Pieces are located by code shape, not names.
async function motionFormat() {
  const assets = [];
  (function walk(node, p) {
    for (const [name, e] of Object.entries(node.files || {})) {
      const q = p ? p + '/' + name : name;
      if (e.files) walk(e, q); else if (/\.js$/.test(name) && !e.unpacked && q.includes('renderer/assets/')) assets.push([name, e]);
    }
  })(index, '');
  const tmpdir = fs.mkdtempSync(path.join(os.tmpdir(), 't1-dash-marks-'));
  try {
    let chunk = null;
    for (const [name, e] of assets) {
      const code = readEntry(e).replace(/(from\s*|import\()"\.\/([^"]+)\.js"/g, '$1"./$2.mjs"');
      fs.writeFileSync(path.join(tmpdir, name.replace(/\.js$/, '.mjs')), code);
      if (code.includes('case"blob":return"sphere"')) chunk = name.replace(/\.js$/, '.mjs');
    }
    if (!chunk) throw new Error('mark module not found in ' + asarPath + ' (unknown Grok Bot avatar format)');
    const code = fs.readFileSync(path.join(tmpdir, chunk), 'utf8');
    const grab = (re, what) => { const m = code.match(re); if (!m) throw new Error('motion marks: ' + what + ' not recognized'); return m[1]; };
    const player = grab(/\.current\?\?=([\w$]+)\(\{body:[\w$]+\}\)/, 'player');
    const machine = grab(/\.current=([\w$]+)\([\w$]+,[\w$]+,\{seed:/, 'state machine');
    const outline = grab(/for\(const [\w$]+ of ([\w$]+)\([\w$]+\([\w$]+\),[\w$]+,[\w$]+\)\.body\)/, 'outline');
    const mapper = grab(/function ([\w$]+)\([\w$]+\)\{switch\([\w$]+\)\{case"blob":return"sphere"/, 'body mapper');
    const shapeList = JSON.parse(grab(/(\["blob","pebble"[^\]]*\])/, 'shape list'));
    const colors = new Function('return ' + grab(/=(\{black:\{light:"#[0-9A-Fa-f]{6}",dark:"#[0-9A-Fa-f]{6}"\}[^;]*?\}\})/, 'colors'))();
    fs.writeFileSync(path.join(tmpdir, 'x.mjs'), code + `\nexport{${player} as __player,${machine} as __machine,${outline} as __outline,${mapper} as __mapper};`);
    browserStubs();
    const m = await import(path.join(tmpdir, 'x.mjs'));
    const SIZE = 100, FPS = 15, SUB = 2; // simulate at 30 fps, keep every 2nd frame
    const resample = (ring, n) => Array.from({ length: n }, (_, i) => {
      const r = i / n * ring.length, k = Math.floor(r), f = r - k, a = ring[k % ring.length], b = ring[(k + 1) % ring.length];
      const p = Array.isArray(a) ? a : [a.x, a.y], q = Array.isArray(b) ? b : [b.x, b.y];
      return [Math.round((p[0] + (q[0] - p[0]) * f) * 10), Math.round((p[1] + (q[1] - p[1]) * f) * 10)];
    }).flat();
    const frames = [], seen = new Map();
    const bake = (shape, state, seconds) => {
      const body = m.__mapper(shape), O = m.__player({ body }), R = m.__machine(O, state, { seed: 1, isCompact: true, isResting: false });
      const idx = [];
      for (let i = 0; i < seconds * FPS * SUB; i++) {
        R.advance(1 / (FPS * SUB)); O.advance(1 / (FPS * SUB));
        if (i % SUB) continue;
        const o = m.__outline(O.frame(), body, SIZE);
        const fr = { b: o.body.map((r) => resample(r, 72)), e: o.eyes.map((r) => resample(r, 28)) };
        const key = JSON.stringify(fr);
        if (!seen.has(key)) { seen.set(key, frames.length); frames.push(fr); }
        idx.push(seen.get(key));
      }
      return idx;
    };
    const out = { format: 2, size: SIZE, fps: FPS, scale: 10, colors, shapes: {}, frames };
    for (const s of shapeList) out.shapes[s] = { idle: bake(s, 'idle', 4), working: bake(s, 'working', 16) };
    write(out);
  } finally { fs.rmSync(tmpdir, { recursive: true, force: true }); }
}

function browserStubs() {
  const noop = () => {};
  const g = globalThis;
  const store = { getItem: () => null, setItem: noop, removeItem: noop };
  Object.assign(g, {
    window: g, self: g, addEventListener: noop, removeEventListener: noop,
    matchMedia: () => ({ matches: false, addEventListener: noop, removeEventListener: noop, addListener: noop }),
    localStorage: store, sessionStorage: store, requestAnimationFrame: noop, cancelAnimationFrame: noop,
    location: { href: 'file:///', protocol: 'file:', search: '', hash: '', pathname: '/' },
    HTMLElement: class {}, Element: class {}, Node: class {},
    MutationObserver: class { observe() {} }, ResizeObserver: class { observe() {} }, IntersectionObserver: class { observe() {} },
    getComputedStyle: () => ({ getPropertyValue: () => '' }), CSS: { supports: () => false },
  });
  g.document = { createElement: () => ({ style: {}, setAttribute: noop, appendChild: noop }), addEventListener: noop,
    documentElement: { style: {} }, head: { appendChild: noop }, querySelector: () => null, querySelectorAll: () => [], hidden: false };
}
