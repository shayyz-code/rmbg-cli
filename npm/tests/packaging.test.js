"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const repository = path.resolve(__dirname, "../..");
const platformRoot = path.join(repository, "npm", "platforms");

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: "utf8", ...options });
  assert.equal(result.status, 0, `${command} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

test("native packages and release workflow bundle uv and dry-run explicit tarballs", { timeout: 60_000 }, () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "rmbg-packaging-"));
  const dist = path.join(root, "dist");
  const cache = path.join(root, "npm-cache");
  fs.mkdirSync(dist);
  const env = { ...process.env, npm_config_cache: cache };
  const tarballs = [];

  for (const packageName of fs.readdirSync(platformRoot)) {
    const source = path.join(platformRoot, packageName);
    if (!fs.existsSync(path.join(source, "package.json"))) continue;
    const fixture = path.join(root, packageName);
    fs.mkdirSync(path.join(fixture, "bin"), { recursive: true });
    fs.copyFileSync(path.join(source, "package.json"), path.join(fixture, "package.json"));
    fs.copyFileSync(
      path.join(repository, "THIRD_PARTY_NOTICES.md"),
      path.join(fixture, "THIRD_PARTY_NOTICES.md")
    );
    const windows = packageName.includes("windows");
    fs.writeFileSync(path.join(fixture, "bin", windows ? "rmbg.exe" : "rmbg"), "fixture");
    fs.writeFileSync(path.join(fixture, "bin", windows ? "uv.exe" : "uv"), "fixture");
    const packed = JSON.parse(run("npm", ["pack", fixture, "--json", "--pack-destination", dist], { env }).stdout);
    const tarball = path.join(dist, packed[0].filename);
    tarballs.push(tarball);
    const listing = run("tar", ["-tzf", tarball]).stdout;
    assert.match(listing, new RegExp(`package/bin/rmbg${windows ? "\\.exe" : ""}`));
    assert.match(listing, new RegExp(`package/bin/uv${windows ? "\\.exe" : ""}`));
    assert.match(listing, /package\/THIRD_PARTY_NOTICES\.md/);
  }

  const launcher = JSON.parse(
    run("npm", ["pack", repository, "--json", "--pack-destination", dist], { env }).stdout
  );
  tarballs.push(path.join(dist, launcher[0].filename));
  assert.equal(tarballs.length, 5);

  const workflow = fs.readFileSync(path.join(repository, ".github/workflows/release.yml"), "utf8");
  assert.match(workflow, /UV_VERSION: 0\.11\.26/);
  assert.equal((workflow.match(/npm publish "\.\/dist\//g) || []).length, 5);
});
