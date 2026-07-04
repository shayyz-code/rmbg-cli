"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const { packageForPlatform, usesGlibc } = require("../lib/platform");

const glibc = { getReport: () => ({ header: { glibcVersionRuntime: "2.39" } }) };
const musl = { getReport: () => ({ header: {} }) };

test("selects every supported native package", () => {
  assert.equal(packageForPlatform("linux", "x64", glibc), "rmbg2-cli-linux-x64-gnu");
  assert.equal(packageForPlatform("linux", "arm64", glibc), "rmbg2-cli-linux-arm64-gnu");
  assert.equal(packageForPlatform("darwin", "arm64"), "rmbg2-cli-darwin-arm64");
  assert.equal(packageForPlatform("win32", "x64"), "rmbg2-cli-windows-x64");
});

test("rejects unsupported operating systems and architectures", () => {
  assert.throws(() => packageForPlatform("darwin", "x64"), /unsupported platform/);
  assert.throws(() => packageForPlatform("win32", "arm64"), /unsupported platform/);
});

test("rejects Linux musl", () => {
  assert.equal(usesGlibc(glibc), true);
  assert.equal(usesGlibc(musl), false);
  assert.throws(() => packageForPlatform("linux", "x64", musl), /musl is not supported/);
});
