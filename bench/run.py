#!/usr/bin/env python3
"""Load every version of the bench app with oha and save the numbers.

    cargo install oha --locked
    cargo build --release -p eaglet-bench
    (cd bench/nestjs && npm ci && npm run build)
    python3 bench/run.py [path/to/release/dir]
    python3 bench/run.py --check [path/to/release/dir]   # only check the answers

Four servers answer the same three routes: plain Axum, eaglet, NestJS on Express and NestJS on
Fastify. Before anything is measured, every server gets the bench requests plus the error cases,
and the run stops unless each one answers like plain Axum (same status and body; for errors the
same status and `code`).

Three measurements:

1. Over HTTP, two setups. "1 CPU": the server pinned to one CPU, Rust with one Tokio worker
   thread, Node as one process. "4 CPUs": the server pinned to four CPUs, Rust with four worker
   threads, Node with node:cluster and four worker processes. Requests per second, p50 and p99.
2. Memory: the resident set size of the whole server (every process of a Node cluster) once it
   has started, and the peak while oha loads it.
3. In memory: bench-inproc calls the two Rust routers with no sockets and prints nanoseconds per
   request, which is the framework's own cost without the network stack and oha in the picture.

Writes bench/results.json (every round, plus the machine), bench/results.csv (plain Axum against
eaglet, medians) and bench/results-frameworks.csv (all four servers, medians). Then
`python3 bench/plot.py` draws both charts in docs/img/ from the JSON.

The server and oha run on the same machine, pinned to separate cores with taskset. Each round
starts at a different server, so a busy moment on the machine does not always hit the same one,
and a round waits for the load average to drop below BENCH_MAX_LOAD (for up to two minutes).
"""
import csv
import json
import os
import platform
import shutil
import signal
import socket
import statistics
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ARGS = [a for a in sys.argv[1:] if not a.startswith("--")]
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
RELEASE = Path(ARGS[0]) if ARGS else TARGET / "release"
SERVER = RELEASE / "bench-server"
INPROC = RELEASE / "bench-inproc"
NEST = ROOT / "bench" / "nestjs"
PORT = 3917
SERVER_CPUS = os.environ.get("BENCH_SERVER_CPUS", "0-3")
ONE_CPU = os.environ.get("BENCH_ONE_CPU", SERVER_CPUS.split("-")[0])
LOAD_CPUS = os.environ.get("BENCH_LOAD_CPUS", "4-11")
CONNECTIONS = int(os.environ.get("BENCH_CONNECTIONS", "64"))
SECONDS = int(os.environ.get("BENCH_SECONDS", "10"))
WARMUP = 3
ROUNDS = int(os.environ.get("BENCH_ROUNDS", "5"))
MAX_LOAD = float(os.environ.get("BENCH_MAX_LOAD", "4"))
FRAMEWORKS = ["axum", "eaglet", "nest-express", "nest-fastify"]


def cpu_count(cpus: str) -> int:
    total = 0
    for part in cpus.split(","):
        first, _, last = part.partition("-")
        total += int(last or first) - int(first) + 1
    return total


# name, CPUs, Tokio worker threads / Node processes
SETUPS = [("1 CPU", ONE_CPU, 1), (f"{cpu_count(SERVER_CPUS)} CPUs", SERVER_CPUS, cpu_count(SERVER_CPUS))]

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

# what --check sends: method, path, headers, body, and whether the whole body must match
ANSWERS = [
    ("GET", "/users/42", {}, None, True),
    ("POST", "/users", {"Content-Type": "application/json"},
     '{"name":"Ana","email":"ana@example.com"}', True),
    ("GET", "/me", {"Authorization": "Bearer bench-token"}, None, True),
    ("POST", "/users", {"Content-Type": "application/json"},
     '{"name":"Ana","email":"not-an-email"}', False),
    ("POST", "/users", {"Content-Type": "application/json"}, '{"name":"","email":"a@b.co"}', False),
    ("GET", "/me", {}, None, False),
    ("GET", "/me", {"Authorization": "Bearer wrong"}, None, False),
]


def cpu_name() -> str:
    for line in Path("/proc/cpuinfo").read_text().splitlines():
        if line.startswith("model name"):
            return line.split(":", 1)[1].strip()
    return platform.processor() or platform.machine()


def version(cmd: list[str]) -> str:
    return subprocess.run(cmd, capture_output=True, text=True, check=True).stdout.strip()


def package_version(name: str) -> str:
    return json.loads((NEST / "node_modules" / name / "package.json").read_text())["version"]


def wait_for_port() -> None:
    for _ in range(200):
        try:
            socket.create_connection(("127.0.0.1", PORT), timeout=0.1).close()
            return
        except OSError:
            time.sleep(0.05)
    sys.exit("the bench server did not start")


