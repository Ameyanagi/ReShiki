// CI-only installation of an authenticated classic Nix bundle. Never execute
// the outer installer, an existing Nix, a flake, or a supervisor before approval.
import fs from "node:fs";
import path from "node:path";
import https from "node:https";
import crypto from "node:crypto";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const directory = path.dirname(fileURLToPath(import.meta.url));
const manifest = JSON.parse(fs.readFileSync(path.join(directory, "nix_ci_runtime.json"), "utf8"));
const socket = "/nix/var/nix/daemon-socket/socket";
const profile = "/nix/var/nix/profiles/default/bin";
const sha256 = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");

export function selectPlatform(
  environment,
  os,
  arch,
  nativeOS = process.platform,
  nativeArch = process.arch,
) {
  const systems = {
    "Linux/X64": "x86_64-linux",
    "Linux/ARM64": "aarch64-linux",
    "macOS/ARM64": "aarch64-darwin",
  };
  const key = `${os}/${arch}`;
  if (
    environment !== "github-hosted" ||
    !systems[key] ||
    { Linux: "linux", macOS: "darwin" }[os] !== nativeOS ||
    { X64: "x64", ARM64: "arm64" }[arch] !== nativeArch
  ) {
    throw Error(
      "Only matching ephemeral GitHub-hosted Linux x64/ARM64 and macOS ARM64 are supported",
    );
  }
  return systems[key];
}

export function assertEnvironment(env) {
  for (const [name, value] of Object.entries(env)) {
    if (
      value &&
      (/^(NIX_|LD_|DYLD_)/.test(name) ||
        ["BASH_ENV", "ENV", "NODE_OPTIONS", "NODE_PATH"].includes(name))
    ) {
      throw Error(`Refusing inherited runtime/installer override: ${name}`);
    }
  }
}

export function checkHash(bytes, expected) {
  if (!/^[a-f0-9]{64}$/.test(expected) || sha256(bytes) !== expected)
    throw Error("Authenticated Nix bytes do not match SHA256");
}

export function embeddedSlice(asset, policy) {
  checkHash(asset, policy.asset_sha256);
  if (
    asset.length !== policy.asset_size ||
    !Number.isSafeInteger(policy.slice_offset) ||
    !Number.isSafeInteger(policy.slice_length) ||
    policy.slice_offset < 0 ||
    policy.slice_length <= 0 ||
    policy.slice_offset + policy.slice_length > asset.length
  )
    throw Error("Invalid embedded Nix slice bounds/size");
  const slice = asset.subarray(policy.slice_offset, policy.slice_offset + policy.slice_length);
  checkHash(slice, policy.slice_sha256);
  return slice;
}

export function validateMembers(output, root) {
  const seen = new Set();
  for (const name of output.trimEnd().split("\n")) {
    const clean = name.replace(/\/$/, "");
    if (
      !clean ||
      (clean !== root && !clean.startsWith(`${root}/`)) ||
      name.includes("\\") ||
      name.split("/").some((part) => part === ".." || part === ".") ||
      seen.has(clean)
    ) {
      throw Error(`Unexpected/duplicate archive member: ${name}`);
    }
    seen.add(clean);
  }
  for (const required of ["install", "install-multi-user", ".reginfo"]) {
    if (!seen.has(`${root}/${required}`))
      throw Error(`Missing authenticated installer member: ${required}`);
  }
  if (!seen.has(`${root}/store`) && ![...seen].some((name) => name.startsWith(`${root}/store/`))) {
    throw Error("Missing authenticated store members");
  }
}

function run(file, args, { env = process.env, timeout = 30000, inherit = false } = {}) {
  const result = spawnSync(file, args, {
    env,
    timeout,
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
    stdio: inherit ? "inherit" : "pipe",
  });
  if (result.error || result.status !== 0)
    throw Error(
      `Command failed: ${file} (${result.status}): ${result.error?.message ?? result.stderr ?? ""}`,
    );
  return result.stdout ?? "";
}

function noLinks(file) {
  for (let current = path.resolve(file); ; current = path.dirname(current)) {
    if (fs.lstatSync(current).isSymbolicLink())
      throw Error(`Refusing linked staging ancestor: ${current}`);
    if (path.dirname(current) === current) break;
  }
}

function exists(file) {
  try {
    fs.lstatSync(file);
    return true;
  } catch (error) {
    if (error.code === "ENOENT") return false;
    throw error;
  }
}

