//! The eaglet command line: `eaglet new` and `eaglet g`, like the Nest CLI.

pub mod edit;
pub mod names;
pub mod plan;

use std::path::Path;

use crate::edit::{add_mod, add_to_module_list, add_use};
use crate::names::Names;
use crate::plan::Plan;

/// Which database `eaglet new --db` sets up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Db {
    /// No database module.
    None,
    /// A lazy `sqlx::PgPool`.
    Postgres,
    /// A lazy `sqlx::SqlitePool`.
    Sqlite,
    /// A `mongodb::Database`.
    Mongo,
    /// A lumen client for Postgres, generated from `prisma/schema.prisma`.
    LumenPostgres,
    /// A lumen client for MongoDB, generated from `prisma/schema.prisma`.
    LumenMongo,
}

impl Db {
    fn lumen(self) -> bool {
        matches!(self, Self::LumenPostgres | Self::LumenMongo)
    }
}

/// What `eaglet g` makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An empty module, imported by `AppModule`.
    Module,
    /// A controller in a module (the module is created when missing).
    Controller,
    /// A service in a module (the module is created when missing).
    Service,
    /// A full CRUD feature: module, controller, service, repository, DTOs and tests.
    Resource,
}

/// Where the `eaglet` (or `lumen-orm`) dependency of a new project comes from.
#[derive(Debug, Clone)]
pub enum Source {
    /// From crates.io, this CLI's version.
    Registry,
    /// A local checkout of the crate (for working on eaglet or lumen itself).
    Path(String),
}

/// The version of `lumen-orm` that `--db lumen-*` projects use.
pub const LUMEN_VERSION: &str = "0.1";

fn render(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = template.to_owned();
    for (key, value) in vars {
        out = out.replace(&format!("{{{{{key}}}}}"), value);
    }
    out
}

fn part_vars(n: &Names, krate: &str) -> Vec<(&'static str, String)> {
    vec![
        ("Plural", n.pascal.clone()),
        ("Singular", n.singular_pascal.clone()),
        ("snake", n.snake.clone()),
        ("path", n.kebab.clone()),
        ("plural_words", n.words.clone()),
        ("singular_words", n.singular_words.clone()),
        ("crate", krate.to_owned()),
    ]
}

