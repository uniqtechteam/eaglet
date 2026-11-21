# Contributing

Thanks for helping. A few things make a pull request easy to merge:

- **Open an issue first** for anything larger than a small fix, so we can agree on the shape. A small
  project that shows the problem is the most useful bug report.
- **One change per pull request**, with a short description of what and why, and how you tested it.
- **Tests** come with every new feature and every bug fix.
- **Checks** must pass (CI runs the same):

  ```sh
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  cargo deny check
  ```

  `cargo test --workspace` needs no database. The tests that run `eaglet new` and `eaglet g` and then
  build the generated projects are slow, so they are ignored by default; run them when you change the
  CLI or its templates:

  ```sh
  cargo test -p eaglet-cli --test generated -- --ignored --test-threads 2
  ```

  The projects generated with `--db lumen-*` are only built when `EAGLET_TEST_LUMEN_DIR` points at a
  lumen checkout.

- **Feature sets**: eaglet builds with and without its optional features, and CI runs clippy on each:

  ```sh
  cargo clippy -p eaglet --lib --no-default-features -- -D warnings
  cargo clippy -p eaglet --lib --no-default-features --features openapi -- -D warnings
  cargo clippy -p eaglet --lib --all-features -- -D warnings
  ```

- **Docs**: every Rust block in the README and in `docs/` is copied from code that builds in this
  repository, and `crates/eaglet/tests/docs.rs` fails when one drifts. Change both together.
- **Compile errors**: the messages of the macros are checked by trybuild tests in
  `crates/eaglet/tests/ui`. If you change one on purpose, refresh the expected output with
  `TRYBUILD=overwrite cargo test -p eaglet --test macros` and read the diff.
- **CHANGELOG.md**: add a line under *Unreleased*, written for people who use eaglet.
- Be kind.

By contributing you agree that your work is licensed under MIT OR Apache-2.0, like the rest of the
project.
