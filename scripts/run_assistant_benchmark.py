"""Capped, opt-in Codex requests against original public benchmark images.
The manifest/reference graph never enters a model request.
"""

import argparse
import datetime as dt
import hashlib
import json
import os
import platform
import shutil
import signal
import subprocess
import time
from pathlib import Path


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def bounded_run(command, seconds, environment=None):
    # Put the frozen runner and its Codex/chemical children in a disposable
    # process group. A watchdog must stop the entire workflow, not orphan it.
    process = subprocess.Popen(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=os.name == "posix",
        env=environment,
    )
    try:
        stdout, stderr = process.communicate(timeout=seconds)
        return subprocess.CompletedProcess(command, process.returncode, stdout, stderr)
    except BaseException:
        if os.name == "posix":
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        elif process.poll() is None:
            subprocess.run(
                ["taskkill", "/PID", str(process.pid), "/T", "/F"], capture_output=True, check=False
            )
        process.communicate()
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--runner", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--backend-commit", required=True)
    parser.add_argument("--nightly-version", required=True)
    parser.add_argument("--budget-seconds", type=int, default=1800)
    parser.add_argument("--request-seconds", type=int, default=300)
    parser.add_argument("--max-requests", type=int, default=16)
    parser.add_argument("--repeats", type=int, default=2)
    parser.add_argument("--model", default="gpt-6.1-sol")
    parser.add_argument("--effort", default="xhigh")
    parser.add_argument(
        "--run",
        action="store_true",
        help="Explicitly enable external model requests; otherwise write plan only.",
    )
    args = parser.parse_args()
    if not (
        1 <= args.budget_seconds <= 1800
        and 1 <= args.request_seconds <= 300
        and 1 <= args.max_requests <= 16
        and 1 <= args.repeats <= 2
    ):
        parser.error("Limits are total<=1800s, per-request<=300s, attempts<=16, repeats<=2")
    manifest = json.loads(args.manifest.read_text())
    args.output.mkdir(parents=True, exist_ok=True)
    plan = [(case, repeat) for repeat in range(1, args.repeats + 1) for case in manifest["cases"]]
    metadata = {
        "started_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "backend_commit": args.backend_commit,
        "nightly_version": args.nightly_version,
        "runner_sha256": hashlib.sha256(args.runner.read_bytes()).hexdigest(),
        "manifest_sha256": hashlib.sha256(args.manifest.read_bytes()).hexdigest(),
        "source_manifest": manifest,
        "provider": "Codex app-server",
        "model": args.model,
        "effort": args.effort,
        "service_tier": "default",
        "total_budget_seconds": args.budget_seconds,
        "per_request_budget_seconds": args.request_seconds,
        "max_requests": args.max_requests,
        "repeats": args.repeats,
        "synthetic_images": True,
        "build_profile": "debug",
        "environment": {
            "platform": platform.platform(),
            "architecture": platform.machine(),
            "python": platform.python_version(),
        },
        "runtime": "Actual Assistant backend in headless runner; application helper dispatch, bounded allocator and blank-canvas tools. Generic fixed request; no reference data.",
        "plan": [{"case": c["id"], "repeat": r} for c, r in plan],
    }
    codex = shutil.which("codex")
    if not codex:
        raise RuntimeError("No Codex CLI in the benchmark launch environment")
    codex = Path(codex).absolute()
    environment = os.environ.copy()
    environment["RESHIKI_CODEX"] = str(codex)
    environment["RESHIKI_INCHI_HELPER"] = str(args.runner.resolve())
    version = subprocess.run([str(codex), "--version"], capture_output=True, text=True, check=True)
    metadata["codex_executable_sha256"] = hashlib.sha256(codex.read_bytes()).hexdigest()
    metadata["codex_version"] = version.stdout.strip()
    write_json(args.output / "metadata.json", metadata)
    if not args.run:
        print(
            "Plan written. No model requests made. Use --run to execute the capped benchmark.",
            flush=True,
        )
        return
    overall_started = time.monotonic()
    deadline = overall_started + args.budget_seconds
    attempted = 0
    unavailable_reason = None
    inference_workflows = 0
    for case, repeat in plan:
        run_dir = args.output / case["id"] / f"run-{repeat}"
        run_dir.mkdir(parents=True, exist_ok=True)
        if (run_dir / "run.json").exists():
            raise RuntimeError(f"Refusing to overwrite existing run: {run_dir}")
        remaining = deadline - time.monotonic()
        if unavailable_reason or attempted >= args.max_requests or remaining < 5:
            write_json(
                run_dir / "run.json",
                {
                    "status": "not_run",
                    "reason": unavailable_reason or "Global benchmark budget exhausted",
                    "stage": "availability" if unavailable_reason else "budget",
                    "elapsed_seconds": None,
                },
            )
            continue
        image = args.manifest.parent / case["image"]
        digest = hashlib.sha256(image.read_bytes()).hexdigest()
        if digest != case["image_sha256"]:
            raise RuntimeError(f"Input checksum changed: {case['id']}")
        seconds = max(1, min(args.request_seconds, int(remaining)))
        command = [
            str(args.runner),
            "--image",
            str(image),
            "--output",
            str(run_dir),
            "--model",
            args.model,
            "--effort",
            args.effort,
            "--seconds",
            str(seconds),
        ]
        attempted += 1
        started = time.monotonic()
        print(
            f"{case['id']} run {repeat}: attempt {attempted}/{args.max_requests}, limit {seconds}s",
            flush=True,
        )
        try:
            result = bounded_run(command, min(remaining, seconds + 5), environment)
            (run_dir / "stdout.txt").write_text(result.stdout)
            (run_dir / "stderr.txt").write_text(result.stderr)
            if not (run_dir / "run.json").exists():
                write_json(
                    run_dir / "run.json",
                    {
                        "status": "failed",
                        "stage": "runner_preflight",
                        "error": result.stderr[:1000],
                        "elapsed_seconds": time.monotonic() - started,
                        "exit_code": result.returncode,
                        "configuration": {
                            "requested_model": args.model,
                            "requested_effort": args.effort,
                        },
                    },
                )
        except subprocess.TimeoutExpired:
            write_json(
                run_dir / "run.json",
                {
                    "status": "timeout",
                    "stage": "runner_watchdog",
                    "error": "Benchmark budget exhausted",
                    "elapsed_seconds": time.monotonic() - started,
                    "configuration": {
                        "requested_model": args.model,
                        "requested_effort": args.effort,
                    },
                },
            )
        run = json.loads((run_dir / "run.json").read_text())
        if run["status"] == "not_run" and run.get("stage") == "availability":
            unavailable_reason = run["reason"]
        else:
            # A preflight failure may still have no inference. Retain the conservative
            # attempted-workflow count; explicit Started events are authoritative.
            if run.get("configuration", {}).get("generation_started"):
                inference_workflows += 1
        print(run["status"], flush=True)
    metadata["finished_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
    metadata["runner_invocations"] = attempted
    metadata["inference_workflows_started"] = inference_workflows
    metadata["availability_blocked_reason"] = unavailable_reason
    metadata["elapsed_seconds"] = time.monotonic() - overall_started
    write_json(args.output / "metadata.json", metadata)
    print(
        f"Benchmark finished with {attempted} runner invocations and {inference_workflows} observed inference workflows. No drawings were applied.",
        flush=True,
    )


if __name__ == "__main__":
    main()