fn render_with(template: &str, vars: &[(&'static str, String)]) -> String {
    let borrowed: Vec<(&str, &str)> = vars.iter().map(|(k, v)| (*k, v.as_str())).collect();
    render(template, &borrowed)
}

/// The `eaglet` lines for `[dependencies]` and `[dev-dependencies]`.
fn eaglet_deps(source: &Source) -> (String, String) {
    let version = eaglet::VERSION;
    match source {
        Source::Registry => (
            format!("\"{version}\""),
            format!("{{ version = \"{version}\", features = [\"testing\"] }}"),
        ),
        Source::Path(p) => (
            format!("{{ path = {p:?} }}"),
            format!("{{ path = {p:?}, features = [\"testing\"] }}"),
        ),
    }
}

/// The `lumen-orm` line for `[dependencies]`.
fn lumen_dep(lumen: &Source, mongo: bool) -> String {
    let features = if mongo {
        ", features = [\"mongodb\"]"
    } else {
        ""
    };
    match lumen {
        Source::Registry => format!("lumen-orm = {{ version = \"{LUMEN_VERSION}\"{features} }}\n"),
        Source::Path(p) => format!("lumen-orm = {{ path = {p:?}{features} }}\n"),
    }
}

/// The dependency lines, the `.env.example` line and the `src/db.rs` template for `--db`.
fn db_setup(db: Db, krate: &str, lumen: &Source) -> (String, String, Option<&'static str>) {
    match db {
        Db::LumenPostgres => (
            lumen_dep(lumen, false),
            format!("DATABASE_URL=postgres://postgres:postgres@localhost/{krate}"),
            Some(include_str!("../templates/lumen/postgres.rs.tmpl")),
        ),
        Db::LumenMongo => (
            lumen_dep(lumen, true),
            format!("DATABASE_URL=mongodb://localhost:27017/{krate}"),
            Some(include_str!("../templates/lumen/mongo.rs.tmpl")),
        ),
        Db::None => (String::new(), String::new(), None),
        Db::Postgres => (
            "sqlx = { version = \"0.9\", default-features = false, features = [\"runtime-tokio\", \"postgres\"] }\n".to_owned(),
            format!("DATABASE_URL=postgres://postgres:postgres@localhost/{krate}"),
            Some(include_str!("../templates/db/postgres.rs.tmpl")),
        ),
        Db::Sqlite => (
            "sqlx = { version = \"0.9\", default-features = false, features = [\"runtime-tokio\", \"sqlite\"] }\n".to_owned(),
            format!("DATABASE_URL=sqlite://{krate}.db?mode=rwc"),
            Some(include_str!("../templates/db/sqlite.rs.tmpl")),
        ),
        Db::Mongo => (
            "mongodb = \"3\"\n".to_owned(),
            "DATABASE_URL=mongodb://localhost:27017".to_owned(),
            Some(include_str!("../templates/db/mongo.rs.tmpl")),
        ),
    }
}

/// Plan `eaglet new <name>` in `parent`.
///
/// # Errors
/// A bad name, or a folder that exists and is not empty.
pub fn plan_new(
    parent: &Path,
    name: &str,
    db: Db,
    (source, lumen): (&Source, &Source),
) -> Result<Plan, String> {
    let names = Names::parse(name)?;
    let root = parent.join(name);
    if root.read_dir().is_ok_and(|mut d| d.next().is_some()) {
        return Err(format!(
            "{} already exists and is not empty",
            root.display()
        ));
    }
    let krate = names.snake.clone();
    let title = names.title();
    let (dep, dev_dep) = eaglet_deps(source);
    let (db_deps, db_env, db_file) = db_setup(db, &krate, lumen);
    let vars = [
        ("crate", krate.as_str()),
        ("title", title.as_str()),
        ("eaglet_dep", dep.as_str()),
        ("eaglet_dev_dep", dev_dep.as_str()),
        ("db_deps", db_deps.as_str()),
        ("db_env", db_env.as_str()),
    ];
    let files: [(&str, &str); 13] = [
        (
            "Cargo.toml",
            include_str!("../templates/new/Cargo.toml.tmpl"),
        ),
        ("src/main.rs", include_str!("../templates/new/main.rs.tmpl")),
        ("src/lib.rs", include_str!("../templates/new/lib.rs.tmpl")),
        (
            "src/app_module.rs",
            include_str!("../templates/new/app_module.rs.tmpl"),
        ),
        (
            "src/config.rs",
            include_str!("../templates/new/config.rs.tmpl"),
        ),
        (
            "src/health.rs",
            include_str!("../templates/new/health.rs.tmpl"),
        ),
        (
            "tests/health.rs",
            include_str!("../templates/new/health_test.rs.tmpl"),
        ),
        (
            "Dockerfile",
            include_str!("../templates/new/Dockerfile.tmpl"),
        ),
        (
            ".dockerignore",
            include_str!("../templates/new/dockerignore.tmpl"),
        ),
        (
            ".gitignore",
            include_str!("../templates/new/gitignore.tmpl"),
        ),
        (
            ".env.example",
            include_str!("../templates/new/env.example.tmpl"),
        ),
        (
            ".github/workflows/ci.yml",
            include_str!("../templates/new/ci.yml.tmpl"),
        ),
        ("README.md", include_str!("../templates/new/README.md.tmpl")),
    ];
    let mut plan = Plan::new(root);
    for (rel, template) in files {
        plan.create(rel, render(template, &vars))?;
    }
    if let Some(template) = db_file
        && db.lumen()
    {
        let schema = if db == Db::LumenMongo {
            include_str!("../templates/lumen/schema-mongo.prisma.tmpl")
        } else {
            include_str!("../templates/lumen/schema-postgres.prisma.tmpl")
        };
        plan.create("prisma/schema.prisma", render(schema, &vars))?;
        plan.create("src/db/mod.rs", render(template, &vars))?;
        plan.create(
            "tests/db.rs",
            render(include_str!("../templates/lumen/test.rs.tmpl"), &vars),
        )?;
        let lib = add_mod(&plan.read("src/lib.rs")?, "pub mod db;");
        plan.update("src/lib.rs", lib)?;
        register_in_app(&mut plan, "db", "LumenModule")?;
    } else if let Some(template) = db_file {
        plan.create("src/db.rs", render(template, &vars))?;
        let lib = add_mod(&plan.read("src/lib.rs")?, "pub mod db;");
        plan.update("src/lib.rs", lib)?;
        register_in_app(&mut plan, "db", "DbModule")?;
    }
    Ok(plan)
}

/// A project made by `eaglet new` has these files.
fn check_project(plan: &Plan) -> Result<(), String> {
    for rel in ["Cargo.toml", "src/lib.rs", "src/app_module.rs"] {
        if !plan.exists(rel) {
            return Err(format!(
                "{} has no {rel}; run eaglet g in a project made by `eaglet new` (or pass --path)",
                plan.root().display()
            ));
        }
    }
    Ok(())
}

/// The crate name, from `Cargo.toml`.
fn crate_name(plan: &Plan) -> Result<String, String> {
    let toml = plan.read("Cargo.toml")?;
    toml.lines()
        .find_map(|l| {
            let rest = l
                .trim()
                .strip_prefix("name")?
                .trim_start()
                .strip_prefix('=')?;
            Some(rest.trim().trim_matches('"').replace('-', "_"))
        })
        .ok_or_else(|| "Cargo.toml has no package name".to_owned())
}

fn register_in_app(plan: &mut Plan, module_dir: &str, module: &str) -> Result<(), String> {
    let app = plan.read("src/app_module.rs")?;
    let app = add_use(&app, &format!("use crate::{module_dir}::{module};"));
    let app = add_to_module_list(&app, "imports", module).ok_or_else(|| {
        format!("src/app_module.rs has no #[module(...)]; add {module} to its imports by hand")
    })?;
    plan.update("src/app_module.rs", app)
}

/// Create `src/<name>/mod.rs` with an empty module, and import it in `AppModule`.
fn plan_module(plan: &mut Plan, n: &Names, vars: &[(&'static str, String)]) -> Result<(), String> {
    let dir = format!("src/{}", n.snake);
    if plan.exists(&format!("{dir}.rs")) {
        return Err(format!(
            "{dir}.rs is a single-file module; eaglet adds parts to folder modules ({dir}/mod.rs)"
        ));
    }
    plan.create(
        &format!("{dir}/mod.rs"),
        render_with(include_str!("../templates/parts/module.rs.tmpl"), vars),
    )?;
    let lib = add_mod(&plan.read("src/lib.rs")?, &format!("pub mod {};", n.snake));
    plan.update("src/lib.rs", lib)?;
    register_in_app(plan, &n.snake, &format!("{}Module", n.pascal))
}

/// Add `src/<name>/<file>.rs` to its module and list `item` under `key` in `#[module]`.
fn plan_part(
    plan: &mut Plan,
    n: &Names,
    vars: &[(&'static str, String)],
    (file, template, key, item): (&str, &str, &str, String),
) -> Result<(), String> {
    let module = format!("src/{}/mod.rs", n.snake);
    if !plan.exists(&module) {
        plan_module(plan, n, vars)?;
    }
    plan.create(
        &format!("src/{}/{file}.rs", n.snake),
        render_with(template, vars),
    )?;
    let src = plan.read(&module)?;
    let src = add_mod(&src, &format!("mod {file};"));
    let src = add_use(&src, &format!("pub use {file}::{item};"));
    let src = add_to_module_list(&src, key, &item)
        .ok_or_else(|| format!("{module} has no #[module(...)]; add {item} to {key} by hand"))?;
    plan.update(&module, src)
}

/// Plan `eaglet g <kind> <name>` in the project at `root`.
///
/// # Errors
/// Not an eaglet project, a bad name, or a file that already exists.
pub fn plan_generate(root: &Path, kind: Kind, name: &str) -> Result<Plan, String> {
    let n = Names::parse(name)?;
    let mut plan = Plan::new(root);
    check_project(&plan)?;
    let vars = part_vars(&n, &crate_name(&plan)?);
    match kind {
        Kind::Module => plan_module(&mut plan, &n, &vars)?,
        Kind::Controller => plan_part(
            &mut plan,
            &n,
            &vars,
            (
                "controller",
                include_str!("../templates/parts/controller.rs.tmpl"),
                "controllers",
                format!("{}Controller", n.pascal),
            ),
        )?,
        Kind::Service => plan_part(
            &mut plan,
            &n,
            &vars,
            (
                "service",
                include_str!("../templates/parts/service.rs.tmpl"),
                "providers",
                format!("{}Service", n.pascal),
            ),
        )?,
        Kind::Resource => plan_resource(&mut plan, &n, &vars)?,
    }
    Ok(plan)
}

fn plan_resource(
    plan: &mut Plan,
    n: &Names,
    vars: &[(&'static str, String)],
) -> Result<(), String> {
    let dir = format!("src/{}", n.snake);
    if plan.exists(&dir) || plan.exists(&format!("{dir}.rs")) {
        return Err(format!(
            "{dir} already exists; pick another name or add parts with eaglet g controller|service"
        ));
    }
    let files: [(&str, &str); 5] = [
        ("mod", include_str!("../templates/resource/mod.rs.tmpl")),
        (
            "controller",
            include_str!("../templates/resource/controller.rs.tmpl"),
        ),
        ("dto", include_str!("../templates/resource/dto.rs.tmpl")),
        (
            "repository",
            include_str!("../templates/resource/repository.rs.tmpl"),
        ),
        (
            "service",
            include_str!("../templates/resource/service.rs.tmpl"),
        ),
    ];
    for (file, template) in files {
        plan.create(&format!("{dir}/{file}.rs"), render_with(template, vars))?;
    }
    plan.create(
        &format!("tests/{}.rs", n.snake),
        render_with(include_str!("../templates/resource/test.rs.tmpl"), vars),
    )?;
    let lib = add_mod(&plan.read("src/lib.rs")?, &format!("pub mod {};", n.snake));
    plan.update("src/lib.rs", lib)?;
    register_in_app(plan, &n.snake, &format!("{}Module", n.pascal))
}
