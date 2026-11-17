#!/usr/bin/env python3
"""Load both versions of the bench app with oha and save the numbers.

    cargo install oha --locked
    cargo build --release -p eaglet-bench
    python3 bench/run.py [path/to/release/dir]

Two measurements:

1. Over HTTP: each server in turn, loaded by oha for a fixed time. Requests per second and latency.
2. In memory: bench-inproc calls both routers with no sockets and prints nanoseconds per request,
   which is the framework's own cost without the network stack and oha in the picture.

Writes bench/results.json (every round, plus the machine) and bench/results.csv (the medians).
Then `python3 bench/plot.py` draws docs/img/bench-axum.svg from the JSON.

The server and oha run on the same machine, pinned to separate cores with taskset, so they do not
fight for the same CPUs. Rounds alternate between the two servers, so a slow moment on the machine
hits both sides instead of one.
"""
import csv
import json
import os
import platform
import shutil
import socket
import statistics
import subprocess
import sys
import time
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
RELEASE = Path(sys.argv[1]) if len(sys.argv) > 1 else TARGET / "release"
SERVER = RELEASE / "bench-server"
INPROC = RELEASE / "bench-inproc"
PORT = 3917
SERVER_CPUS = os.environ.get("BENCH_SERVER_CPUS", "0-3")
LOAD_CPUS = os.environ.get("BENCH_LOAD_CPUS", "4-11")
CONNECTIONS = int(os.environ.get("BENCH_CONNECTIONS", "64"))
SECONDS = int(os.environ.get("BENCH_SECONDS", "10"))
WARMUP = 3
ROUNDS = int(os.environ.get("BENCH_ROUNDS", "5"))
FRAMEWORKS = ["axum", "eaglet"]

# name, bench-inproc's name for it, oha arguments (path first), the status every response must have
CASES = [
    ("GET /users/:id", "get", ["/users/42"], "200"),
    (
        "POST /users (validated)",
        "post",
        ["/users", "-m", "POST", "-T", "application/json",
         "-d", '{"name":"Ana","email":"ana@example.com"}'],
        "201",
    ),
    ("GET /me (guarded)", "me", ["/me", "-H", "Authorization: Bearer bench-token"], "200"),
]


def cpu_name() -> str:
    for line in Path("/proc/cpuinfo").read_text().splitlines():
        if line.startswith("model name"):
            return line.split(":", 1)[1].strip()
    return platform.processor() or platform.machine()


def version(cmd: list[str]) -> str:
    return subprocess.run(cmd, capture_output=True, text=True, check=True).stdout.strip()


def wait_for_port() -> None:
    for _ in range(100):
        try:
            socket.create_connection(("127.0.0.1", PORT), timeout=0.1).close()
            return
        except OSError:
            time.sleep(0.05)
    sys.exit("the bench server did not start")


def oha(args: list[str], seconds: int) -> dict:
    path, *rest = args
    cmd = [
        "taskset", "-c", LOAD_CPUS, "oha", f"http://127.0.0.1:{PORT}{path}",
        "-z", f"{seconds}s", "-c", str(CONNECTIONS), "--no-tui", "--output-format", "json", *rest,
    ]
    return json.loads(subprocess.run(cmd, capture_output=True, text=True, check=True).stdout)


def measure(framework: str, args: list[str], status: str) -> dict:
    server = subprocess.Popen(
        ["taskset", "-c", SERVER_CPUS, str(SERVER), framework, str(PORT)],
        stdout=subprocess.DEVNULL,
    )
    try:
        wait_for_port()
        oha(args, WARMUP)
        out = oha(args, SECONDS)
    finally:
        server.terminate()
        server.wait()
    codes = out["statusCodeDistribution"]
    # oha cuts the requests still in flight when the time is up; those are not server errors
    errors = {k: v for k, v in out["errorDistribution"].items() if "deadline" not in k}
    if set(codes) != {status} or errors:
        sys.exit(f"{framework} {args[0]}: unexpected answers {codes} {errors}")
    p = out["latencyPercentiles"]
    return {
        "rps": round(out["summary"]["requestsPerSec"]),
        "p50_us": round(p["p50"] * 1e6, 1),
        "p99_us": round(p["p99"] * 1e6, 1),
        "requests": codes[status],
    }


def main() -> None:
    if not shutil.which("oha"):
        sys.exit("oha not found: cargo install oha --locked")
    if not (SERVER.exists() and INPROC.exists()):
        sys.exit(f"no bench binaries in {RELEASE}: cargo build --release -p eaglet-bench")
    machine = {
        "cpu": cpu_name(),
        "cores": os.cpu_count(),
        "os": f"{platform.system()} {platform.release()}",
        "rustc": version(["rustc", "--version"]),
        "oha": version(["oha", "--version"]),
        "server_cpus": SERVER_CPUS,
        "load_cpus": LOAD_CPUS,
        "connections": CONNECTIONS,
        "seconds": SECONDS,
        "rounds": ROUNDS,
        "load_average_before": os.getloadavg()[0],
        "date": date.today().isoformat(),
    }
    rounds = []
    for n in range(ROUNDS):
        order = FRAMEWORKS if n % 2 == 0 else FRAMEWORKS[::-1]
        for name, _, args, status in CASES:
            for framework in order:
                result = measure(framework, args, status)
                rounds.append({"round": n + 1, "case": name, "framework": framework, **result})
                print(f"round {n + 1} {name:<26} {framework:<7} {result['rps']:>8} req/s "
                      f"p50 {result['p50_us']:>6} us  p99 {result['p99_us']:>7} us", flush=True)
    inproc = []
    for n in range(ROUNDS):
        out = subprocess.run(
            ["taskset", "-c", SERVER_CPUS.split("-")[0], str(INPROC)],
            capture_output=True, text=True, check=True,
        ).stdout
        for line in out.splitlines():
            row = json.loads(line)
            inproc.append({"round": n + 1, **row})
            print(f"round {n + 1} in memory {row['case']:<5} {row['framework']:<7} {row['ns']:>6} ns",
                  flush=True)
    machine["load_average_after"] = os.getloadavg()[0]

    summary = []
    for name, short, _, _ in CASES:
        row = {"case": name}
        for framework in FRAMEWORKS:
            mine = [r for r in rounds if r["case"] == name and r["framework"] == framework]
            for key in ("rps", "p50_us", "p99_us"):
                row[f"{framework}_{key}"] = statistics.median(r[key] for r in mine)
            row[f"{framework}_inproc_ns"] = statistics.median(
                r["ns"] for r in inproc if r["case"] == short and r["framework"] == framework
            )
        row["eaglet_vs_axum_rps"] = round(row["eaglet_rps"] / row["axum_rps"] - 1, 4)
        row["eaglet_extra_ns"] = row["eaglet_inproc_ns"] - row["axum_inproc_ns"]
        summary.append(row)

    bench = ROOT / "bench"
    (bench / "results.json").write_text(
        json.dumps(
            {"machine": machine, "summary": summary, "rounds": rounds, "inproc": inproc}, indent=2
        ) + "\n"
    )
    with (bench / "results.csv").open("w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=list(summary[0]), lineterminator="\n")
        writer.writeheader()
        writer.writerows(summary)
    for row in summary:
        print(f"{row['case']:<26} axum {row['axum_rps']:>8.0f}  eaglet {row['eaglet_rps']:>8.0f} "
              f"req/s ({row['eaglet_vs_axum_rps']:+.1%}), in memory "
              f"{row['axum_inproc_ns']:.0f} vs {row['eaglet_inproc_ns']:.0f} ns")


if __name__ == "__main__":
    main()
