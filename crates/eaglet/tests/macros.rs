//! The macros explain wrong usage at compile time.

#[test]
fn wrong_usage_gives_clear_errors() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
