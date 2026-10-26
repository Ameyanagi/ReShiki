import assert from "node:assert/strict";
import test from "node:test";
import { assertNoSourceToolchain, plannedBuilds } from "./check_nix_build_plan.mjs";

const path = (name) => `/nix/store/${"0".repeat(32)}-${name}`;
const message = (msg) => `@nix ${JSON.stringify({ action: "msg", level: 0, msg })}`;
const plan = (names) =>
  [
    message(
      names.length === 1
        ? "this derivation will be built:"
        : `these ${names.length} derivations will be built:`,
    ),
    ...names.map((name) => message(`  ${path(name)}`)),
  ].join("\n");

test("reject source LLVM/Rust/Cargo builds while allowing binary toolchains and source fetches", () => {
  for (const name of [
    "llvm-21.1.8",
    "llvm-tblgen-21.1.8",
    "clang-21.1.8",
    "libcxx-21.1.8",
    "bootstrap-stage0-libcxx",
    "rustc-1.99.1",
    "cargo-1.99.1",
  ]) {
    assert.throws(
      () => assertNoSourceToolchain(plannedBuilds(plan([`${name}.drv`]))),
      /Refusing source/,
    );
  }
  assert.doesNotThrow(() =>
    assertNoSourceToolchain(
      plannedBuilds(
        plan([
          "rustc-bootstrap-1.99.1.drv",
          "cargo-bootstrap-1.99.1.drv",
          "llvm-21.1.8.src.tar.gz.drv",
          "cargo-auditable-0.7.4.drv",
          "auditable-cargo-1.99.0.drv",
          "reshiki-0.13.1.drv",
        ]),
      ),
    ),
  );
});

test("cached LLVM paths do not count as builds", () => {
  const log = [
    plan(["reshiki-0.13.1.drv"]),
    message("this path will be fetched (1 MiB download, 2 MiB unpacked):"),
    message(`  ${path("llvm-21.1.8")}`),
  ].join("\n");
  assert.deepEqual(plannedBuilds(log), [path("reshiki-0.13.1.drv")]);
  assert.doesNotThrow(() => assertNoSourceToolchain(plannedBuilds(log)));
  assert.deepEqual(plannedBuilds(message(`  ${path("llvm-21.1.8")}`)), []);
});

test("fail closed on truncated, duplicate, malformed, and unfamiliar derivation plans", () => {
  for (const log of [
    message("these 2 derivations will be built:"),
    plan(["reshiki.drv", "reshiki.drv"]),
    `${plan(["reshiki.drv"])}\n${plan(["llvm-21.1.8.drv"])}`,
    message(`  ${path("llvm-21.1.8.drv")}`),
    message(`will build ${path("llvm-21.1.8.drv")}`),
    "not JSON",
    "@nix {",
  ]) {
    assert.throws(() => plannedBuilds(log));
  }
});
