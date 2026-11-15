//! SPEC-CORE-001: modules, providers and the startup checks.

#![allow(clippy::unwrap_used, reason = "tests: a failure should stop the test")]
#![allow(
    dead_code,
    reason = "test providers exist to be wired together, not read"
)]

use eaglet::prelude::*;

// ------------------------------------------------------------------ a small, working app

#[injectable]
struct Db;

#[injectable]
struct UsersRepo {
    db: Arc<Db>,
}

#[injectable]
struct UsersService {
    repo: Arc<UsersRepo>,
}

#[injectable]
struct OrdersService {
    users: Arc<UsersService>,
    db: Arc<Db>,
}

#[module(providers = [Db], exports = [Db])]
struct DbModule;

#[module(imports = [DbModule], providers = [UsersRepo, UsersService], exports = [UsersService])]
struct UsersModule;

#[module(imports = [DbModule, UsersModule], providers = [OrdersService])]
struct AppModule;

#[tokio::test]
async fn builds_providers_in_dependency_order() {
    let app = App::new::<AppModule>().await.unwrap();
    let orders = app.provider::<OrdersService>().unwrap();
    let repo = app.provider::<UsersRepo>().unwrap();
    assert!(
        Arc::ptr_eq(&orders.users.repo, &repo),
        "nested dependencies are the shared instances"
    );
}

#[tokio::test]
async fn ac_003_an_exported_provider_is_one_shared_instance() {
    let app = App::new::<AppModule>().await.unwrap();
    let orders = app.provider::<OrdersService>().unwrap();
    let repo = app.provider::<UsersRepo>().unwrap();
    assert!(
        Arc::ptr_eq(&orders.db, &repo.db),
        "Db is built once and shared"
    );
    assert!(Arc::ptr_eq(
        &orders.users,
        &app.provider::<UsersService>().unwrap()
    ));
}

// ------------------------------------------------------------------ AC-001: missing provider

#[injectable]
struct DbPool;

#[injectable]
struct AccountsService {
    pool: Arc<DbPool>,
}

#[module(providers = [AccountsService])]
struct AccountsModule;

#[tokio::test]
async fn ac_001_a_missing_provider_names_the_provider_and_the_module() {
    let err = App::new::<AccountsModule>().await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "AccountsService needs DbPool, which AccountsModule does not provide or import"
    );
}

// a provider declared in another module but not exported is not visible

#[module(providers = [DbPool])]
struct PoolModule;

#[module(imports = [PoolModule], providers = [AccountsService])]
struct AccountsWithoutExportModule;

#[tokio::test]
async fn a_provider_that_is_not_exported_is_not_visible() {
    let err = App::new::<AccountsWithoutExportModule>().await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "AccountsService needs DbPool, which AccountsWithoutExportModule does not provide or import"
    );
}

// ------------------------------------------------------------------ AC-002: cycles

#[injectable]
struct A {
    b: Arc<B>,
}

#[injectable]
struct B {
    a: Arc<A>,
}

#[module(providers = [A, B])]
struct CycleModule;

#[tokio::test]
async fn ac_002_a_cycle_is_reported_with_the_whole_circle() {
    let err = App::new::<CycleModule>().await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "providers depend on each other in a circle: A → B → A"
    );
}

// ------------------------------------------------------------------ duplicates and exports

#[module(providers = [Db])]
struct OtherDbModule;

#[module(imports = [DbModule, OtherDbModule])]
struct TwoDbsModule;

#[tokio::test]
async fn the_same_provider_in_two_modules_is_an_error() {
    let err = App::new::<TwoDbsModule>().await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "Db is provided by both DbModule and OtherDbModule; provide it in one module and export it"
    );
}

#[module(exports = [Db])]
struct BadExportModule;

#[module(imports = [BadExportModule])]
struct ImportsBadExportModule;

#[tokio::test]
async fn exporting_something_not_provided_is_an_error() {
    let err = App::new::<ImportsBadExportModule>().await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "BadExportModule exports Db, which it does not provide or import"
    );
}

// ------------------------------------------------------------------ traits, values, factories, overrides

trait Mailer: Send + Sync {
    fn send(&self, to: &str) -> String;
}

#[injectable]
struct SmtpMailer;

impl Mailer for SmtpMailer {
    fn send(&self, to: &str) -> String {
        format!("smtp:{to}")
    }
}

struct FakeMailer;

impl Mailer for FakeMailer {
    fn send(&self, to: &str) -> String {
        format!("fake:{to}")
    }
}

#[derive(Debug)]
struct Config {
    from: String,
}

#[injectable]
struct Notifier {
    mailer: Arc<dyn Mailer>,
    config: Arc<Config>,
    #[inject(default)]
    sent: std::sync::atomic::AtomicUsize,
}

#[module(providers = [
    provide!(dyn Mailer => SmtpMailer),
    ProviderDef::value(Config { from: "hi@example.com".into() }),
    Notifier,
])]
struct NotifyModule;

#[tokio::test]
async fn traits_values_and_default_fields() {
    let app = App::new::<NotifyModule>().await.unwrap();
    let n = app.provider::<Notifier>().unwrap();
    assert_eq!(n.mailer.send("a@b.c"), "smtp:a@b.c");
    assert_eq!(n.config.from, "hi@example.com");
    assert_eq!(n.sent.load(std::sync::atomic::Ordering::Relaxed), 0);
}

#[tokio::test]
async fn a_provider_can_be_overridden() {
    let app = App::builder::<NotifyModule>()
        .override_provider::<dyn Mailer>(Arc::new(FakeMailer))
        .build()
        .await
        .unwrap();
    assert_eq!(
        app.provider::<Notifier>().unwrap().mailer.send("x"),
        "fake:x"
    );
}

#[derive(Debug)]
struct Pool {
    size: u32,
}

#[module(providers = [ProviderDef::factory(|| async { Ok::<_, std::io::Error>(Pool { size: 5 }) })])]
struct PoolFactoryModule;

#[module(providers = [ProviderDef::factory(|| async { Err::<Pool, _>("connection refused") })])]
struct FailingFactoryModule;

#[tokio::test]
async fn async_factories_build_values_and_report_errors() {
    let app = App::new::<PoolFactoryModule>().await.unwrap();
    assert_eq!(app.provider::<Pool>().unwrap().size, 5);
    let err = App::new::<FailingFactoryModule>().await.unwrap_err();
    assert_eq!(err.to_string(), "could not create Pool: connection refused");
}
