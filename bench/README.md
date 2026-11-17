# Benchmark: eaglet against plain Axum and NestJS

eaglet builds a plain `axum::Router`, so the first question is what it adds on top. The second is
what moving a NestJS service to it changes. This folder answers both with the same three routes
written four ways: plain Axum, an eaglet module, and a NestJS 11 app run on Express and on Fastify,
plus a script that loads them all under the same conditions.

![Requests per second for plain Axum, eaglet, NestJS with Express and NestJS with Fastify](../docs/img/bench-frameworks.svg)

## Results against NestJS

Measured on 2026-11-17 on an Intel i7-12700 (Linux 7.0, rustc 1.100.0, Node 22.23.5, NestJS
11.2.10 with Express 5.2.1 and Fastify 5.12.8, oha 1.17.0). Medians of five rounds; every round is
in [results.json](results.json), the medians in [results-frameworks.csv](results-frameworks.csv).

Node runs JavaScript on one thread per process, and a Rust server uses every core it is given, so
there is no single fair setup. The bench runs two and reports both:

- **1 CPU**: the server pinned to one logical CPU. Rust gets one Tokio worker thread, Node one
  process. Node's garbage collector and libuv threads share that CPU, as Tokio's do.
- **4 CPUs**: the server pinned to four logical CPUs (two physical cores with hyper-threading).
  Rust gets four worker threads; Node gets `node:cluster` with four worker processes, which is how
  Nest is usually scaled on one machine (pm2 cluster mode does the same).

Requests per second, latency at p50 and p99 under 64 connections, and memory (resident set size of
the whole server, every process of a cluster included) once started and at its peak under load.

**1 CPU**

| Route | Plain Axum | eaglet | NestJS + Express | NestJS + Fastify |
|---|---|---|---|---|
| `GET /users/:id` | 216,008 req/s | 204,634 req/s | 15,115 req/s | 71,076 req/s |
| p50 / p99 | 297 / 354 µs | 314 / 444 µs | 3,582 / 13,212 µs | 869 / 1,399 µs |
| `POST /users` (validated) | 160,287 req/s | 165,219 req/s | 10,250 req/s | 40,630 req/s |
| p50 / p99 | 376 / 742 µs | 384 / 473 µs | 5,398 / 17,067 µs | 1,512 / 2,637 µs |
| `GET /me` (guarded) | 199,254 req/s | 190,415 req/s | 14,940 req/s | 71,926 req/s |
| p50 / p99 | 322 / 385 µs | 335 / 502 µs | 3,588 / 12,518 µs | 833 / 1,569 µs |
| Memory at rest / under load | 3.4 / 5.6 MB | 3.6 / 5.8 MB | 85 / 180 MB | 86 / 114 MB |

**4 CPUs**

| Route | Plain Axum | eaglet | NestJS + Express | NestJS + Fastify |
|---|---|---|---|---|
| `GET /users/:id` | 501,367 req/s | 467,635 req/s | 28,290 req/s | 159,834 req/s |
| p50 / p99 | 125 / 240 µs | 132 / 322 µs | 1,975 / 6,815 µs | 398 / 788 µs |
| `POST /users` (validated) | 387,178 req/s | 343,227 req/s | 18,751 req/s | 73,209 req/s |
| p50 / p99 | 158 / 338 µs | 168 / 563 µs | 3,331 / 9,130 µs | 861 / 1,485 µs |
| `GET /me` (guarded) | 455,378 req/s | 443,575 req/s | 28,373 req/s | 169,703 req/s |
| p50 / p99 | 136 / 325 µs | 141 / 269 µs | 2,064 / 7,017 µs | 388 / 743 µs |
| Memory at rest / under load | 3.5 / 6.7 MB | 3.6 / 6.6 MB | 413 / 767 MB | 422 / 517 MB |

Memory is the highest of the three routes, in MB of resident set size.

What this says, for these routes on this machine:

- With the same CPU, eaglet answered 2.6 to 2.9 times as many requests as NestJS on Fastify for the
  two `GET` routes and 4.1 to 4.7 times as many for the validated `POST`. Against NestJS on the
  default Express adapter it was about 13 to 18 times.
- For a Nest app, Fastify is the bigger step: 5 to 6 times Express on the `GET` routes and 4 times
  on `POST`, with a p99 6 to 9 times lower.
- The validated `POST` costs Nest the most. On Fastify it gets about half the requests of the
  plain `GET`; the Rust servers lose 19 to 27% on the same route. Both numbers include parsing the
  JSON body, and on the Nest side `class-transformer` and `class-validator` running in
  `ValidationPipe`.
- A Rust server held 3 to 4 MB at rest and under 7 MB under load. A Nest process held about 85 MB
  at rest, so a four-worker cluster sat at about 415 MB before the first request.

The Node numbers are not a ceiling for Node. They are what a Nest 11 app written the usual way
gets, compiled with `tsc` and run with `NODE_ENV=production node dist/main.js`, logger off.

## Results against plain Axum

The same run, server on 4 CPUs for requests per second; nanoseconds are the framework's own cost
per request, called in memory on one core. The medians are in [results.csv](results.csv).

![Requests per second and cost per request, plain Axum against eaglet](../docs/img/bench-axum.svg)

