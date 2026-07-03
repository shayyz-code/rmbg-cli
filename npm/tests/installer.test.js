"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const repository = path.resolve(__dirname, "../..");

function writeExecutable(file, contents) {
  fs.writeFileSync(file, contents, { mode: 0o755 });
}

function installerFixture(validChecksum) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "rmbg-installer-test-"));
  const fakeBin = path.join(root, "fake-bin");
  const payload = path.join(root, "payload");
  const destination = path.join(root, "destination");
  fs.mkdirSync(fakeBin);
  fs.mkdirSync(payload);

  writeExecutable(path.join(payload, "rmbg"), "#!/bin/sh\necho fixture-rmbg\n");
  const archiveName = "rmbg-x86_64-unknown-linux-gnu.tar.gz";
  const archive = path.join(root, archiveName);
  const tar = spawnSync("tar", ["-czf", archive, "-C", payload, "rmbg"]);
  assert.equal(tar.status, 0, tar.stderr?.toString());

  const actual = crypto.createHash("sha256").update(fs.readFileSync(archive)).digest("hex");
  const checksum = validChecksum ? actual : "0".repeat(64);
  const checksums = path.join(root, "SHA256SUMS");
  fs.writeFileSync(checksums, `${checksum}  ${archiveName}\n`);

  writeExecutable(
    path.join(fakeBin, "uname"),
    '#!/bin/sh\ncase "$1" in -s) echo Linux ;; -m) echo x86_64 ;; *) echo Linux ;; esac\n'
  );
  writeExecutable(path.join(fakeBin, "getconf"), "#!/bin/sh\nexit 0\n");
  writeExecutable(
    path.join(fakeBin, "curl"),
    `#!/bin/sh
out=""
url=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-o" ]; then shift; out="$1"; else url="$1"; fi
  shift
done
case "$url" in
  */SHA256SUMS) cp "$FIXTURE_CHECKSUMS" "$out" ;;
  *) cp "$FIXTURE_ARCHIVE" "$out" ;;
esac
`
  );

  const result = spawnSync("sh", [path.join(repository, "install.sh")], {
    encoding: "utf8",
    env: {
      ...process.env,
      PATH: `${fakeBin}:${process.env.PATH}`,
      FIXTURE_ARCHIVE: archive,
      FIXTURE_CHECKSUMS: checksums,
      RMBG_INSTALL_DIR: destination,
      RMBG_VERSION: "0.4.0"
    }
  });

  return { destination, result };
}

test("Unix installer verifies and installs the selected archive", { skip: process.platform === "win32" }, () => {
  const { destination, result } = installerFixture(true);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(fs.existsSync(path.join(destination, "rmbg")), true);
  assert.match(result.stdout, /Run 'rmbg setup'/);
});

test("Unix installer rejects a checksum mismatch", { skip: process.platform === "win32" }, () => {
  const { destination, result } = installerFixture(false);
  assert.notEqual(result.status, 0);
  assert.equal(fs.existsSync(path.join(destination, "rmbg")), false);
  assert.match(result.stderr, /checksum verification failed/);
});
