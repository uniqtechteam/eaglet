# eaglet for coding agents

A reference for coding agents (Claude Code, Codex, Cursor and the like) and for people who want the
short, exact version. It covers eaglet 0.1 on Rust 1.88 or newer. The [README](../README.md) and the
[guide](guide.md) explain the same things at a slower pace.

Every Rust block on this page is copied from code that compiles and passes its tests in this
repository: [`crates/eaglet/tests/guide.rs`](../crates/eaglet/tests/guide.rs),
[`crates/eaglet/tests/mistakes.rs`](../crates/eaglet/tests/mistakes.rs),
[`examples/shop`](../examples/shop) and the `eaglet new` templates.
[`crates/eaglet/tests/docs.rs`](../crates/eaglet/tests/docs.rs) fails when one of them drifts, so
the syntax here is the syntax that builds.

## Rules

1. Start with `use eaglet::prelude::*;`. It brings the macros, `App`, `Guard`, `HttpError`,
   `ProviderDef`, `Valid`, `Arc`, `Json`, `Path`, `Query`, `Extension`, `StatusCode` and `Parts`.
2. Use Axum through `eaglet::axum` (for example `eaglet::axum::http::header`) instead of adding
   `axum` to `Cargo.toml`, so there is only one Axum version in the build.
3. Routes live in an `impl` block marked `#[controller("/prefix")]`, on an `#[injectable]` struct
   that is listed in a module's `controllers`. Do not build an `axum::Router` by hand for a module's
   routes, and do not call `Router::route` for them. For layers around the whole app, take the
   finished router with `app.into_router()`.
4. Every provider is declared in exactly one module's `providers`. Other modules `import` that module,
   and it lists the provider in `exports`.
5. A provider's dependencies are its `Arc<T>` fields. State the provider owns (a counter, a cache, a
   lock) is a field marked `#[inject(default)]`. Never build providers by hand with `new()` and never
   pass them around yourself; the app builds each one once.
6. Put storage and outside services behind a trait, bind it with `provide!(dyn Trait => Impl)`, and
   depend on `Arc<dyn Trait>`. Tests replace it with `override_provider::<dyn Trait>(..)`.
7. Handlers, services, repositories and guards return `Result<T, HttpError>`. Turn unexpected errors
   into `HttpError::internal(e)`. Do not `unwrap()` in request code.
8. Take request bodies as `Valid<Json<T>>` and query strings as `Valid<Query<T>>`, with `T` deriving
   `Deserialize`, `Validate` and `ToSchema`. The body extractor comes last in the handler's arguments.
9. Write route paths with `/:id` and `/*rest`. The macro turns them into Axum's `{id}` and `{*rest}`.
10. Give every handler a doc comment: its first line becomes the summary in the API docs.
11. After a change run `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test`.

## The project `eaglet new` makes

```console
$ eaglet new shop --db sqlite
```

```text
shop/
  AGENTS.md, CLAUDE.md      notes for coding agents, pointing here
  Cargo.toml                eaglet, serde, serde_json, tokio, tracing, utoipa, validator (+ sqlx)
  .env.example              PORT and DATABASE_URL
  Dockerfile, .dockerignore, .github/workflows/ci.yml
  src/main.rs               reads Config, App::new::<AppModule>(), Swagger UI at /docs, listen
  src/lib.rs                pub mod for every module folder; `eaglet g` adds to it
  src/app_module.rs         AppModule: the root, imports every feature module
  src/config.rs             Config (PORT) and ConfigModule, which exports it
  src/db.rs                 DbModule, exports the pool (only with --db)
  src/health.rs             HealthController at GET /health and HealthModule
  tests/health.rs           TestApp tests for /health and the API docs
```

`--db` takes `none` (the default), `postgres`, `sqlite`, `mongo`, `lumen-postgres` or
`lumen-mongo`. Database modules connect on first use, so the app and its tests start without a
database running. `[dev-dependencies]` already has `eaglet` with the `testing` feature.

The root module as generated. `eaglet g` adds every new feature module to `imports`:

```rust
use eaglet::prelude::*;

use crate::config::ConfigModule;
use crate::health::HealthModule;

/// The root of the module tree.
#[module(
    imports = [
        ConfigModule,
        HealthModule,
    ]
)]
pub struct AppModule;
```

