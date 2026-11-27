import fs from "node:fs";
import path from "node:path";

const version = process.argv[2];
if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  console.error("usage: node scripts/set-version.mjs <major.minor.patch>");
  process.exit(1);
}

const root = path.resolve(import.meta.dirname, "..");

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function writeJson(file, value) {
  fs.writeFileSync(file, JSON.stringify(value, null, 2) + "\n", "utf8");
}

const packagePath = path.join(root, "package.json");
const packageJson = readJson(packagePath);
packageJson.version = version;
writeJson(packagePath, packageJson);

const lockPath = path.join(root, "package-lock.json");
const lock = readJson(lockPath);
lock.version = version;
if (lock.packages?.[""]) lock.packages[""].version = version;
writeJson(lockPath, lock);

const tauriPath = path.join(root, "src-tauri", "tauri.conf.json");
const tauri = readJson(tauriPath);
tauri.version = version;
writeJson(tauriPath, tauri);

const cargoPath = path.join(root, "src-tauri", "Cargo.toml");
const cargo = fs.readFileSync(cargoPath, "utf8");
const nextCargo = cargo.replace(/^version = "[^"]+"$/m, `version = "${version}"`);
if (nextCargo === cargo) {
  console.error("failed to update Cargo.toml version");
  process.exit(1);
}
fs.writeFileSync(cargoPath, nextCargo, "utf8");

console.log(`app version set to ${version}`);