def wait_until_quiet() -> float:
    """Wait (up to two minutes) for the 1-minute load average to drop below MAX_LOAD."""
    deadline = time.monotonic() + 120
    while os.getloadavg()[0] >= MAX_LOAD and time.monotonic() < deadline:
        time.sleep(5)
    return round(os.getloadavg()[0], 2)


def rss_mb(pid: int) -> float:
    """Resident memory of a process and all its children (a Node cluster is several processes)."""
    total, todo = 0, [pid]
    while todo:
        p = todo.pop()
        try:
            for line in Path(f"/proc/{p}/status").read_text().splitlines():
                if line.startswith("VmRSS:"):
                    total += int(line.split()[1])
            for task in Path(f"/proc/{p}/task").iterdir():
                todo += [int(c) for c in (task / "children").read_text().split()]
        except (FileNotFoundError, ProcessLookupError):
            continue
    return round(total / 1024, 1)


def start(framework: str, cpus: str, workers: int) -> subprocess.Popen:
    if framework.startswith("nest-"):
        cmd = ["node", str(NEST / "dist" / "main.js"), framework[5:], str(PORT), str(workers)]
        env = {**os.environ, "NODE_ENV": "production"}
    else:
        cmd = [str(SERVER), framework, str(PORT)]
        env = {**os.environ, "TOKIO_WORKER_THREADS": str(workers)}
    server = subprocess.Popen(
        ["taskset", "-c", cpus, *cmd], stdout=subprocess.DEVNULL, env=env, start_new_session=True
    )
    wait_for_port()
    # a Node cluster listens as soon as its first worker does; give the others time to start
    deadline = time.monotonic() + 10
    while framework.startswith("nest-") and workers > 1 and time.monotonic() < deadline:
        children = Path(f"/proc/{server.pid}/task/{server.pid}/children").read_text().split()
        if len(children) >= workers:
            break
        time.sleep(0.1)
    time.sleep(1)
    return server


def stop(server: subprocess.Popen) -> None:
    os.killpg(server.pid, signal.SIGTERM)
    server.wait()


def oha(args: list[str], seconds: int) -> dict:
    path, *rest = args
    cmd = [
        "taskset", "-c", LOAD_CPUS, "oha", f"http://127.0.0.1:{PORT}{path}",
        "-z", f"{seconds}s", "-c", str(CONNECTIONS), "--no-tui", "--output-format", "json", *rest,
    ]
    return json.loads(subprocess.run(cmd, capture_output=True, text=True, check=True).stdout)


def measure(framework: str, cpus: str, workers: int, args: list[str], status: str) -> dict:
    server = start(framework, cpus, workers)
    peak = [0.0]
    done = threading.Event()

    def sample() -> None:
        while not done.wait(0.25):
            peak[0] = max(peak[0], rss_mb(server.pid))

    try:
        rest = rss_mb(server.pid)
        oha(args, WARMUP)
        sampler = threading.Thread(target=sample)
        sampler.start()
        try:
            out = oha(args, SECONDS)
        finally:
            done.set()
            sampler.join()
    finally:
        stop(server)
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
        "rss_rest_mb": rest,
        "rss_load_mb": peak[0],
    }


def answer(method: str, path: str, headers: dict, body: str | None) -> tuple[int, dict]:
    req = urllib.request.Request(
        f"http://127.0.0.1:{PORT}{path}", method=method, headers=headers,
        data=body.encode() if body else None,
    )
    try:
        with urllib.request.urlopen(req) as res:
            return res.status, json.loads(res.read())
    except urllib.error.HTTPError as err:
        return err.code, json.loads(err.read())


def check_answers() -> None:
    """Every server must answer every request like plain Axum, or the numbers compare different work."""
    answers = {}
    for framework in FRAMEWORKS:
        server = start(framework, SERVER_CPUS, 2)
        try:
            answers[framework] = [answer(*a[:4]) for a in ANSWERS]
        finally:
            stop(server)
    for framework in FRAMEWORKS[1:]:
        for (method, path, _, body, whole), want, got in zip(
            ANSWERS, answers["axum"], answers[framework], strict=True
        ):
            same = got == want if whole else (got[0], got[1]["code"]) == (want[0], want[1]["code"])
            if not same:
                sys.exit(f"{framework} {method} {path} {body or ''}: {got}, plain Axum: {want}")
    print(f"same answers from {', '.join(FRAMEWORKS)} for {len(ANSWERS)} requests", flush=True)