Starting the app, in `src/main.rs`:

```rust
let config = Config::from_env();
let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
let app = App::new::<AppModule>()
    .await?
    .with_info("Shop API", env!("CARGO_PKG_VERSION"))
    .swagger("/docs");
tracing::info!("listening on http://{addr}, API docs at /docs");
app.listen(addr).await?;
```

## Adding a feature with `eaglet g`

Run these inside the project. They never overwrite a file, and `--dry-run` prints the plan only.

| Command | Short | Adds |
| --- | --- | --- |
| `eaglet g resource products` | `eaglet g res products` | `src/products/{mod,controller,service,repository,dto}.rs` and `tests/products.rs`, imported by `AppModule` |
| `eaglet g module billing` | `eaglet g mo billing` | `src/billing/mod.rs` with an empty module, imported by `AppModule` |
| `eaglet g controller billing` | `eaglet g co billing` | `src/billing/controller.rs`, listed in the module's `controllers` |
| `eaglet g service billing` | `eaglet g s billing` | `src/billing/service.rs`, listed in the module's `providers` |

A resource is full CRUD at `/products`: `GET /`, `GET /:id`, `POST /`, `PATCH /:id` and
`DELETE /:id`, with validated DTOs, a `ProductsRepo` trait, a `MemoryProductsRepo` that keeps rows
in memory, and tests. It builds and passes `cargo clippy -- -D warnings` as generated. To store the
rows in a database:

1. Write `SqlProductsRepo` in `repository.rs` with the pool as an `Arc` field, and
   `impl ProductsRepo for SqlProductsRepo`.
2. In `mod.rs`, change `provide!(dyn ProductsRepo => MemoryProductsRepo)` to point at it, and add
   `DbModule` to the module's `imports`.
3. Change the fields in `dto.rs` and the rules in `service.rs`; keep HTTP concerns in
   `controller.rs` and storage in `repository.rs`.

The finished module looks like the shop's:

```rust
#[module(
    imports = [AuthModule, DbModule],
    controllers = [ProductsController],
    providers = [ProductsService, provide!(dyn ProductsRepo => SqlProductsRepo)]
)]
pub struct ProductsModule;
```

Names can be given as `order-items`, `order_items` or `OrderItems`; the folder is `order_items`, the
path `/order-items` and the types `OrderItemsModule`, `OrderItem`.

## Modules

```text
#[module(imports = [..], controllers = [..], providers = [..], exports = [..])]
pub struct NameModule;
```

- Goes on a unit struct. Every key is optional; any other key is a compile error.
- `imports`: other module types. A module sees its own providers and what its imports export.
- `controllers`: `#[injectable]` structs with a `#[controller]` impl block.
- `providers`: entries of four kinds, shown below.
- `exports`: provider types, `dyn Trait` included, that importing modules may use. A module can only
  export what it provides or imports.

```rust
#[module(
    providers = [
        ProviderDef::value(MailSettings { from: "shop@example.com".into() }),
        provide!(dyn Mailer => SmtpMailer),
    ],
    exports = [dyn Mailer]
)]
struct MailModule;
```

| Entry | Builds | Inject it as |
| --- | --- | --- |
| `Greeter` | the `#[injectable]` struct, from its fields | `Arc<Greeter>` |
| `provide!(dyn Mailer => SmtpMailer)` | `SmtpMailer`, which must `impl Mailer` | `Arc<dyn Mailer>` |
| `ProviderDef::value(v)` | nothing, `v` as it is | `Arc<TypeOfV>` |
| `ProviderDef::factory(\|\| async { .. })` | the `Ok` value of the async block, at startup | `Arc<TheOkType>` |

A factory returns a `Result`; an `Err` stops the app. This is how a pool is made:

```rust
#[module(
    providers = [ProviderDef::factory(|| async { connect(&database_url()).await })],
    exports = [SqlitePool]
)]
pub struct DbModule;
```

Settings are a plain struct provided as a value, as `eaglet new` does:

```rust
#[module(providers = [ProviderDef::value(Config::from_env())], exports = [Config])]
pub struct ConfigModule;
```

## Providers

