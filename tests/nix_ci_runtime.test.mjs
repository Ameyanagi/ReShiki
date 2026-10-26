import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import https from "node:https";
import { EventEmitter } from "node:events";
import { Readable } from "node:stream";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import {
  selectPlatform,
  assertEnvironment,
  checkHash,
  embeddedSlice,
  validateMembers,
  validateObservation,
  validateStableObservation,
  parseService,
  downloadAsset,
  verifyFiles,
  removeStaging,
} from "../scripts/nix_ci_runtime.mjs";

const hash = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");

test("explicit hosted platform metadata matches the actual Node process architecture", () => {
  assert.equal(selectPlatform("github-hosted", "Linux", "X64", "linux", "x64"), "x86_64-linux");
  assert.equal(
    selectPlatform("github-hosted", "Linux", "ARM64", "linux", "arm64"),
    "aarch64-linux",
  );
  assert.equal(
    selectPlatform("github-hosted", "macOS", "ARM64", "darwin", "arm64"),
    "aarch64-darwin",
  );
  for (const args of [
    ["self-hosted", "Linux", "X64", "linux", "x64"],
    ["github-hosted", "macOS", "X64", "darwin", "x64"],
    ["github-hosted", "Windows", "ARM64", "win32", "arm64"],
    ["github-hosted", "Linux", "ARM64", "linux", "x64"],
  ]) {
    assert.throws(() => selectPlatform(...args));
  }
});

test("reject inherited store, daemon, loader and installer overrides before execution", () => {
  for (const name of [
    "NIX_CONFIG",
    "NIX_REMOTE",
    "NIX_DAEMON_SOCKET_PATH",
    "NIX_SSL_CERT_FILE",
    "NIX_INSTALLER_NO_CHANNEL_ADD",
    "BASH_ENV",
    "ENV",
    "LD_PRELOAD",
    "DYLD_LIBRARY_PATH",
    "NODE_OPTIONS",
  ]) {
    assert.throws(() => assertEnvironment({ [name]: "unsafe" }));
  }
  assertEnvironment({ PATH: "/usr/bin", HOME: "/owned", NIX_CONFIG: "" });
});

test("whole asset authentication precedes bounded fixed-slice authentication", () => {
  const bytes = Buffer.from("outer|authenticated-xz|tail");
  const policy = {
    asset_sha256: hash(bytes),
    asset_size: bytes.length,
    slice_offset: 6,
    slice_length: 16,
    slice_sha256: hash(bytes.subarray(6, 22)),
  };
  assert.deepEqual(embeddedSlice(bytes, policy), bytes.subarray(6, 22));
  for (const change of [
    { asset_sha256: "0".repeat(64) },
    { asset_size: 1 },
    { slice_offset: -1 },
    { slice_offset: 0.5 },
    { slice_length: 999 },
    { slice_length: 0 },
    { slice_sha256: "0".repeat(64) },
  ]) {
    assert.throws(() => embeddedSlice(bytes, { ...policy, ...change }));
  }
  assert.throws(() =>
    checkHash(Buffer.from("replaced service/library"), hash(Buffer.from("approved"))),
  );
});

test("archive list rejects escape/duplicate/missing members before extraction", () => {
  const valid = "root/\nroot/install\nroot/install-multi-user\nroot/.reginfo\nroot/store/\n";
  validateMembers(valid, "root");
  for (const addition of [
    "../escape",
    "/absolute",
    "root/../escape",
    "other/file",
    "root\\file",
    "root/install",
  ]) {
    assert.throws(() => validateMembers(`${valid}${addition}\n`, "root"));
  }
  assert.throws(() => validateMembers("root/install\n", "root"));
});