function cleanInstallation() {
  for (const file of [
    "/nix",
    "/etc/nix",
    "/nix/receipt.json",
    "/usr/local/bin/determinate-nixd",
    "/etc/systemd/system/nix-daemon.service",
    "/Library/LaunchDaemons/org.nixos.nix-daemon.plist",
    "/Library/LaunchDaemons/systems.determinate.nix-daemon.plist",
  ]) {
    if (exists(file))
      throw Error(`Refusing to reuse or modify an existing Nix installation: ${file}`);
  }
  for (const entry of (process.env.PATH ?? "").split(path.delimiter)) {
    if (entry && exists(path.join(entry, "nix")))
      throw Error("A Nix executable already exists on PATH");
  }
}

export async function downloadAsset(url, expectedSize, deadlineMs = 180000) {
  const abort = new AbortController();
  const timer = setTimeout(() => abort.abort(), deadlineMs);
  const allowed = new Set(["github.com", "release-assets.githubusercontent.com"]);
  try {
    for (let redirects = 0; redirects <= 3; redirects++) {
      const target = new URL(url);
      if (
        target.protocol !== "https:" ||
        target.username ||
        target.password ||
        target.port ||
        !allowed.has(target.hostname)
      ) {
        throw Error("Unexpected Nix release download origin");
      }
      const response = await new Promise((resolve, reject) => {
        const request = https.get(
          target,
          { signal: abort.signal, headers: { "Accept-Encoding": "identity" } },
          resolve,
        );
        request.on("error", reject);
      });
      if ([301, 302, 303, 307, 308].includes(response.statusCode)) {
        const location = response.headers.location;
        response.destroy();
        if (!location || redirects === 3) throw Error("Invalid/excessive Nix release redirect");
        url = new URL(location, target).href;
        continue;
      }
      if (
        response.statusCode !== 200 ||
        (response.headers["content-encoding"] &&
          response.headers["content-encoding"] !== "identity") ||
        (response.headers["content-length"] &&
          Number(response.headers["content-length"]) !== expectedSize)
      ) {
        response.destroy();
        throw Error("Unexpected Nix download status/encoding/length");
      }
      const chunks = [];
      let count = 0;
      for await (const chunk of response) {
        count += chunk.length;
        if (count > expectedSize || count > 134217728) {
          response.destroy();
          throw Error("Nix download exceeded its pinned size");
        }
        chunks.push(chunk);
      }
      if (count !== expectedSize) throw Error("Partial Nix download");
      return Buffer.concat(chunks);
    }
    throw Error("No authenticated Nix download");
  } finally {
    clearTimeout(timer);
  }
}

export function verifyFiles(policy, base = "") {
  const hashes = { ...policy.critical_files };
  if (base)
    for (const [relative, hash] of Object.entries(policy.installer_files)) {
      hashes[path.join(base, relative)] = hash;
    }
  for (const [absolute, hash] of Object.entries(hashes)) {
    const file =
      base && absolute.startsWith("/nix/store/")
        ? path.join(base, absolute.slice("/nix/".length))
        : absolute;
    if (!fs.lstatSync(file).isFile()) throw Error(`Expected a regular authenticated file: ${file}`);
    checkHash(fs.readFileSync(file), hash);
  }
}

export function validateObservation(observation, policy) {
  if (
    !Number.isSafeInteger(observation.pid) ||
    observation.pid <= 1 ||
    observation.uid !== 0 ||
    observation.executable !== policy.nix ||
    !observation.socketOwned
  )
    throw Error("Wrong/unknown active classic Nix daemon");
  const loaded = new Set(observation.loaded);
  for (const file of loaded) {
    if (
      file.startsWith("/nix/store/") &&
      !policy.store_roots.some((root) => file.startsWith(`${root}/`))
    ) {
      throw Error(`Daemon loaded an unapproved store object: ${file}`);
    }
  }
  for (const component of ["-curl-", "-brotli-", "-openssl-"]) {
    if (
      ![...loaded].some(
        (file) => file.includes(component) && Object.hasOwn(policy.critical_files, file),
      )
    ) {
      throw Error(`Daemon has no authenticated loaded ${component} library`);
    }
  }
}

export function parseService(text, linux) {
  if (linux) {
    const lines = text.trim().split("\n");
    const fields = Object.fromEntries(
      lines.map((line) => {
        const i = line.indexOf("=");
        return [line.slice(0, i), line.slice(i + 1)];
      }),
    );
    if (
      lines.length !== 3 ||
      fields.ActiveState !== "active" ||
      fields.DropInPaths !== "" ||
      !/^[0-9]+$/.test(fields.MainPID)
    ) {
      throw Error("Nix daemon inactive, ambiguous or overridden");
    }
    return Number(fields.MainPID);
  }
  const matches = [...text.matchAll(/^\s*pid = ([0-9]+)\s*$/gm)];
  if (matches.length !== 1) throw Error("Ambiguous/missing launchd Nix PID");
  return Number(matches[0][1]);
}

