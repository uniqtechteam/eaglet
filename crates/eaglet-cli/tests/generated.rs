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
    generated_with(db, None);
}

/// With `lumen`: the lumen checkout, whose `lumen generate` writes the client before the build.
fn generated_with(db: &str, lumen: Option<&Path>) {
    let dir = std::env::temp_dir().join(format!("eaglet-generated-{db}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let eaglet_crate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../eaglet");
    let eaglet_crate = eaglet_crate.canonicalize().unwrap();
    let bin = env!("CARGO_BIN_EXE_eaglet");
    let mut args = vec![
        "new".to_owned(),
        "demo".to_owned(),
        "--db".to_owned(),
        db.to_owned(),
        "--eaglet-path".to_owned(),
        eaglet_crate.display().to_string(),
    ];
    if let Some(lumen) = lumen {
        args.push("--lumen-path".to_owned());
        args.push(lumen.join("crates/lumen-orm").display().to_string());
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    run(&dir, bin, &args);
    let project = dir.join("demo");
    if let Some(lumen) = lumen {
        let manifest = lumen.join("Cargo.toml").display().to_string();
        run(
            &project,
            "cargo",
            &[
                "run",
                "-q",
                "--manifest-path",
                &manifest,
                "--bin",
                "lumen",
                "--",
                "generate",
            ],
        );
    }
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

/// The lumen checkout named by `EAGLET_TEST_LUMEN_DIR`. lumen is not on crates.io yet, so these
/// projects build against a local checkout and are skipped without one.
fn lumen_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("EAGLET_TEST_LUMEN_DIR").map(PathBuf::from);
    if dir.is_none() {
        eprintln!("skipped: EAGLET_TEST_LUMEN_DIR (a lumen checkout) is not set");
    }
    dir.map(|d| d.canonicalize().unwrap())
}

#[test]
#[ignore = "builds a whole generated project with lumen; run with --ignored"]
fn the_lumen_postgres_project_builds_and_passes() {
    if let Some(lumen) = lumen_dir() {
        generated_with("lumen-postgres", Some(&lumen));
    }
}

#[test]
#[ignore = "builds a whole generated project with lumen; run with --ignored"]
fn the_lumen_mongo_project_builds_and_passes() {
    if let Some(lumen) = lumen_dir() {
        generated_with("lumen-mongo", Some(&lumen));
    }
}