const policy = {
  nix: "/nix/store/new-nix/bin/nix",
  store_roots: [
    "/nix/store/new-nix",
    "/nix/store/new-curl-8.23",
    "/nix/store/new-brotli-1.2.1",
    "/nix/store/new-openssl-3.6.5",
  ],
  critical_files: {
    "/nix/store/new-curl-8.23/lib/libcurl.so": "hash",
    "/nix/store/new-brotli-1.2.1/lib/libbrotlidec.so": "hash",
    "/nix/store/new-openssl-3.6.5/lib/libcrypto.so": "hash",
  },
};
const observation = {
  pid: 123,
  uid: 0,
  executable: policy.nix,
  socketOwned: true,
  loaded: Object.keys(policy.critical_files),
};

test("live daemon identity includes root PID, exact executable, socket and loaded security closure", () => {
  validateObservation(observation, policy);
  for (const change of [
    { pid: 0 },
    { pid: 2.5 },
    { uid: 1000 },
    { socketOwned: false },
    { executable: "/usr/local/bin/determinate-nixd" },
    { loaded: observation.loaded.slice(1) },
    { loaded: [...observation.loaded, "/nix/store/old-brotli-1.2.0/lib/libbrotli.so"] },
  ]) {
    assert.throws(() => validateObservation({ ...observation, ...change }, policy));
  }
  validateStableObservation(observation, { ...observation }, policy);
  assert.throws(() => validateStableObservation(observation, { ...observation, pid: 124 }, policy));
});

test("systemd/launchd PID fixtures reject inactive, overridden, malformed and ambiguous services", () => {
  assert.equal(parseService("MainPID=123\nDropInPaths=\nActiveState=active\n", true), 123);
  for (const text of [
    "MainPID=123\nDropInPaths=/override\nActiveState=active",
    "MainPID=123\nDropInPaths=\nActiveState=failed",
    "MainPID=123\nMainPID=456\nDropInPaths=\nActiveState=active",
    "MainPID=no\nDropInPaths=\nActiveState=active",
  ]) {
    assert.throws(() => parseService(text, true));
  }
  assert.equal(parseService("system/org.nixos.nix-daemon = {\n\tpid = 123\n}\n", false), 123);
  assert.throws(() => parseService("pid = 123\npid = 456", false));
  assert.throws(() => parseService("state = waiting", false));
});

test("download fails closed on network, redirect, encoding, length and deadline errors", async (t) => {
  const url = "https://github.com/DeterminateSystems/nix-installer/releases/download/v3.24.1/test";
  const responses = [];
  t.mock.method(https, "get", (_url, options, callback) => {
    assert.notEqual(options.rejectUnauthorized, false);
    assert.equal(options.headers["Accept-Encoding"], "identity");
    const request = new EventEmitter();
    const next = responses.shift();
    if (next?.error) queueMicrotask(() => request.emit("error", Error(next.error)));
    else if (next)
      queueMicrotask(() => {
        const response = Readable.from(next.body ?? [Buffer.from("abc")]);
        response.statusCode = next.status ?? 200;
        response.headers = next.headers ?? {};
        callback(response);
      });
    else
      options.signal.addEventListener("abort", () => request.emit("error", Error("deadline")), {
        once: true,
      });
    return request;
  });
  await assert.rejects(downloadAsset("http://github.com/unsafe", 3));
  await assert.rejects(downloadAsset("https://other.example/unsafe", 3));
  for (const response of [
    { error: "network unavailable" },
    { status: 500 },
    { headers: { "content-encoding": "br" } },
    { headers: { "content-length": "5" } },
    { body: [Buffer.from("ab")] },
    { body: [Buffer.from("abcd")] },
    { status: 302, headers: { location: "https://other.example/unsafe" } },
  ]) {
    responses.push(response);
    await assert.rejects(downloadAsset(url, 3));
  }
  await assert.rejects(downloadAsset(url, 3, 1));
  responses.push(
    { status: 302, headers: { location: "https://release-assets.githubusercontent.com/pinned" } },
    {},
  );
  assert.equal((await downloadAsset(url, 3)).toString(), "abc");
});