def summarise(rounds: list[dict], inproc: list[dict]) -> tuple[list[dict], list[dict]]:
    frameworks = []
    for setup, _, _ in SETUPS:
        for name, _, _, _ in CASES:
            for framework in FRAMEWORKS:
                mine = [r for r in rounds
                        if (r["setup"], r["case"], r["framework"]) == (setup, name, framework)]
                row = {"setup": setup, "case": name, "framework": framework}
                for key in ("rps", "p50_us", "p99_us", "rss_rest_mb", "rss_load_mb"):
                    row[key] = statistics.median(r[key] for r in mine)
                frameworks.append(row)
    # plain Axum against eaglet, on the multi-CPU setup, as the first version of this bench had it
    summary = []
    for name, short, _, _ in CASES:
        row = {"case": name}
        for framework in ("axum", "eaglet"):
            (mine,) = [r for r in frameworks if (r["setup"], r["case"], r["framework"])
                       == (SETUPS[1][0], name, framework)]
            for key in ("rps", "p50_us", "p99_us"):
                row[f"{framework}_{key}"] = mine[key]
            row[f"{framework}_inproc_ns"] = statistics.median(
                r["ns"] for r in inproc if r["case"] == short and r["framework"] == framework
            )
        row["eaglet_vs_axum_rps"] = round(row["eaglet_rps"] / row["axum_rps"] - 1, 4)
        row["eaglet_extra_ns"] = row["eaglet_inproc_ns"] - row["axum_inproc_ns"]
        summary.append(row)
    return summary, frameworks


def write_csv(path: Path, rows: list[dict]) -> None:
    with path.open("w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=list(rows[0]), lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)


def main() -> None:
    if not shutil.which("oha"):
        sys.exit("oha not found: cargo install oha --locked")
    if not (SERVER.exists() and INPROC.exists()):
        sys.exit(f"no bench binaries in {RELEASE}: cargo build --release -p eaglet-bench")
    if not (NEST / "dist" / "main.js").exists():
        sys.exit("no NestJS build: cd bench/nestjs && npm ci && npm run build")
    check_answers()
    if "--check" in sys.argv:
        return
    machine = {
        "cpu": cpu_name(),
        "cores": os.cpu_count(),
        "os": f"{platform.system()} {platform.release()}",
        "rustc": version(["rustc", "--version"]),
        "node": version(["node", "--version"]),
        "nestjs": package_version("@nestjs/core"),
        "express": package_version("express"),
        "fastify": package_version("fastify"),
        "oha": version(["oha", "--version"]),
        "setups": [{"name": s, "cpus": c, "workers": w} for s, c, w in SETUPS],
        "load_cpus": LOAD_CPUS,
        "connections": CONNECTIONS,
        "seconds": SECONDS,
        "rounds": ROUNDS,
        "max_load": MAX_LOAD,
        "load_average_before": os.getloadavg()[0],
        "date": date.today().isoformat(),
    }
    rounds = []
    for n in range(ROUNDS):
        order = FRAMEWORKS[n % len(FRAMEWORKS):] + FRAMEWORKS[:n % len(FRAMEWORKS)]
        load = wait_until_quiet()
        for setup, cpus, workers in SETUPS:
            for name, _, args, status in CASES:
                for framework in order:
                    result = measure(framework, cpus, workers, args, status)
                    rounds.append({"round": n + 1, "load": load, "setup": setup, "case": name,
                                   "framework": framework, **result})
                    print(f"round {n + 1} {setup:<6} {name:<24} {framework:<12} "
                          f"{result['rps']:>8} req/s p50 {result['p50_us']:>7} us "
                          f"p99 {result['p99_us']:>8} us  {result['rss_rest_mb']:>6} / "
                          f"{result['rss_load_mb']:>6} MB  (load {load})", flush=True)
    inproc = []
    for n in range(ROUNDS):
        out = subprocess.run(
            ["taskset", "-c", ONE_CPU, str(INPROC)], capture_output=True, text=True, check=True
        ).stdout
        for line in out.splitlines():
            row = json.loads(line)
            inproc.append({"round": n + 1, **row})
            print(f"round {n + 1} in memory {row['case']:<5} {row['framework']:<7} {row['ns']:>6} ns",
                  flush=True)
    machine["load_average_after"] = os.getloadavg()[0]

    summary, frameworks = summarise(rounds, inproc)
    bench = ROOT / "bench"
    (bench / "results.json").write_text(
        json.dumps(
            {"machine": machine, "summary": summary, "frameworks": frameworks, "rounds": rounds,
             "inproc": inproc},
            indent=2,
        ) + "\n"
    )
    write_csv(bench / "results.csv", summary)
    write_csv(bench / "results-frameworks.csv", frameworks)
    for row in frameworks:
        print(f"{row['setup']:<6} {row['case']:<24} {row['framework']:<12} {row['rps']:>8.0f} req/s "
              f"p99 {row['p99_us']:>8} us  {row['rss_rest_mb']:>6} / {row['rss_load_mb']:>6} MB")


if __name__ == "__main__":
    main()
