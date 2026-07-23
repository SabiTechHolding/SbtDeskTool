import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

// Normalize junctions/symlinks so Vite does not see the HTML entry and output
// directory as belonging to two different project roots.
const projectRoot = fs.realpathSync(process.cwd());
const stageDirectory = path.join(
  projectRoot,
  "tmp",
  `exe-${Date.now()}-${process.pid}`,
);
const frontendDirectory = path.join(stageDirectory, "frontend");
const overridePath = path.join(stageDirectory, "tauri.override.json");

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: projectRoot,
    stdio: "inherit",
    shell: false,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${command} exited with code ${result.status ?? 1}`);
  }
}

function runNpm(args) {
  if (process.platform === "win32") {
    run(process.env.ComSpec || "cmd.exe", ["/d", "/s", "/c", "npm.cmd", ...args]);
  } else {
    run("npm", args);
  }
}

fs.mkdirSync(stageDirectory, { recursive: true });
try {
  runNpm(["run", "build", "--", "--outDir", frontendDirectory]);
  const frontendFromNativeConfig = path
    .relative(path.join(projectRoot, "src-tauri"), frontendDirectory)
    .replaceAll("\\", "/");
  fs.writeFileSync(
    overridePath,
    `${JSON.stringify(
      {
        build: {
          frontendDist: frontendFromNativeConfig,
          beforeBuildCommand: "",
        },
        bundle: {
          active: false,
          createUpdaterArtifacts: false,
        },
      },
      null,
      2,
    )}\n`,
  );
  runNpm(["run", "tauri", "--", "build", "--config", overridePath, "--no-bundle"]);
} finally {
  fs.rmSync(stageDirectory, { recursive: true, force: true });
}