export function validateStableObservation(first, second, policy) {
  validateObservation(first, policy);
  validateObservation(second, policy);
  if (first.pid !== second.pid || first.executable !== second.executable)
    throw Error("Nix daemon changed during attestation");
}

export function removeStaging(temporary) {
  // Authenticated store directories may be read-only. Restore owner access only
  // inside this freshly created staging tree; never follow its store symlinks.
  const directories = [temporary];
  while (directories.length) {
    const current = directories.pop();
    fs.chmodSync(current, 0o700);
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      if (entry.isDirectory()) directories.push(path.join(current, entry.name));
    }
  }
  fs.rmSync(temporary, { recursive: true, force: true });
}

function observeDaemon(policy, pwsh) {
  const linux = process.platform === "linux";
  let pid;
  let service;
  if (linux) {
    service = "/etc/systemd/system/nix-daemon.service";
    const text = run("/usr/bin/systemctl", [
      "show",
      "nix-daemon.service",
      "--property=MainPID,ActiveState,DropInPaths",
    ]);
    pid = parseService(text, true);
  } else {
    service = "/Library/LaunchDaemons/org.nixos.nix-daemon.plist";
    const text = run("/usr/bin/sudo", [
      "-n",
      "/bin/launchctl",
      "print",
      "system/org.nixos.nix-daemon",
    ]);
    pid = parseService(text, false);
  }
  const serviceSuffix = linux
    ? "/lib/systemd/system/nix-daemon.service"
    : "/org.nixos.nix-daemon.plist";
  const expectedService = Object.entries(policy.installer_files).filter(([name]) =>
    name.endsWith(serviceSuffix),
  );
  if (expectedService.length !== 1) throw Error("Missing pinned classic service definition");
  checkHash(fs.readFileSync(service), expectedService[0][1]);
  if (!Number.isSafeInteger(pid) || pid <= 1) throw Error("Nix daemon has no live PID");
  let executable;
  if (linux)
    executable = run("/usr/bin/sudo", ["-n", "/usr/bin/readlink", `/proc/${pid}/exe`]).trim();
  else {
    if (!path.isAbsolute(pwsh) || !fs.lstatSync(pwsh).isFile())
      throw Error("Missing absolute PowerShell process observer");
    executable = run("/usr/bin/sudo", [
      "-n",
      pwsh,
      "-NoLogo",
      "-NoProfile",
      "-Command",
      `[Console]::WriteLine((Get-Process -Id ${pid} -ErrorAction Stop).Path)`,
    ]).trim();
  }
  executable = fs.realpathSync(executable);
  const uidText = run("/bin/ps", ["-p", String(pid), "-o", "uid="]).trim();
  if (!/^[0-9]+$/.test(uidText)) throw Error("Missing daemon process owner");
  const uid = Number(uidText);
  const lsof = linux ? "/usr/bin/lsof" : "/usr/sbin/lsof";
  const sockets = run("/usr/bin/sudo", ["-n", lsof, "-nP", "-a", "-p", String(pid), "-U", "-Fn"]);
  const socketOwned = sockets
    .split("\n")
    .some((line) => line === `n${socket}` || line.startsWith(`n${socket} `));
  let loaded;
  if (linux) {
    const maps = run("/usr/bin/sudo", ["-n", "/bin/cat", `/proc/${pid}/maps`]);
    loaded = maps
      .split("\n")
      .map((line) => line.match(/\s(\/nix\/store\/.*)$/)?.[1])
      .filter(Boolean);
  } else {
    const maps = run("/usr/bin/sudo", [
      "-n",
      lsof,
      "-nP",
      "-a",
      "-p",
      String(pid),
      "-d",
      "txt",
      "-Fn",
    ]);
    loaded = maps
      .split("\n")
      .filter((line) => line.startsWith("n/nix/store/"))
      .map((line) => line.slice(1));
  }
  const observation = { pid, uid, executable, socketOwned, loaded };
  validateObservation(observation, policy);
  return observation;
}