| Route | Plain Axum | eaglet | Difference |
|---|---|---|---|
| `GET /users/:id` | 501,367 req/s, 601 ns | 467,635 req/s, 773 ns | -6.7%, +172 ns |
| `POST /users` (validated) | 387,178 req/s, 1,514 ns | 343,227 req/s, 1,716 ns | -11.4%, +202 ns |
| `GET /me` (guarded) | 455,378 req/s, 976 ns | 443,575 req/s, 1,124 ns | -2.6%, +148 ns |

**eaglet costs about 150 to 200 ns per request over plain Axum**, measured in memory. The first
version of this bench measured about 500 ns (1,135 ns against 634 ns on the `GET` route); the
section below says where that went.

The requests per second do not show that difference cleanly. The machine was shared with other
work, and single rounds of the same Rust server moved by a lot: `GET /me` on plain Axum ranged from
229,170 to 465,461 req/s over the five rounds, and `GET /users/:id` on eaglet from 242,391 to
478,315. A 5% difference cannot be read from that, so the in-memory column is the one to trust
here. Median latency over HTTP is 125 µs against 132 µs on the `GET` route.

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

The NestJS version in [nestjs/](nestjs) is written the way a Nest app usually is: one module, a
`Users` provider, two controllers, a DTO with `@Length(1, 64)` and `@IsEmail()` behind a global
`ValidationPipe`, and a `CanActivate` guard on `/me` that puts the user on the request. Its
validation errors are 422 `VALIDATION_FAILED` and its guard answers 401 `UNAUTHORIZED`, like the
Rust versions; Nest's defaults would be 400 and 403. Before measuring anything, `run.py` sends all
four servers the three bench requests and four error cases, and stops unless each one answers like
plain Axum: the same status and body, and for errors the same status and `code`.

What each server does beyond the route differs a little, and none of it was turned off. eaglet
adds an `x-request-id` header to every response; Express adds `ETag` and `X-Powered-By`.

## Method

- `bench-server axum|eaglet <port>` serves one Rust version on `127.0.0.1`;
  `node bench/nestjs/dist/main.js express|fastify <port> <workers>` serves the NestJS one.
- `run.py` starts a fresh server for every measurement, pinned with `taskset` to CPU 0 or CPUs
  0-3, with `TOKIO_WORKER_THREADS` set to 1 or 4 for Rust and 1 process or a 4-worker cluster for
  Node. `oha` runs pinned to CPUs 4-11, 64 connections, 3 seconds of warm-up and then 10 seconds
  measured. Each round runs every route on all four servers in both setups, and starts at a
  different server each round, so a busy moment on the machine does not always hit the same one.
  A round waits up to two minutes for the 1-minute load average to drop below 4. It stops if any
  response has an unexpected status.
- Memory is `VmRSS` from `/proc`, summed over the server and its child processes: once a second
  after it starts listening, and the peak of samples taken every 250 ms during the measured run.
- `bench-inproc` calls both routers with `tower::ServiceExt::oneshot`, no sockets, 200,000 requests
  per sample, seven samples, and reports the median nanoseconds per request. This is the cleanest
  view of the framework's own cost, because the network stack and the load generator are out of it.
- `plot.py` draws `docs/img/bench-frameworks.svg` and `docs/img/bench-axum.svg` from
  `results.json`.

## Run it

```bash
cargo install oha --locked
cargo build --release -p eaglet-bench
(cd bench/nestjs && npm ci && npm run build)
python3 bench/run.py --check   # only check that all four servers answer alike
python3 bench/run.py           # about 35 minutes; writes results.json and both CSV files
python3 bench/plot.py          # redraws both charts in docs/img/
```

`run.py` takes the release folder as its argument when `CARGO_TARGET_DIR` points elsewhere, and
reads `BENCH_SERVER_CPUS`, `BENCH_ONE_CPU`, `BENCH_LOAD_CPUS`, `BENCH_CONNECTIONS`,
`BENCH_SECONDS`, `BENCH_ROUNDS` and `BENCH_MAX_LOAD` from the environment.

## Caveats

- The server and the load generator share one machine. They are on separate cores, but they share
  caches and memory bandwidth, and other work was running on the machine. The load average was
  13.3 when the run started and 4.0 at the end; rounds 1 and 2 started at 11.2 and 7.3 after the
  two-minute wait ran out, rounds 3 to 5 at 3.9 to 4.0. Single rounds of the same server moved by
  up to 100% for the Rust servers and up to 41% for Fastify; every round is in `results.json`. The
  in-memory numbers agree within a few percent from round to round.
- The gap between the Rust servers and NestJS (3 to 18 times) is much larger than that noise. The
  gap between plain Axum and eaglet over HTTP is not, which is why that comparison leans on the
  in-memory numbers.
- 64 connections on one Node process queue up behind its single thread, which is most of Nest's
  p99. Fewer connections give Node a lower p99 and similar throughput.
- In the cluster, the primary process hands connections to the workers (Node's default round-robin
  on Linux) and shares the four CPUs with them. Its memory is counted.
- One Node version (22) and one Nest version (11). A newer Node, or a different validation library,
  will move the Nest numbers; `run.py` records the versions with every run.
- The routes do almost nothing on purpose, which makes the framework's share as large as it can be.
  Real handlers that read a database make every framework's share smaller and the gaps narrower.
- One machine, one OS. Numbers on other hardware will differ; the ratios are the part to compare.
