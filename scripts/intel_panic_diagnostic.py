"""Bounded native Intel probe; retain every result without building the app."""

import hashlib
import json
import os
import platform
import shutil
import signal
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = "x86_64-apple-darwin"
MODES = (
    "alignment",
    "main-literal",
    "main-payload",
    "thread-literal",
    "thread-payload",
    "thread-join-payload",
)


def run(command, stem, timeout):
    started = time.monotonic()
    with stem.with_suffix(".stdout.txt").open("wb") as stdout:
        with stem.with_suffix(".stderr.txt").open("wb") as stderr:
            child = subprocess.Popen(command, stdout=stdout, stderr=stderr, start_new_session=True)
            timed_out = False
            try:
                child.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                timed_out = True
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
    result = dict(
        command=[str(arg) for arg in command],
        exit_code=child.returncode,
        timed_out=timed_out,
        elapsed_seconds=time.monotonic() - started,
    )
    stem.with_suffix(".json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def main():
    output = Path(os.environ["RUNNER_TEMP"]) / "intel-panic-diagnostic"
    output.mkdir(exist_ok=False)
    if platform.system() != "Darwin" or platform.machine() != "x86_64":
        raise RuntimeError("This diagnostic requires the native Intel macOS runner")
    environment = dict(
        commit=os.environ.get("GITHUB_SHA"),
        system=platform.system(),
        architecture=platform.machine(),
        macos=platform.mac_ver()[0],
        deployment_target=os.environ.get("MACOSX_DEPLOYMENT_TARGET"),
    )
    (output / "environment.json").write_text(json.dumps(environment, indent=2) + "\n")
    for name, command in (
        ("compiler", ["rustup", "run", "1.99.0", "rustc", "-vV"]),
        ("translation", ["sysctl", "-in", "sysctl.proc_translated"]),
    ):
        run(command, output / name, 10)
    translated = (output / "translation.stdout.txt").read_text().strip()
    if translated == "1":
        raise RuntimeError("Rosetta is not native Intel validation")

    scratch = Path(tempfile.mkdtemp(prefix="intel-panic-build-", dir=os.environ["RUNNER_TEMP"]))
    heap = ROOT / "crates/process-heap/src"
    aligned = scratch / "aligned-heap"
    shutil.copytree(heap, aligned)
    source = (aligned / "lib.rs").read_text()
    needle = "fn layout(original: Layout) -> Option<(Layout, usize)> {\n"
    if source.count(needle) != 1:
        raise RuntimeError("The diagnostic alignment patch no longer matches the heap source")
    # Hypothesis only: modify the runner's private clone, never production files.
    (aligned / "lib.rs").write_text(
        source.replace(needle, needle + "    let original = original.align_to(16).ok()?;\n")
    )
    (output / "alignment-hypothesis.diff.txt").write_text(
        "+    let original = original.align_to(16).ok()?;\n"
    )
    (output / "source-sha256.json").write_text(
        json.dumps(
            {
                "production_heap": hashlib.sha256((heap / "lib.rs").read_bytes()).hexdigest(),
                "aligned_clone": hashlib.sha256((aligned / "lib.rs").read_bytes()).hexdigest(),
                "probe": hashlib.sha256(
                    (ROOT / "scripts/intel_panic_probe.rs").read_bytes()
                ).hexdigest(),
            },
            indent=2,
        )
        + "\n"
    )
    results = []
    build_failures = []
    for profile, optimization in (("debug", "0"), ("release", "3")):
        common = [
            "rustup",
            "run",
            "1.99.0",
            "rustc",
            "--edition=2024",
            "--target",
            TARGET,
            "-C",
            f"opt-level={optimization}",
            "-C",
            "debuginfo=2",
            "-C",
            "panic=unwind",
            "-C",
            "linker=/usr/bin/clang",
        ]
        for variant, allocator, heap_source in (
            ("system", "allocator_system", heap),
            ("bounded", "allocator_bounded", heap),
            ("measured", "allocator_measured", heap),
            ("aligned16", "allocator_measured", aligned),
        ):
            name = f"{profile}-{variant}"
            library = scratch / f"lib{name}.rlib"
            build = run(
                common
                + [
                    "--crate-name",
                    "reshiki_process_heap",
                    "--crate-type",
                    "rlib",
                    "--cfg",
                    'feature="allocation-metrics"',
                    heap_source / "lib.rs",
                    "-o",
                    library,
                ],
                output / f"{name}-heap-build",
                60,
            )
            if build["exit_code"]:
                build_failures.append(name + "-heap")
                continue
            binary = scratch / name
            build = run(
                common
                + [
                    "--cfg",
                    allocator,
                    "--extern",
                    f"reshiki_process_heap={library}",
                    ROOT / "scripts/intel_panic_probe.rs",
                    "-o",
                    binary,
                ],
                output / f"{name}-probe-build",
                60,
            )
            if build["exit_code"]:
                build_failures.append(name + "-probe")
                continue
            captured_crash = False
            for mode in MODES:
                case = f"{name}-{mode}"
                result = run([binary, mode], output / case, 10)
                result.update(profile=profile, variant=variant, mode=mode)
                results.append(result)
                print(
                    f"{case}: exit={result['exit_code']} timeout={result['timed_out']}", flush=True
                )
                if result["exit_code"] < 0 and not result["timed_out"] and not captured_crash:
                    captured_crash = True
                    run(
                        [
                            "xcrun",
                            "lldb",
                            "--batch",
                            "-o",
                            "settings set target.disable-aslr false",
                            "-o",
                            "run",
                            "-o",
                            "thread backtrace all",
                            "-o",
                            "register read",
                            "-o",
                            "disassemble --frame",
                            "--",
                            binary,
                            mode,
                        ],
                        output / f"{case}-lldb",
                        20,
                    )
    (output / "summary.json").write_text(
        json.dumps(dict(build_failures=build_failures, results=results), indent=2) + "\n"
    )
    # Native crashes remain recorded outcomes; setup/build failures fail the job.
    return 1 if build_failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
