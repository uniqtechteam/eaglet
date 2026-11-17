# Benchmark: eaglet against plain Axum

eaglet builds a plain `axum::Router`, so the question is what it adds on top. This folder answers it
with the same three routes written twice, once in plain Axum and once as an eaglet module, and a
script that loads both.

![Requests per second and cost per request, plain Axum against eaglet](../docs/img/bench-axum.svg)

## Results

Measured on 2026-11-17 on an Intel i7-12700 (Linux 7.0, rustc 1.100.0, oha 1.17.0). Medians of five
rounds; every round is in [results.json](results.json), the medians in [results.csv](results.csv).

| Route | Plain Axum | eaglet | Difference |
|---|---|---|---|
| `GET /users/:id` | 464,710 req/s, 604 ns | 421,057 req/s, 792 ns | -9.4%, +188 ns |
| `POST /users` (validated) | 371,694 req/s, 1,553 ns | 359,468 req/s, 1,695 ns | -3.3%, +142 ns |
| `GET /me` (guarded) | 441,482 req/s, 979 ns | 386,264 req/s, 1,126 ns | -12.5%, +147 ns |

Requests per second are over HTTP with the server held to 4 logical CPUs; nanoseconds are the
framework's own cost per request, called in memory on one core.

**eaglet costs about 150 to 190 ns per request over plain Axum**, measured in memory. The first
version of this bench measured about 500 ns (1,135 ns against 634 ns on the `GET` route); the
section below says where that went.

The requests per second do not show that change cleanly. The machine was shared with other work
(load average 4.8 when the run started, 7.6 at the end), and single rounds of the same server
moved by up to 35%: `GET /users/:id` on plain Axum ranged from 311,742 to 505,701 req/s. A 2%
difference cannot be read from that, so the in-memory column is the one to trust here. Median
latency over HTTP is 127 µs against 135 µs on the `GET` route.

### Where the time went

The extra cost was the same on every route, so it came from the request layer every eaglet app runs
first (trace id, `x-request-id`, error envelope), not from guards or validation. Taking that layer
out made the `GET` route as fast as plain Axum. The layer was slow for ordinary reasons:

- It was written with `axum::middleware::from_fn`, which clones and boxes the next service and
  boxes the future on every request.
- It built each id with a new `RandomState`, formatted it into a `String`, cloned that twice for the
  task-local and copied it again into the header value.
- It entered the task-local twice, once for the handler and once for the error check.

It is now a small tower `Service` with its own future. A fresh id is a `u64` from a per-thread
SplitMix64 and only becomes text when something asks for it; an incoming id keeps the header value
it came with. The route future is polled in place with the id in scope, and the body is only read
when an error response has to become the envelope.

What is left is mostly what any `Router::layer` costs in Axum: each layered route is boxed again, so
a request pays for one more clone and one more boxed future. A layer that does nothing at all
measured about 100 ns on this machine, and the eaglet layer about 60 ns more than that, most of it
the one allocation for the `x-request-id` header value. An Axum app that adds request ids with a
layer of its own pays the same.

For an endpoint that reads a database or calls another service, 150 ns is lost in the noise. For
a service whose whole job is answering trivial routes as fast as possible, plain Axum is still a
little faster.

## The routes

| Route | What both versions do |
|---|---|
| `GET /users/:id` | path parameter in, a small JSON object out |
| `POST /users` | JSON body checked with `validator` (name length, email), 201 with the user, 422 if invalid |
| `GET /me` | a bearer-token guard runs first and puts the user on the request; the handler returns it |

Both versions call the same `Users` service through an `Arc`. The plain one validates in the
handler and guards with a `route_layer`, the way a careful Axum app would. The eaglet one uses
`#[controller]`, `Valid<Json<T>>` and `#[guard]`. `tests/same_answers.rs` checks that both answer
every bench request with the same status and body, so the numbers compare the same work.

## Method

- `bench-server axum|eaglet <port>` serves one version on `127.0.0.1`.
- `run.py` starts each server pinned with `taskset` to CPUs 0-3 and runs `oha` pinned to CPUs 4-11,
  64 connections, 3 seconds of warm-up and then 10 seconds measured. Each round runs every route on
  both servers, and the order flips every round, so a busy moment on the machine hits both sides.
  It stops if any response has an unexpected status.
- `bench-inproc` calls both routers with `tower::ServiceExt::oneshot`, no sockets, 200,000 requests
  per sample, seven samples, and reports the median nanoseconds per request. This is the cleanest
  view of the framework's own cost, because the network stack and the load generator are out of it.
- `plot.py` draws `docs/img/bench-axum.svg` from `results.json`.

## Run it

```bash
cargo install oha --locked
cargo build --release -p eaglet-bench
python3 bench/run.py      # about seven minutes; writes results.json and results.csv
python3 bench/plot.py     # redraws docs/img/bench-axum.svg
```

`run.py` takes the release folder as its argument when `CARGO_TARGET_DIR` points elsewhere, and
reads `BENCH_SERVER_CPUS`, `BENCH_LOAD_CPUS`, `BENCH_CONNECTIONS`, `BENCH_SECONDS` and
`BENCH_ROUNDS` from the environment.

## Caveats

- The server and the load generator share one machine. They are on separate cores, but they share
  caches and memory bandwidth, and other work was running on the machine (load average 4.8 when the
  run started, 7.6 at the end). Over HTTP, single rounds moved by up to 35% when the machine got
  busy, which is more than the difference being measured; all of them are in `results.json`. The
  in-memory numbers agree within a few percent from round to round.
- The routes do almost nothing on purpose, which makes the framework's share as large as it can be.
  Real handlers make it smaller.
- One machine, one OS. Numbers on other hardware will differ; the gap in nanoseconds is the part to
  compare.