async function main() {
  if (process.env.GITHUB_ACTIONS !== "true" || !/^[0-9]+$/.test(process.env.GITHUB_RUN_ID ?? "")) {
    throw Error("This installer is restricted to a GitHub-hosted workflow job");
  }
  const system = selectPlatform(
    process.env.RESHIKI_RUNNER_ENVIRONMENT,
    process.env.RESHIKI_RUNNER_OS,
    process.env.RESHIKI_RUNNER_ARCH,
  );
  assertEnvironment(process.env);
  cleanInstallation();
  if (
    !process.env.RUNNER_TEMP ||
    !process.env.GITHUB_PATH ||
    !process.env.GITHUB_ENV ||
    !process.env.RESHIKI_PWSH
  )
    throw Error("Missing hosted runner paths");
  noLinks(process.env.RUNNER_TEMP);
  const policy = manifest.platforms[system];
  const temporary = fs.mkdtempSync(path.join(process.env.RUNNER_TEMP, "reshiki-nix-"));
  fs.chmodSync(temporary, 0o700);
  try {
    const asset = await downloadAsset(policy.asset, policy.asset_size);
    const slice = embeddedSlice(asset, policy);
    const archive = path.join(temporary, "nix.tar.xz");
    fs.writeFileSync(archive, slice, { flag: "wx", mode: 0o600 });
    const tarEnv = { ...process.env, PATH: "/usr/bin:/bin:/usr/sbin:/sbin" };
    validateMembers(run("/usr/bin/tar", ["-tf", archive], { env: tarEnv }), policy.tar_root);
    run("/usr/bin/tar", ["-xf", archive, "-C", temporary], { env: tarEnv });
    const bundle = path.join(temporary, policy.tar_root);
    verifyFiles(policy, bundle);
    const config = path.join(temporary, "nix-extra.conf");
    fs.writeFileSync(
      config,
      `experimental-features = nix-command flakes\nrequire-sigs = true\nssl-cert-file = ${policy.ca_cert}\n`,
      {
        flag: "wx",
        mode: 0o600,
      },
    );
    // Running the shipped multi-user installer as root preserves these explicit
    // opt-outs through its internal _sudo helper. All store inputs are local.
    run(
      "/usr/bin/sudo",
      [
        "-n",
        "/usr/bin/env",
        "NIX_SENTRY_ENDPOINT=",
        "DETSYS_IDS_TELEMETRY=disabled",
        "OTEL_SDK_DISABLED=true",
        `NIX_CONFIG=substituters =\nssl-cert-file = ${policy.ca_cert}\n`,
        `NIX_SSL_CERT_FILE=${policy.ca_cert}`,
        "/bin/sh",
        path.join(bundle, "install"),
        "--daemon",
        "--yes",
        "--no-channel-add",
        "--nix-extra-conf-file",
        config,
      ],
      { env: tarEnv, timeout: 600000, inherit: true },
    );
    if (exists("/usr/local/bin/determinate-nixd") || exists("/etc/nix/sentry-endpoint"))
      throw Error("Unexpected supervisor or crash endpoint");
    if (
      fs.realpathSync(path.join(profile, "nix")) !== policy.nix ||
      fs.realpathSync(policy.daemon) !== policy.nix
    ) {
      throw Error("Installed client/daemon does not resolve to the authenticated store identity");
    }
    verifyFiles(policy);
    const configuration = fs.readFileSync("/etc/nix/nix.conf", "utf8");
    const caSettings = configuration
      .split("\n")
      .filter((line) => /^\s*ssl-cert-file\s*=/.test(line));
    if (caSettings.length !== 1 || caSettings[0].trim() !== `ssl-cert-file = ${policy.ca_cert}`) {
      throw Error("The daemon CA bundle was overridden or omitted");
    }
    const environment = {
      ...tarEnv,
      NIX_SENTRY_ENDPOINT: "",
      NIX_SSL_CERT_FILE: policy.ca_cert,
      DETSYS_IDS_TELEMETRY: "disabled",
      OTEL_SDK_DISABLED: "true",
    };
    const version = run(policy.nix, ["--version"], { env: environment }).trim();
    if (!version.includes("3.24.1")) throw Error("Unexpected authenticated Nix version");
    // Activate a socket-only classic service using authenticated local metadata
    // RPC, with no flake evaluation, substitution or remote endpoint.
    run(
      policy.nix,
      ["--offline", "--store", `unix://${socket}`, "--option", "substituters", "", "store", "ping"],
      { env: environment },
    );
    const first = observeDaemon(policy, process.env.RESHIKI_PWSH);
    verifyFiles(policy);
    const second = observeDaemon(policy, process.env.RESHIKI_PWSH);
    validateStableObservation(first, second, policy);
    console.log(
      JSON.stringify(
        {
          system,
          asset: policy.asset_sha256,
          embedded: policy.slice_sha256,
          client: policy.nix,
          version,
          daemon: second,
          critical_files: policy.critical_files,
        },
        null,
        2,
      ),
    );
    fs.appendFileSync(process.env.GITHUB_PATH, `${profile}\n`);
    fs.appendFileSync(
      process.env.GITHUB_ENV,
      `NIX_SSL_CERT_FILE=${policy.ca_cert}\nNIX_SENTRY_ENDPOINT=\nDETSYS_IDS_TELEMETRY=disabled\nOTEL_SDK_DISABLED=true\n`,
    );
  } finally {
    removeStaging(temporary);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}
