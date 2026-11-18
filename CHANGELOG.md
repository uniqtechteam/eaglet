# Changelog

All notable changes to eaglet are written down here, for people who use it.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed

- `app.listen(addr)` stops on Ctrl-C or SIGTERM and lets the requests in flight finish (for up to
  30 seconds) instead of dropping them. Before, a deploy or `docker stop` cut open requests off.
- `app.listen(addr)` gives a client 30 seconds to send a request's headers and closes kept-alive
  connections idle for longer, so slow or stuck clients can no longer hold connections open forever.
- A handler or guard that panics answers 500 `INTERNAL_ERROR` in the envelope, with the trace id,
  instead of closing the connection with no response.
- `eaglet new`: the Dockerfile builds on the newest Rust 1.x. With `rust:1.88` the build of a
  `--db postgres` or `--db sqlite` project failed, because sqlx 0.9 needs Rust 1.94.
- `eaglet new --db postgres` turns on sqlx's rustls support, so `sslmode=require` URLs (what most
  hosted Postgres services hand out) connect.

### Changed

- `eaglet new`: Swagger UI at `/docs` is on in debug builds and off in release builds unless
  `API_DOCS=true`. The Dockerfile caches the registry and `target` between builds, and logs are
  plain text when they do not go to a terminal.

### Added

- `app.serve(listener, stop)`: the same server on a listener you bound, until a future of your
  choice finishes.
- Handlers served by `listen` or `serve` can read the peer address with `ConnectInfo<SocketAddr>`.
- `app.layer(..)` wraps every route, and the 404 for unknown paths, in a Tower layer, inside the
  error envelope. Use it for a request timeout, CORS or security headers across the whole app.

## [0.1.1] - 2026-11-17

### Changed

- Documentation: the README introduces eaglet on its own, with a walkthrough of the shop example,
  a comparison with plain Axum and the overhead measured against it. Comparisons with other
  frameworks moved to their own guides in `docs/`. The crate description, the API docs and the
  `AGENTS.md` that `eaglet new` writes were reworded to match. No changes to the API or behaviour.

## [0.1.0] - 2026-11-17

The first release. Needs Rust 1.88 or newer.

### Added

- Releases come from a `v*` tag: `eaglet` binaries for Linux (x86_64, aarch64), macOS (aarch64, x86_64)
  and Windows (x86_64) with SHA256SUMS on the GitHub release, and `eaglet-macros`, `eaglet` and
  `eaglet-cli` on crates.io.
- `bench/`: the same routes in plain Axum and in eaglet, measured in memory and over HTTP, with the
  raw numbers and the chart in the README.
- Project setup: the `eaglet` runtime, the `eaglet-macros` crate and the `eaglet` command line tool.
- Modules and providers: `#[module(imports, providers, exports)]`, `#[injectable]` structs whose `Arc<…>`
  fields are other providers, `provide!(dyn Trait => Type)`, ready values and async factories.
  Providers are built once at startup in dependency order and shared.
- Startup checks with plain messages: a missing provider names the provider and the module
  ("UsersService needs DbPool, which UsersModule does not provide or import"), circles show the whole
  path, duplicates and bad exports are reported.
- `App::new::<AppModule>()`, `App::builder().override_provider(...)`, `app.provider::<T>()`,
  `app.listen(addr)`.
- Controllers: `#[controller("/users")]` on an `impl` block, with `#[get]`, `#[post]`, `#[put]`, `#[patch]`
  and `#[delete]` methods that take `&self` and any Axum extractors. Paths use `/:id` and `/*rest`.
  A controller is an `#[injectable]` struct listed in `#[module(controllers = [...])]`, and its
  dependencies get the same startup checks as providers.
- Guards: `impl Guard for AuthGuard` and `#[guard(AuthGuard)]` on a controller or a route. Guards are
  providers, run before the body is read, and can hand values to the handler through
  `parts.extensions`.
- `Valid<Json<T>>` and `Valid<Query<T>>` check `#[validate(...)]` rules (the `validate` feature, on by
  default); a broken rule is a 422 `VALIDATION_FAILED` listing every field.
