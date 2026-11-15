# Changelog

All notable changes to eaglet are written down here, for people who use it.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

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
- `HttpError` and one error shape for every failure, `{ code, message, details, traceId }`, including
  Axum's own 404, 405 and extractor rejections. Internal errors log the cause and send a generic
  message. Each request gets a trace id (or keeps a sane incoming `x-request-id`), returned in the
  `x-request-id` header and available from `eaglet::trace_id()`.
- `#[layer(...)]` adds Tower layers to a controller or a route.