```text
#[injectable]
pub struct Name { dep: Arc<OtherProvider>, repo: Arc<dyn SomeTrait>, #[inject(default)] state: T }
```

- `#[injectable]` takes no arguments and goes on a struct with named fields or a unit struct.
  Generic structs are not supported.
- Each field is `Arc<T>`, where `T` is a provider the module can see, or is marked
  `#[inject(default)]` and starts as `Default::default()`.
- Every provider is a singleton built once at startup, dependencies first. There are no request
  scoped providers and no lifecycle hooks.

```rust
#[injectable]
struct Greeter {
    mailer: Arc<dyn Mailer>,
    #[inject(default)]
    sent: AtomicU64,
}
```

A service is an `#[injectable]` struct with ordinary methods:

```rust
#[injectable]
pub struct ProductsService {
    repo: Arc<dyn ProductsRepo>,
}

impl ProductsService {
    // ...
    pub async fn find(&self, id: i64) -> Result<Product, HttpError> {
        self.repo
            .get(id)
            .await?
            .ok_or_else(|| HttpError::not_found(format!("no product with id {id}")))
    }
    // ...
}
```

A trait used as `Arc<dyn Trait>` must be `Send + Sync`. Async methods in such a trait need
`#[async_trait]` (from the `async-trait` crate), because a plain `async fn` in a trait cannot be
used as `dyn`:

```rust
#[async_trait]
pub trait ProductsRepo: Send + Sync {
    // ...
    async fn get(&self, id: i64) -> Result<Option<Product>, HttpError>;
    // ...
}
```

`app.provider::<T>()` returns a built provider, in `main` or in tests.

## Controllers

```text
#[injectable]
pub struct NameController { service: Arc<NameService> }

#[controller("/prefix")]
#[guard(SomeGuard)]          // optional, every route; after #[controller]
#[layer(some_tower_layer)]   // optional, every route; after #[controller]
impl NameController {
    /// Summary for the API docs.
    #[get("/:id")]           // also #[post], #[put], #[patch], #[delete]
    #[guard(OtherGuard)]     // optional, this route only
    async fn find(&self, Path(id): Path<i64>) -> Result<Json<T>, HttpError> { .. }
}
```

- `#[controller("/prefix")]` goes on an inherent `impl` block, not a trait impl.
- A route attribute takes one string path. One route per method; write two methods for two routes.
- Handlers take `&self` first, then any Axum extractors, and return anything that implements
  `IntoResponse`. They may be `async` or not.
- Methods without a route attribute stay ordinary methods.
- Guards and layers on the `impl` block apply to every route; on a method, to that route only.
  Several guards run in the order written, controller guards first.

```rust
#[injectable]
struct ReportsController {
    greeter: Arc<Greeter>,
}

#[controller("/reports")]
#[guard(ApiKeyGuard)]
#[layer(eaglet::axum::middleware::map_response(no_store))]
impl ReportsController {
    /// Search the reports.
    #[get("/search")]
    fn search(
        &self,
        Extension(partner): Extension<Partner>,
        Valid(Query(search)): Valid<Query<Search>>,
    ) -> String {
        format!("{} searched for {}", partner.name, search.q)
    }
```

A route with a body, a guard on that route only and a `201` answer:

```rust
/// Put a product on sale. Needs a signed-in user, who becomes its seller.
#[post("/")]
#[guard(AuthGuard)]
async fn create(
    &self,
    Extension(user): Extension<CurrentUser>,
    Valid(Json(input)): Valid<Json<NewProduct>>,
) -> Result<(StatusCode, Json<Product>), HttpError> {
    let product = self.products.create(&user, input).await?;
    Ok((StatusCode::CREATED, Json(product)))
}
```

## Guards

A guard is an `#[injectable]` provider that implements `Guard`. It sees the request head before the
body is read. `Ok(())` lets the request through; an `HttpError` goes back to the client as it is.
Values it inserts into `parts.extensions` reach the handler as `Extension<T>`.

```rust
#[injectable]
struct ApiKeyGuard {
    keys: Arc<ApiKeys>,
}

impl Guard for ApiKeyGuard {
    async fn can_activate(&self, parts: &mut Parts) -> Result<(), HttpError> {
        let key = parts
            .headers
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| HttpError::unauthorized("send an x-api-key header"))?;
        let partner = self
            .keys
            .find(key)
            .await
            .ok_or_else(|| HttpError::unauthorized("unknown API key"))?;
        parts.extensions.insert(partner);
        Ok(())
    }
}
```