test("manifest is bounded to three authenticated platforms and fixed security-library identities", () => {
  const manifest = JSON.parse(
    fs.readFileSync(new URL("../scripts/nix_ci_runtime.json", import.meta.url)),
  );
  assert.deepEqual(Object.keys(manifest.platforms).sort(), [
    "aarch64-darwin",
    "aarch64-linux",
    "x86_64-linux",
  ]);
  for (const entry of Object.values(manifest.platforms)) {
    assert.match(
      entry.asset,
      /^https:\/\/github.com\/DeterminateSystems\/nix-installer\/releases\/download\/v3\.24\.1\/nix-installer-/,
    );
    assert.equal(entry.asset_sha256.length, 64);
    assert.equal(entry.slice_sha256.length, 64);
    assert(entry.slice_offset + entry.slice_length <= entry.asset_size);
    assert(Object.hasOwn(entry.critical_files, entry.nix));
    for (const version of ["-curl-8.23.0", "-brotli-1.2.1", "-openssl-3.6.5"]) {
      assert(Object.keys(entry.critical_files).some((file) => file.includes(version)));
    }
    for (const file of Object.keys(entry.critical_files))
      assert(entry.store_roots.some((root) => file.startsWith(`${root}/`)));
    assert(entry.installer_files[".reginfo"]);
    assert(entry.installer_files.install);
    assert.equal(
      entry.critical_files[entry.ca_cert],
      "81ffe9877138a9e9bee56e067479a5abd59c65cdbb310d830b13108e1afe4e33",
    );
  }
});

test("file-hash gate rejects a changed critical file in an owned sandbox", () => {
  const temporary = fs.mkdtempSync(
    path.join(path.dirname(new URL(import.meta.url).pathname), "nix-file-test-"),
  );
  try {
    const leaf = path.join(temporary, "store/test/bin/nix");
    fs.mkdirSync(path.dirname(leaf), { recursive: true });
    fs.writeFileSync(leaf, "approved");
    const policy = {
      critical_files: { "/nix/store/test/bin/nix": hash(Buffer.from("approved")) },
      installer_files: {},
    };
    verifyFiles(policy, temporary);
    fs.writeFileSync(leaf, "replaced");
    assert.throws(() => verifyFiles(policy, temporary));
    fs.chmodSync(path.dirname(leaf), 0o500);
  } finally {
    removeStaging(temporary);
  }
});

test(
  "all three real authenticated payloads pass the extractor and exact file gate without execution",
  {
    skip: !process.env.RESHIKI_NIX_PASSIVE_ROOT,
  },
  () => {
    const root = process.env.RESHIKI_NIX_PASSIVE_ROOT;
    const manifest = JSON.parse(
      fs.readFileSync(new URL("../scripts/nix_ci_runtime.json", import.meta.url)),
    );
    for (const [system, policy] of Object.entries(manifest.platforms)) {
      const temporary = fs.mkdtempSync(path.join(root, "passive-runtime-test-"));
      try {
        const asset = fs.readFileSync(path.join(root, "assets", `nix-installer-${system}`));
        const archive = path.join(temporary, "nix.tar.xz");
        fs.writeFileSync(archive, embeddedSlice(asset, policy));
        const listing = spawnSync("/usr/bin/tar", ["-tf", archive], {
          encoding: "utf8",
          maxBuffer: 32 * 1024 * 1024,
        });
        assert.equal(listing.status, 0, listing.stderr);
        validateMembers(listing.stdout, policy.tar_root);
        const extraction = spawnSync("/usr/bin/tar", ["-xf", archive, "-C", temporary], {
          encoding: "utf8",
        });
        assert.equal(extraction.status, 0, extraction.stderr);
        verifyFiles(policy, path.join(temporary, policy.tar_root));
        assert.equal(
          fs.readlinkSync(
            path.join(temporary, policy.tar_root, policy.daemon.slice("/nix/".length)),
          ),
          "nix",
        );
      } finally {
        removeStaging(temporary);
      }
    }
  },
);
