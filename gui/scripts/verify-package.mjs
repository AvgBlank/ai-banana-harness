// Checks the finished package: the sidecar is in the bundle and runs, the asar holds no
// node_modules, the disk image verifies, and the version is the crate's. Exits non-zero on the
// first problem. Run from gui/ after electron-builder.
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const crate = /^version\s*=\s*"([^"]+)"/m.exec(
  readFileSync(join(root, "../crates/banana-harness/Cargo.toml"), "utf8"),
)?.[1];

const dist = join(root, "dist");
const app = join(dist, "mac-arm64", `${pkg.productName}.app`);
const dmg = join(dist, `${pkg.productName}-${pkg.version}-arm64.dmg`);
const zip = join(dist, `${pkg.productName}-${pkg.version}-arm64-mac.zip`);
const sidecar = join(app, "Contents", "Resources", "bin", "banana-harness");

const fail = (message) => {
  console.error(`verify-package: ${message}`);
  process.exit(1);
};
const run = (command, args) => execFileSync(command, args, { encoding: "utf8" });

for (const file of [dmg, zip, sidecar]) if (!existsSync(file)) fail(`${file} is missing`);
if (crate !== pkg.version)
  fail(`app version ${pkg.version} differs from the crate version ${crate}`);

const mode = statSync(sidecar).mode & 0o777;
if (mode !== 0o755) fail(`the sidecar has mode ${mode.toString(8)}, not 755`);
const reported = run(sidecar, ["--version"]).trim();
if (!reported.includes(crate)) fail(`the sidecar reports "${reported}", not ${crate}`);

// A release must not carry the build machine's paths (cargo records source paths in the binary).
const bytes = readFileSync(sidecar);
for (const path of [process.env.HOME, resolve(root, "..")]) {
  if (path && path !== "/" && bytes.includes(Buffer.from(path)))
    fail(`the sidecar contains the build path ${path}`);
}

// The renderer and main are bundled, so the archive carries no dependencies.
const asar = join(app, "Contents", "Resources", "app.asar");
const listing = run(join(root, "node_modules", ".bin", "asar"), ["list", asar]);
if (/node_modules/.test(listing)) fail("app.asar lists node_modules");

const verified = run("hdiutil", ["verify", dmg]);
if (!/checksum .* is VALID|verified|VALID/i.test(verified)) fail(`hdiutil verify: ${verified}`);

console.log(
  `package ok: ${pkg.productName} ${pkg.version}, sidecar "${reported}" (${statSync(sidecar).size} bytes), ` +
    `${statSync(dmg).size} byte DMG, ${statSync(zip).size} byte ZIP`,
);
