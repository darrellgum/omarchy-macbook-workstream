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
let src = null;
for (const [, e] of bundles) {
  const b = Buffer.alloc(e.size); fs.readSync(fd, b, 0, e.size, base + Number(e.offset));
  const s = b.toString('utf8'); if (s.includes('black:{lightFrom:"#')) { src = s; break; }
}
if (!src) fail('mark module not found in ' + asarPath);

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
fs.mkdirSync(path.dirname(outPath), { recursive: true });
const tmp = outPath + '.tmp'; fs.writeFileSync(tmp, JSON.stringify(out)); fs.renameSync(tmp, outPath);
console.log('extract-marks: wrote ' + outPath + ' (' + Object.keys(out.shapes).length + ' shapes)');