The guard must be visible in the controller's module: provided there, or exported by a module it
imports. The shop's `AuthModule` provides `AuthGuard` and exports it, and `ProductsModule` imports
`AuthModule`. The type put into the extensions must be `Clone`, as Axum's `Extension` requires.

## Validation and DTOs

Rules come from the [`validator`](https://docs.rs/validator) crate. `ToSchema` (from `utoipa`) puts
the type in the API docs; without it the docs show "any value".

```rust
/// The body of `POST /products`.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct NewProduct {
    /// 1 to 200 characters.
    #[validate(length(min = 1, max = 200))]
    pub name: String,
    /// At least 1 cent.
    #[validate(range(min = 1, max = 100_000_000))]
    pub price_cents: i64,
}
```

The imports this needs:

```rust
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;
```

A body that is not JSON or does not fit the type is a 400. A broken rule is a 422 with the code
`VALIDATION_FAILED` and every broken field under `details.fields`, using the rule's `message` or
else its name:

```rust
assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
let body = res.json::<Value>();
assert_eq!(body["code"], "VALIDATION_FAILED");
assert_eq!(
    body["details"]["fields"]["q"],
    json!(["type at least 2 characters"])
);
```

## Errors

| Constructor | Status | `code` |
| --- | --- | --- |
| `HttpError::bad_request(msg)` | 400 | `BAD_REQUEST` |
| `HttpError::unauthorized(msg)` | 401 | `UNAUTHORIZED` |
| `HttpError::forbidden(msg)` | 403 | `FORBIDDEN` |
| `HttpError::not_found(msg)` | 404 | `NOT_FOUND` |
| `HttpError::conflict(msg)` | 409 | `CONFLICT` |
| `HttpError::validation(msg)` | 422 | `VALIDATION_FAILED` |
| `HttpError::internal(cause)` | 500 | `INTERNAL_ERROR` |
| `HttpError::new(status, code, msg)` | any | any |

`.with_details(json!(..))` adds `details`. Every error, Axum's own 404, 405 and extractor
rejections included, leaves as `{"code", "message", "details", "traceId"}`. `internal` logs the
cause with the trace id and sends only a generic message, so it is the right place for database
errors:

```rust
async fn delete(&self, id: i64) -> Result<(), HttpError> {
    sqlx::query("delete from products where id = ?")
        .bind(id)
        .execute(&*self.pool)
        .await
        .map_err(HttpError::internal)?;
    Ok(())
}
```

A code of your own:

```rust
let error = HttpError::new(
    StatusCode::PAYMENT_REQUIRED,
    "PLAN_LIMIT",
    "needs the team plan",
);
Err(error.with_details(json!({ "plan": "free", "needs": "team" })))
```

`eaglet::trace_id()` returns the current request's trace id, for your own logs.

## Testing

`eaglet::testing::TestApp` needs the `testing` feature, which `eaglet new` turns on under
`[dev-dependencies]`:

```toml
[dev-dependencies]
eaglet = { version = "0.1", features = ["testing"] }
```

`TestApp::new::<AppModule>()` builds the real module tree with the same startup checks and sends
requests in memory, with no port:

```rust
#[tokio::test]
async fn health_is_ok() {
    let app = TestApp::new::<AppModule>().build().await.unwrap();
    let res = app.get("/health").send().await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.json::<serde_json::Value>()["status"], "ok");
}
```

To test with a fake, write a struct that implements the trait, keep an `Arc` to it, and override the
provider. The test can then look at what the app did with it:

```rust
#[derive(Default)]
struct FakeMailer {
    sent: Mutex<Vec<String>>,
}

impl Mailer for FakeMailer {
    fn send(&self, to: &str, _text: &str) -> Result<(), HttpError> {
        self.sent.lock().unwrap().push(to.to_owned());
        Ok(())
    }
}
```

```rust
#[tokio::test]
async fn a_fake_mailer_replaces_the_real_one() {
    let mailer = Arc::new(FakeMailer::default());
    let app = TestApp::new::<AppModule>()
        .override_provider::<dyn Mailer>(mailer.clone())
        .build()
        .await
        .unwrap();

    let res = app
        .post("/reports/welcome")
        .header("x-api-key", "k1")
        .send()
        .await;

    assert_eq!(res.text(), "mail number 1");
    assert_eq!(*mailer.sent.lock().unwrap(), ["acme"]);
}
```

- `.override_provider::<dyn Trait>(Arc::new(fake))` replaces a provider everywhere. A fake that
  does not implement the trait is a compile error.
- `.override_value(value)` is the short form for a concrete type, such as a test `Config` or a pool.
- The overridden provider's own dependencies are still checked at startup. When a repository's
  real version needs a pool, give the test a pool that never connects, as the shop's fake test does:

```rust
async fn app() -> TestApp {
    TestApp::new::<AppModule>()
        .override_provider::<dyn ProductsRepo>(Arc::new(FakeProducts::with_a_lamp()))
        // never connected to: the products repo is fake and these tests sign no one up
        .override_value(sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap())
        .override_value(Config {
            port: 0,
            database_url: String::new(),
            jwt_secret: "test-secret".into(),
        })
        .build()
        .await
        .unwrap()
}
```

- Requests: `get`, `post`, `put`, `patch`, `delete` or `request(method, path)`, then `.header(k, v)`,
  `.bearer(token)`, `.json(&value)` or `.body(..)`, then `.send().await`.
- Responses: `status()`, `header(name)`, `headers()`, `text()`, `bytes()`, `json::<T>()`.
- `app.provider::<T>()` reaches a built provider, for example to sign a test token.

The API docs can be tested the same way:

```rust
#[tokio::test]
async fn the_api_docs_list_the_health_route() {
    let app = App::new::<AppModule>().await.unwrap();
    let doc = serde_json::to_value(app.openapi()).unwrap();
    assert!(doc["paths"]["/health"]["get"].is_object());
}
```

## lumen

[lumen](https://github.com/uniqtechteam/lumen) is a Prisma-style data layer. With
`eaglet new shop --db lumen-postgres` (or `lumen-mongo`) the project gets:

- `prisma/schema.prisma`, the data model;
- `src/db/mod.rs` with `LumenModule`, which provides and exports the generated `Client`, so
  providers take `client: Arc<Client>`;
- `src/db/client.rs`, written by `lumen generate`. Run it before the first build and after every
  schema change (`lumen migrate dev` runs it too). Do not edit `client.rs` by hand;
- `db::http`, which turns a lumen error into the matching `HttpError`. Use it as
  `.map_err(db::http)?`:

```rust
#[must_use]
pub fn http(e: lumen_orm::Error) -> HttpError {
    match StatusCode::from_u16(e.http_status()) {
        Ok(status) if status.is_client_error() => HttpError::new(status, e.code, e.message.clone()),
        _ => HttpError::internal(e),
    }
}
```

A missing record is a 404, a duplicate a 409, a refused value a 400, and anything else a 500 that
hides the details. lumen's query API is described in its own README.

## Common mistakes

### At compile time

The macros explain wrong usage themselves. These are the exact messages
([`crates/eaglet/tests/ui`](../crates/eaglet/tests/ui) checks the first five rows):

| Message | Cause and fix |
| --- | --- |
| ``a provider's fields must be `Arc<SomeProvider>` (or mark the field #[inject(default)])`` | A field like `count: u32` on an `#[injectable]`. Wrap dependencies in `Arc`, or add `#[inject(default)]` for owned state. |
| `` #[injectable] needs named fields, for example `repo: Arc<UsersRepo>` `` | A tuple struct. Use named fields. |
| ``unknown key `services`; expected imports, controllers, providers or exports`` | A misspelled `#[module]` key. |
| ``#[get] only works inside an `impl` block marked #[controller("/prefix")]; put it below #[controller]`` | A route attribute on a free function. Move the handler into a controller's `impl` block. The same message comes for `#[guard]` outside a controller. |
| ``the trait bound `u32: UsersRepo` is not satisfied`` (on `override_provider`) | The test fake does not implement the trait it replaces. |
| ``route handlers take `&self` first, then Axum extractors`` | A handler without `&self`, or with `&mut self`. Providers are shared, so use `&self` and a `Mutex` or atomic field for state. |
| ``a method serves one route; write a second method for the other one`` | Two route attributes on one method. |
| ``write the path as a string, for example #[get("/:id")]`` | A path that is not a string literal. |
| ``#[guard] and #[layer] on a method need a route attribute too, for example #[get("/")]`` | A guard or layer on a method that is not a route. |
| ``#[controller] goes on a plain `impl` block, not a trait impl`` | `#[controller]` on `impl SomeTrait for X`. |
| `` a module is a unit struct, for example `struct UsersModule;` `` | `#[module]` on a struct with fields. |
| `generic providers are not supported yet` | `#[injectable]` on a generic struct. |
| ``the trait bound `{closure@…}: Handler<_, _>` is not satisfied``, pointing at `#[controller]` | One handler in the block is not a valid Axum handler. Check that the body extractor (`Json`, `Valid<Json<T>>`) is the last argument, that a `Valid<..>` type derives `Validate`, that every argument is an extractor, and that the return type implements `IntoResponse`. |
| `` unresolved import `eaglet::testing` `` | The `testing` feature is off. Add it under `[dev-dependencies]`. |
| ``the trait `ProductsRepo` is not dyn compatible`` | A trait with a plain `async fn` used as `dyn`. Add `#[async_trait]` to the trait and to every `impl`. |

### When the app starts

`App::new` and `TestApp::build` check the whole module tree before taking a request and return a
`StartupError`. These come from
[`crates/eaglet/tests/mistakes.rs`](../crates/eaglet/tests/mistakes.rs).

A provider, controller or guard that needs something its module cannot see. Here a controller uses
`AuthGuard` without importing `AuthModule`; the fix is `imports = [AuthModule]`:

```rust
#[module(controllers = [OrdersController])]
struct OrdersModule;

#[tokio::test]
async fn a_guard_from_another_module_needs_its_import() {
    let err = App::new::<OrdersModule>().await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "OrdersController needs AuthGuard, which OrdersModule does not provide or import"
    );
}
```

The same message comes for a trait with no `provide!(dyn Trait => Impl)` line, for example
`ProductsService needs dyn ProductsRepo, which ProductsModule does not provide or import`, and for a
field such as `Arc<Mutex<Vec<String>>>` that was meant to be owned state: mark it
`#[inject(default)]`.

The same provider declared in two modules. Declare it once, export it, and import that module:

```rust
assert_eq!(
    err.to_string(),
    "AuthGuard is provided by both AuthModule and AdminModule; provide it in one module and export it"
);
```

Providers that need each other. Move the shared part into a third provider both can depend on:

```rust
assert_eq!(
    err.to_string(),
    "providers depend on each other in a circle: Invoices → Payments → Invoices"
);
```

An export the module does not have. Provide it there, or import the module that does:

```rust
assert_eq!(
    err.to_string(),
    "LeakyModule exports AuthGuard, which it does not provide or import"
);
```

A factory that returned an error, such as a database that refused the connection. Factories made
by `eaglet new` connect lazily, so this one is usually a hand-written factory:

```rust
assert_eq!(err.to_string(), "could not create Pool: connection refused");
```

### At run time

A controller that is written but not listed in any module's `controllers` compiles and starts
without complaint, and its routes answer 404. Add it to `controllers`:

```rust
#[tokio::test]
async fn a_controller_left_out_of_its_module_has_no_routes() {
    let app = TestApp::new::<ForgotTheController>().build().await.unwrap();
    let res = app.get("/orders").send().await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
```

A new module that `AppModule` does not import (directly or through another module) is not part of
the app at all. `eaglet g` adds the import for you; when writing a module by hand, add it to
`AppModule`'s `imports` and its folder to `src/lib.rs`.

## More

- [README](../README.md): a walkthrough that builds the products feature of the shop example.
- [Guide](guide.md): each part in more detail, including layers and the API docs.
- [`examples/shop`](../examples/shop): a complete app with sign-up, JWT sign-in, a guard, SQLite
  and both kinds of tests.
- [API reference on docs.rs](https://docs.rs/eaglet).
