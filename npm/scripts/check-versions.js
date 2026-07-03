"use strict";

const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "../..");
const rootPackage = require(path.join(root, "package.json"));
const cargo = fs.readFileSync(path.join(root, "Cargo.toml"), "utf8");
const cargoVersion = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const runtime = fs.readFileSync(path.join(root, "runtime", "pyproject.toml"), "utf8");
const runtimeVersion = runtime.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const expected = rootPackage.version;

if (cargoVersion !== expected) {
  throw new Error(`Cargo version ${cargoVersion} does not match npm version ${expected}`);
}
if (runtimeVersion !== expected) {
  throw new Error(`Runtime version ${runtimeVersion} does not match npm version ${expected}`);
}

for (const dependency of Object.keys(rootPackage.optionalDependencies)) {
  if (rootPackage.optionalDependencies[dependency] !== expected) {
    throw new Error(`${dependency} is not pinned to ${expected}`);
  }

  const packageJson = path.join(root, "npm", "platforms", dependency, "package.json");
  const platformPackage = JSON.parse(fs.readFileSync(packageJson, "utf8"));
  if (platformPackage.name !== dependency || platformPackage.version !== expected) {
    throw new Error(`${dependency} metadata does not match ${expected}`);
  }
}

console.log(`Cargo, runtime, and npm package versions match: ${expected}`);
