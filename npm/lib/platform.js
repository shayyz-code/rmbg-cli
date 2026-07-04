"use strict";

const PACKAGES = new Map([
  ["darwin-arm64", "rmbg2-cli-darwin-arm64"],
  ["linux-arm64", "rmbg2-cli-linux-arm64-gnu"],
  ["linux-x64", "rmbg2-cli-linux-x64-gnu"],
  ["win32-x64", "rmbg2-cli-windows-x64"]
]);

function usesGlibc(report = process.report) {
  if (!report || typeof report.getReport !== "function") return false;
  return Boolean(report.getReport().header?.glibcVersionRuntime);
}

function packageForPlatform(platform, arch, report = process.report) {
  if (platform === "linux" && !usesGlibc(report)) {
    throw new Error("Linux musl is not supported; a glibc-based distribution is required");
  }

  const packageName = PACKAGES.get(`${platform}-${arch}`);
  if (!packageName) {
    throw new Error(
      `unsupported platform ${platform}/${arch}; supported targets are Linux x64/ARM64 (glibc), macOS ARM64, and Windows x64`
    );
  }
  return packageName;
}

function packageForCurrentPlatform() {
  return packageForPlatform(process.platform, process.arch);
}

module.exports = { packageForCurrentPlatform, packageForPlatform, usesGlibc };
