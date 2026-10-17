// The app version has one owner, gui/package.json, and must equal the sidecar's crate version.
// The bundle identifier appears in package.json and electron-builder.yml and must match.
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const pkg = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8"));
const cargo = readFileSync(resolve(root, "../crates/banana-harness/Cargo.toml"), "utf8");
const builder = readFileSync(resolve(root, "electron-builder.yml"), "utf8");

const crate = /^version\s*=\s*"([^"]+)"/m.exec(cargo)?.[1];
const builderAppId = /^appId:\s*(\S+)/m.exec(builder)?.[1];

let failures = 0;
const fail = (message) => {
  failures++;
  console.error(`FAIL ${message}`);
};
if (!crate) fail("no version found in crates/banana-harness/Cargo.toml");
else if (pkg.version !== crate)
  fail(`app version ${pkg.version} differs from the banana-harness crate version ${crate}`);
if (builderAppId !== pkg.appId)
  fail(`electron-builder.yml appId ${builderAppId} differs from package.json appId ${pkg.appId}`);
if (failures) process.exit(1);
console.log(`versions ok (${pkg.version}, ${pkg.appId})`);
