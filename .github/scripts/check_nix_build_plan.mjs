import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";

const storeDerivation = /^  (\/nix\/store\/[a-z0-9]{32}-[^/]+\.drv)$/;
const sourceLLVM =
  /-(?:llvm(?:-tblgen)?|libllvm|clang(?:-unwrapped)?|libclang|lld|lldb|compiler-rt|libcxx(?:abi)?)(?:-[0-9][\w.+-]*)?\.drv$/;
const sourceRust = /\/[a-z0-9]{32}-(?:rustc|cargo)-[0-9][\w.+-]*\.drv$/;
const sourceArchive = /\.(?:tar\.(?:gz|xz|bz2|zst)|zip|patch)\.drv$/;

export function plannedBuilds(log) {
  let expected = 0;
  let foundHeader = false;
  const builds = [];
  for (const line of log.split("\n").filter(Boolean)) {
    if (!line.startsWith("@nix ")) {
      throw new Error("Unexpected Nix dry-run log format.");
    }
    const event = JSON.parse(line.slice(5));
    if (event.action !== "msg") continue;
    if (typeof event.msg !== "string") {
      throw new Error("Malformed Nix dry-run message.");
    }
    const message = event.msg.replace(/\u001b\[[0-9;]*m/g, "");
    const header = message.match(/^these ([1-9][0-9]*) derivations will be built:$/);
    if (header || message === "this derivation will be built:") {
      if (foundHeader) {
        throw new Error("Duplicate Nix build-plan header.");
      }
      foundHeader = true;
      expected = header ? Number(header[1]) : 1;
      continue;
    }
    const derivation = message.match(storeDerivation);
    if (derivation) {
      if (!foundHeader || builds.length >= expected) {
        throw new Error("Derivation outside the declared Nix build plan.");
      }
      builds.push(derivation[1]);
    } else if (message.includes(".drv") && !message.startsWith("warning:")) {
      throw new Error("Unrecognized derivation in Nix dry-run output.");
    }
  }
  if (builds.length !== expected || new Set(builds).size !== builds.length) {
    throw new Error("Incomplete or duplicate Nix build plan.");
  }
  return builds;
}

export function assertNoSourceToolchain(builds) {
  const rejected = builds.filter(
    (path) => !sourceArchive.test(path) && (sourceLLVM.test(path) || sourceRust.test(path)),
  );
  if (rejected.length) {
    throw new Error(
      "Refusing source LLVM/Rust/Cargo builds; restore binary-cache availability " +
        `or update the pinned toolchain:\n${rejected.join("\n")}`,
    );
  }
}

function nix(args) {
  const result = spawnSync("nix", args, {
    encoding: "utf8",
    env: { ...process.env, LC_ALL: "C" },
    maxBuffer: 32 * 1024 * 1024,
    timeout: 300_000,
  });
  if (result.error || result.status !== 0) {
    throw new Error(`Nix build-plan query failed: ${result.error?.message ?? result.stderr}`);
  }
  return result;
}

function main() {
  const system =
    process.argv[2] ??
    nix(["eval", "--raw", "--impure", "--expr", "builtins.currentSystem"]).stdout;
  if (!["x86_64-linux", "aarch64-linux", "aarch64-darwin"].includes(system)) {
    throw new Error(`Unsupported Nix CI system: ${system}`);
  }
  const result = nix([
    "build",
    "--dry-run",
    "--json",
    "--no-link",
    "--no-write-lock-file",
    "--no-update-lock-file",
    "--log-format",
    "internal-json",
    `.#checks.${system}.reshiki`,
  ]);
  const outputs = JSON.parse(result.stdout);
  if (!Array.isArray(outputs) || outputs.length !== 1 || typeof outputs[0].drvPath !== "string") {
    throw new Error("Nix dry run did not identify exactly one application derivation.");
  }
  const builds = plannedBuilds(result.stderr);
  assertNoSourceToolchain(builds);
  console.log(
    JSON.stringify({
      system,
      application: outputs[0].drvPath,
      scheduledBuilds: builds.length,
      sourceToolchainBuilds: 0,
    }),
  );
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
