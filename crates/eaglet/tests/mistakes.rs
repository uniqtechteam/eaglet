//! The wiring mistakes listed in `docs/llm-guide.md`, each with the exact message it causes. The
//! compile-time ones are in `tests/ui`; these are found when the app starts.

#![allow(clippy::unwrap_used, reason = "tests: a failure should stop the test")]

use eaglet::prelude::*;
use eaglet::testing::TestApp;

#[injectable]
struct AuthGuard;

impl Guard for AuthGuard {
    fn can_activate(
        &self,
        _parts: &mut Parts,
    ) -> impl Future<Output = Result<(), HttpError>> + Send {
        std::future::ready(Ok(()))
    }
}

#[module(providers = [AuthGuard], exports = [AuthGuard])]
struct AuthModule;

#[injectable]
struct OrdersController;

#[controller("/orders")]
impl OrdersController {
    /// List the orders.
    #[get("/")]
    #[guard(AuthGuard)]
    fn list(&self) -> &'static str {
        "[]"
    }
}

/// Uses `AuthGuard` but does not import `AuthModule`.
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

/// The controller is written but not listed in `controllers`.
#[module(imports = [AuthModule])]
struct ForgotTheController;

#[tokio::test]
async fn a_controller_left_out_of_its_module_has_no_routes() {
    let app = TestApp::new::<ForgotTheController>().build().await.unwrap();
    let res = app.get("/orders").send().await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[allow(dead_code, reason = "never built: the circle stops the app first")]
#[injectable]
struct Invoices {
    payments: Arc<Payments>,
}

#[allow(dead_code, reason = "never built: the circle stops the app first")]
#[injectable]
struct Payments {
    invoices: Arc<Invoices>,
}

#[module(providers = [Invoices, Payments])]
struct BillingModule;

#[tokio::test]
async fn providers_that_need_each_other_are_a_circle() {
    let err = App::new::<BillingModule>().await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "providers depend on each other in a circle: Invoices → Payments → Invoices"
    );
}

/// Declares `AuthGuard` again instead of importing `AuthModule`.
#[module(providers = [AuthGuard])]
struct AdminModule;

#[module(imports = [AuthModule, AdminModule])]
struct TwiceModule;

#[tokio::test]
async fn a_provider_belongs_to_one_module() {
    let err = App::new::<TwiceModule>().await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "AuthGuard is provided by both AuthModule and AdminModule; provide it in one module and export it"
    );
}

/// Exports a guard it neither provides nor imports.
#[module(exports = [AuthGuard])]
struct LeakyModule;

#[module(imports = [LeakyModule])]
struct ImportsLeaky;

#[tokio::test]
async fn a_module_exports_only_what_it_has() {
    let err = App::new::<ImportsLeaky>().await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "LeakyModule exports AuthGuard, which it does not provide or import"
    );
}

struct Pool;

#[module(providers = [ProviderDef::factory(|| async {
    Err::<Pool, _>(std::io::Error::other("connection refused"))
})])]
struct DbModule;

#[tokio::test]
async fn a_failed_factory_stops_the_app() {
    let err = App::new::<DbModule>().await.unwrap_err();
    assert_eq!(err.to_string(), "could not create Pool: connection refused");
}
