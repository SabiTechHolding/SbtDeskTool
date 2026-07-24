import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const projectRoot = process.cwd();
const defaultBundles = process.platform === "win32"
  ? "nsis"
  : process.platform === "darwin"
    ? "app,dmg"
    : "appimage,deb,rpm";
const bundleIndex = process.argv.indexOf("--bundles");
const bundles = bundleIndex >= 0 ? process.argv[bundleIndex + 1] : defaultBundles;

if (!bundles) throw new Error("--bundles requires a comma-separated value");

const stageName = `package-${Date.now()}-${process.pid}`;
const stageDirectory = path.join(projectRoot, "tmp", stageName);
const frontendDirectory = path.join(stageDirectory, "frontend");
const overridePath = path.join(stageDirectory, "tauri.override.json");
const cargoTargetDirectory = process.env.CARGO_TARGET_DIR
  ? path.resolve(projectRoot, process.env.CARGO_TARGET_DIR)
  : path.join(projectRoot, "target");
process.env.CARGO_TARGET_DIR = cargoTargetDirectory;
if (process.platform === "win32" && process.env.USERPROFILE) {
  const userCargoBin = path.join(process.env.USERPROFILE, ".cargo", "bin");
  if (fs.existsSync(path.join(userCargoBin, "cargo.exe"))) {
    process.env.PATH = `${userCargoBin}${path.delimiter}${process.env.PATH ?? ""}`;
  }
}
fs.mkdirSync(stageDirectory, { recursive: true });

// A build directory may contain installers from older versions. Clear only
// generated bundle output so every successful run has an unambiguous result.
fs.rmSync(path.join(cargoTargetDirectory, "release", "bundle"), {
  recursive: true,
  force: true,
});

function run(command, args) {
  const result = spawnSync(command, args, { cwd: projectRoot, stdio: "inherit", shell: false });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} exited with code ${result.status ?? 1}`);
}

function runNpm(args) {
  if (process.platform === "win32") {
    run(process.env.ComSpec || "cmd.exe", ["/d", "/s", "/c", "npm.cmd", ...args]);
  } else {
    run("npm", args);
  }
}

try {
  runNpm(["run", "build", "--", "--outDir", frontendDirectory]);
  runNpm(["run", "test:rust"]);

  const frontendFromNativeConfig = path.relative(path.join(projectRoot, "src-tauri"), frontendDirectory).replaceAll("\\", "/");
  const signingEnabled = Boolean(process.env.TAURI_SIGNING_PRIVATE_KEY?.trim());
  fs.writeFileSync(overridePath, JSON.stringify({
    build: {
      frontendDist: frontendFromNativeConfig,
      beforeBuildCommand: "",
    },
    bundle: {
      createUpdaterArtifacts: signingEnabled,
    },
  }, null, 2));

  runNpm(["run", "tauri", "--", "build", "--config", overridePath, "--bundles", bundles]);
  console.log(`Packages completed for ${process.platform}: ${bundles}`);
} finally {
  fs.rmSync(stageDirectory, { recursive: true, force: true });
}
