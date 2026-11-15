//! SPEC-CLI-001: the eaglet command, run as a user would.

#![allow(clippy::unwrap_used, reason = "tests: a failure should stop the test")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A fresh, empty folder for one test.
fn scratch(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eaglet-cli-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn eaglet(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_eaglet"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn ac_fr_004_a_dry_run_lists_the_files_and_writes_nothing() {
    let dir = scratch("dry");
    let out = eaglet(&dir, &["new", "shop", "--db", "postgres", "--dry-run"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    for file in [
        "create Cargo.toml",
        "create src/app_module.rs",
        "create src/db.rs",
        "create Dockerfile",
        "create .github/workflows/ci.yml",
    ] {
        assert!(text.contains(file), "missing `{file}` in:\n{text}");
    }
    assert!(text.contains("dry run"));
    assert!(!dir.join("shop").exists());
}

#[test]
fn generate_registers_modules_and_parts() {
    let dir = scratch("gen");
    assert!(eaglet(&dir, &["new", "shop"]).status.success());
    let shop = dir.join("shop");
    let out = eaglet(&shop, &["g", "resource", "order-items"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("create src/order_items/controller.rs"));
    let out = eaglet(&shop, &["g", "co", "billing"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let out = eaglet(&shop, &["g", "s", "billing"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let app = std::fs::read_to_string(shop.join("src/app_module.rs")).unwrap();
    assert!(app.contains("use crate::order_items::OrderItemsModule;"));
    assert!(app.contains("OrderItemsModule,"));
    assert!(app.contains("BillingModule,"));
    let lib = std::fs::read_to_string(shop.join("src/lib.rs")).unwrap();
    assert!(lib.contains("pub mod billing;") && lib.contains("pub mod order_items;"));
    let billing = std::fs::read_to_string(shop.join("src/billing/mod.rs")).unwrap();
    assert!(billing.contains("BillingController") && billing.contains("BillingService"));
    let controller = std::fs::read_to_string(shop.join("src/order_items/controller.rs")).unwrap();
    assert!(controller.contains("#[controller(\"/order-items\")]"));
    assert!(shop.join("tests/order_items.rs").exists());
}

#[test]
fn existing_files_and_folders_are_never_overwritten() {
    let dir = scratch("exists");
    assert!(eaglet(&dir, &["new", "shop"]).status.success());
    let again = eaglet(&dir, &["new", "shop"]);
    assert!(!again.status.success());
    assert!(stderr(&again).contains("already exists"));

    let shop = dir.join("shop");
    assert!(
        eaglet(&shop, &["g", "resource", "products"])
            .status
            .success()
    );
    let twice = eaglet(&shop, &["g", "resource", "products"]);
    assert!(!twice.status.success());
    assert!(stderr(&twice).contains("src/products already exists"));
    let controller = eaglet(&shop, &["g", "controller", "products"]);
    assert!(stderr(&controller).contains("src/products/controller.rs already exists"));
}

#[test]
fn generate_outside_a_project_explains_what_to_do() {
    let dir = scratch("outside");
    let out = eaglet(&dir, &["g", "module", "billing"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("made by `eaglet new`"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn bad_names_are_refused() {
    let dir = scratch("names");
    let out = eaglet(&dir, &["new", "type"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("keyword"));
    assert!(!dir.join("type").exists());
}
