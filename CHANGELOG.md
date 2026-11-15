# Changelog

All notable changes to eaglet are written down here, for people who use it.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

Needs Rust 1.88 or newer.

### Added

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
- `eaglet::testing` (the `testing` feature): `TestApp::new::<AppModule>()` builds the real module tree,
  `.override_provider::<dyn UsersRepo>(Arc::new(Fake))` or `.override_value(fake)` swaps providers (a
  wrong type does not compile), and `app.get("/users/1").send().await` runs requests in memory with
  `status()`, `header()`, `text()` and `json::<T>()` on the response.

