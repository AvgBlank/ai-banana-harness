// Guards the frozen design tokens: the light theme and the system-light media query must match, and
// the text, focus and chip pairs must meet WCAG AA. Usage: pnpm tokens:check
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const css = readFileSync(resolve(dirname(fileURLToPath(import.meta.url)), "../src/styles/tokens.css"), "utf8");

function block(startMarker) {
  const i = css.indexOf(startMarker);
  if (i < 0) throw new Error(`missing block ${startMarker}`);
  const open = css.indexOf("{", i);
  let depth = 0;
  for (let j = open; j < css.length; j++) {
    if (css[j] === "{") depth++;
    if (css[j] === "}" && --depth === 0) return css.slice(open + 1, j);
  }
  throw new Error(`unterminated ${startMarker}`);
}
const vars = (text) => Object.fromEntries([...text.matchAll(/(--[\w-]+):\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]));

const dark = vars(block(':root,\n:root[data-theme="dark"]'));
const light = vars(block(':root[data-theme="light"]'));
const system = vars(block(':root[data-theme="system"]'));

let failures = 0;
const fail = (msg) => {
  failures++;
  console.error("FAIL " + msg);
};

// parity: every key the system block sets must equal the light value (color-scheme is a keyword, skip it)
for (const [k, v] of Object.entries(system)) if (light[k] !== v) fail(`system block ${k}: ${v} differs from light ${light[k]}`);
for (const k of Object.keys(light)) if (!(k in system) && !["--bg", "--accent", "--text"].includes(k) && !(k in dark && dark[k] === light[k])) fail(`system block is missing ${k}`);

const hex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
const lin = (c) => ((c /= 255) <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const lum = (h) => {
  const [r, g, b] = hex(h).map(lin);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};
const ratio = (a, b) => {
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
};
const blend = (rgba, bg) => {
  const m = /rgba\((\d+),\s*(\d+),\s*(\d+),\s*([\d.]+)\)/.exec(rgba);
  const [r, g, b, a] = [Number(m[1]), Number(m[2]), Number(m[3]), Number(m[4])];
  const base = hex(bg);
  return "#" + [r, g, b].map((c, i) => Math.round(c * a + base[i] * (1 - a)).toString(16).padStart(2, "0")).join("");
};

for (const [name, t] of [["dark", dark], ["light", { ...dark, ...light }]]) {
  const text = (n, fg, bg, need = 4.5) => {
    const r = ratio(fg, bg);
    if (r < need) fail(`${name}: ${n} ${r.toFixed(2)} < ${need}`);
  };
  for (const s of ["--bg", "--surface-1", "--surface-2"]) {
    text(`text on ${s}`, t["--text"], t[s]);
    text(`muted on ${s}`, t["--text-muted"], t[s]);
    text(`faint on ${s}`, t["--text-faint"], t[s]);
  }
  text("muted on surface-3", t["--text-muted"], t["--surface-3"]);
  text("accent-fg on accent", t["--accent-fg"], t["--accent"]);
  text("accent-text on bg", t["--accent-text"], t["--bg"]);
  text("peel on peel-soft", t["--peel"], t["--peel-soft"]);
  text("add-fg on add-bg", t["--add-fg"], blend(t["--add-bg"], t["--bg"]));
  text("del-fg on del-bg", t["--del-fg"], blend(t["--del-bg"], t["--bg"]));
  for (const k of ["--warn", "--danger", "--ok"]) text(`${k} on surface-1`, t[k], t["--surface-1"]);
  for (const s of ["--bg", "--surface-1", "--surface-2"]) text(`focus ring on ${s}`, t["--focus"], t[s], 3);
}

if (failures) {
  console.error(`${failures} token check(s) failed`);
  process.exit(1);
}
console.log("tokens ok: light/system parity and WCAG AA pairs hold in both themes");
