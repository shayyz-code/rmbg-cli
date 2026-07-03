#!/usr/bin/env node

"use strict";

const path = require("node:path");
const { spawn } = require("node:child_process");
const { packageForCurrentPlatform } = require("../lib/platform");

let platformPackage;
try {
  platformPackage = packageForCurrentPlatform();
} catch (error) {
  console.error(`rmbg: ${error.message}`);
  process.exit(1);
}

let packageJson;
try {
  packageJson = require.resolve(`${platformPackage}/package.json`);
} catch {
  console.error(
    `rmbg: native package ${platformPackage} is missing. ` +
      "Reinstall rmbg2-cli without omitting optional dependencies."
  );
  process.exit(1);
}

const executable = path.join(
  path.dirname(packageJson),
  "bin",
  process.platform === "win32" ? "rmbg.exe" : "rmbg"
);
const child = spawn(executable, process.argv.slice(2), { stdio: "inherit" });

child.on("error", (error) => {
  console.error(`rmbg: failed to start native executable: ${error.message}`);
  process.exit(1);
});

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => child.kill(signal));
}

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});
