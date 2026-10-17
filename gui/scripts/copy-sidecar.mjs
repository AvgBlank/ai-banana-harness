// Stages the release sidecar for the package: gui/build/sidecar/banana-harness, mode 755.
// Usage: node scripts/copy-sidecar.mjs <path to the release banana-harness>
import { chmodSync, copyFileSync, mkdirSync, rmSync, statSync } from "node:fs";
import { dirname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source = resolve(process.argv[2] ?? "");

const refuse = (message) => {
  console.error(`copy-sidecar: ${message}`);
  process.exit(1);
};

if (!process.argv[2]) refuse("usage: copy-sidecar.mjs <path to the release banana-harness>");
let info;
try {
  info = statSync(source);
} catch {
  refuse(
    `${source} does not exist; build it with 'cargo build --release --locked --bin banana-harness'`,
  );
}
if (!info.isFile()) refuse(`${source} is not a file`);
// A debug build is large, slow and carries debug assertions: never package one.
if (source.split(sep).includes("debug"))
  refuse(`${source} is a debug build; package the release binary`);

const target = join(root, "build", "sidecar");
rmSync(target, { recursive: true, force: true });
mkdirSync(target, { recursive: true });
copyFileSync(source, join(target, "banana-harness"));
chmodSync(join(target, "banana-harness"), 0o755);
console.log(`staged ${source} -> ${join(target, "banana-harness")}`);