- `HttpError` (one pointer wide, so `Result<T, HttpError>` stays small) and one error shape for every failure, `{ code, message, details, traceId }`, including
  Axum's own 404, 405 and extractor rejections. Internal errors log the cause and send a generic
  message. Each request gets a trace id (or keeps a sane incoming `x-request-id`), returned in the
  `x-request-id` header and available from `eaglet::trace_id()`.
- `#[layer(...)]` adds Tower layers to a controller or a route.
- API docs, like `@nestjs/swagger`: `app.openapi()` builds an OpenAPI 3.1 document from the controllers
  (paths, path and query parameters, JSON bodies, responses, doc comments as summaries, one tag per
  controller, the error envelope as the default response), and `.swagger("/docs")` serves Swagger UI
  with the JSON at `/docs/openapi.json`. DTOs that derive `utoipa::ToSchema` get full schemas; others
  show as "any value", so nothing has to derive it. Features `openapi` and `swagger` (on by default).
- The `eaglet` command line, like the Nest CLI. `eaglet new shop --db none|postgres|sqlite|mongo`
  creates a project that builds, passes `cargo fmt`, `cargo clippy -D warnings` and its tests as
  generated: `AppModule`, a config module, `/health`, Swagger UI at `/docs`, tests, a Dockerfile that
  runs as a non-root user, and CI. Database pools connect on first use, so the app starts without a
  database. `eaglet g resource products` adds a CRUD module (controller, service, repository trait
  with an in-memory version, validated DTOs, tests) and imports it in `AppModule`;
  `eaglet g module|controller|service <name>` add single parts. `--dry-run` shows the plan. Existing
  files are never overwritten.
- `eaglet new shop --db lumen-postgres|lumen-mongo`: the data lives in `prisma/schema.prisma` and
  [lumen](https://github.com/uniqtechteam/lumen) generates the client. The project gets a
  `LumenModule` that provides `Arc<Client>` (connecting on first use), a `db::http` function that
  turns lumen errors into the right `HttpError` (404 for nothing found, 409 for a duplicate, 400 for a
  refused value, a hidden 500 otherwise) and a test for it. `--lumen-path` builds against a local
  lumen checkout.
- `examples/shop`: sign-up, JWT sign-in with an `AuthGuard`, products only their seller can change,
  SQLite through SQLx, validation, Swagger UI, and tests both against SQLite in memory and with fake
  repositories.
- Route methods no longer trip clippy's pedantic `unused_self`: eaglet always passes `&self`.
- `eaglet::testing` (the `testing` feature): `TestApp::new::<AppModule>()` builds the real module tree,
  `.override_provider::<dyn UsersRepo>(Arc::new(Fake))` or `.override_value(fake)` swaps providers (a
  wrong type does not compile), and `app.get("/users/1").send().await` runs requests in memory with
  `status()`, `header()`, `text()` and `json::<T>()` on the response.
- Documentation: the README walks a NestJS developer through the shop example step by step, with
  the NestJS code next to the eaglet code, a concept table and a diagram of the request pipeline.
  `docs/guide.md` covers modules and providers, controllers, guards, validation, errors, layers,
  testing, the API docs and the command line. Every Rust block in both is copied from code the
  workspace builds and tests, and `crates/eaglet/tests/docs.rs` fails when one drifts.
- `docs/llm-guide.md`, a reference for coding agents such as Claude Code, Codex and Cursor: the exact
  macro syntax, the project layout, code for each part, testing with fakes, lumen, and the compile and
  startup messages common mistakes cause, with their fixes. Its snippets are checked like the README's,
  and the startup messages are tested in `crates/eaglet/tests/mistakes.rs`. `llms.txt` at the root
  points tools to it.
- `eaglet new` writes an `AGENTS.md` with the project's commands, layout and rules, linking to the
  guide for coding agents, and a `CLAUDE.md` that imports it.

### Fixed

- `eaglet new --eaglet-path/--lumen-path` on Windows writes a plain `C:\...` path into Cargo.toml
  instead of `\\?\C:\...`.
- The `eaglet` crate no longer depends on `futures`.
- The request layer costs 150 to 190 ns over plain Axum instead of about 500 ns: it is a plain tower
  service now, makes the request id only when something reads it, and reads the body only on errors.

[Unreleased]: https://github.com/uniqtechteam/eaglet/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/uniqtechteam/eaglet/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/uniqtechteam/eaglet/releases/tag/v0.1.0
