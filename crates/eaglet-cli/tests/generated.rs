//! SPEC-CLI-001 AC-001 and AC-002, end to end: a generated project builds, passes `cargo fmt
//! --check`, `cargo clippy -D warnings` and its own tests, before and after `eaglet g resource`.
//! It compiles a whole project, so it is ignored by default; CI runs it with `--ignored`.

#![allow(clippy::unwrap_used, reason = "tests: a failure should stop the test")]

use std::path::{Path, PathBuf};
use std::process::Command;

fn run(dir: &Path, program: &str, args: &[&str]) {
    let target = std::env::var_os("EAGLET_GENERATED_TARGET").map_or_else(
        || std::env::temp_dir().join("eaglet-generated-target"),
        PathBuf::from,
    );
    let out = Command::new(program)
        .args(args)
        .current_dir(dir)
        .env("CARGO_TARGET_DIR", target)
        .env_remove("DATABASE_URL")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "`{program} {}` failed in {}:\n{}\n{}",
        args.join(" "),
        dir.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn check(project: &Path) {
    run(project, "cargo", &["fmt", "--check"]);
    run(
        project,
        "cargo",
        &["clippy", "--all-targets", "--", "-D", "warnings"],
    );
    run(project, "cargo", &["test"]);
}

fn generated(db: &str) {
    let dir = std::env::temp_dir().join(format!("eaglet-generated-{db}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let eaglet_crate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../eaglet");
    let eaglet_crate = eaglet_crate.canonicalize().unwrap();
    let bin = env!("CARGO_BIN_EXE_eaglet");
    run(
        &dir,
        bin,
        &[
            "new",
            "demo",
            "--db",
            db,
            "--eaglet-path",
            eaglet_crate.to_str().unwrap(),
        ],
    );
    let project = dir.join("demo");
    check(&project);
    run(&project, bin, &["g", "resource", "products"]);
    run(&project, bin, &["g", "controller", "billing"]);
    run(&project, bin, &["g", "service", "billing"]);
    check(&project);
}

#[test]
#[ignore = "builds a whole generated project; run with --ignored"]
fn ac_001_ac_002_a_new_project_and_a_resource_pass_fmt_clippy_and_tests() {
    generated("none");
}

#[test]
#[ignore = "builds a whole generated project with sqlx; run with --ignored"]
fn the_postgres_project_builds_and_passes() {
    generated("postgres");
}

#[test]
#[ignore = "builds a whole generated project with sqlx; run with --ignored"]
fn the_sqlite_project_builds_and_passes() {
    generated("sqlite");
}

#[test]
#[ignore = "builds a whole generated project with the mongodb driver; run with --ignored"]
fn the_mongo_project_builds_and_passes() {
    generated("mongo");
}
