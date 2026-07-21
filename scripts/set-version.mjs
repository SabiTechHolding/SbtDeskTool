import fs from "node:fs";

const input = process.argv[2]?.trim().replace(/^v/i, "");
if (!input) {
  throw new Error("Usage: node scripts/set-version.mjs <version>");
}

// Release/display versions use 1.YY.M.D-BUILD. Cargo, npm and Tauri require
// SemVer, so store the equivalent 1.YY.M-D.BUILD internally.
const calendar = input.match(/^(\d+)\.(\d+)\.0?(\d{1,2})\.0?(\d{1,2})-(\d+)$/);
const version = calendar
  ? `${Number(calendar[1])}.${Number(calendar[2])}.${Number(calendar[3])}-${Number(calendar[4])}.${Number(calendar[5])}`
  : input;

if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
  throw new Error(`Invalid semantic version: ${input}`);
}

if (calendar && fs.existsSync("version.json")) {
  const versionConfig = JSON.parse(fs.readFileSync("version.json", "utf8"));
  versionConfig.major = Number(calendar[1]);
  versionConfig.build = Number(calendar[5]);
  fs.writeFileSync("version.json", `${JSON.stringify(versionConfig, null, 2)}\n`);
}

const packagePath = "package.json";
const packageJson = JSON.parse(fs.readFileSync(packagePath, "utf8"));
packageJson.version = version;
fs.writeFileSync(packagePath, `${JSON.stringify(packageJson, null, 2)}\n`);

const lockPath = "package-lock.json";
const packageLock = JSON.parse(fs.readFileSync(lockPath, "utf8"));
packageLock.version = version;
if (packageLock.packages?.[""]) packageLock.packages[""].version = version;
fs.writeFileSync(lockPath, `${JSON.stringify(packageLock, null, 2)}\n`);

const cargoPath = "src-tauri/Cargo.toml";
const cargo = fs.readFileSync(cargoPath, "utf8").replace(
  /^(\[package\][\s\S]*?^version\s*=\s*)"[^"]+"/m,
  `$1"${version}"`,
);
fs.writeFileSync(cargoPath, cargo);

const cargoLockPath = "src-tauri/Cargo.lock";
const cargoLock = fs.readFileSync(cargoLockPath, "utf8").replace(
  /(\[\[package\]\]\r?\nname = "sbt-desk-tool"\r?\nversion = ")[^"]+"/,
  `$1${version}"`,
);
fs.writeFileSync(cargoLockPath, cargoLock);

const tauriPath = "src-tauri/tauri.conf.json";
const tauri = JSON.parse(fs.readFileSync(tauriPath, "utf8"));
tauri.version = version;
fs.writeFileSync(tauriPath, `${JSON.stringify(tauri, null, 2)}\n`);

process.stdout.write(version);
