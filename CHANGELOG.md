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

