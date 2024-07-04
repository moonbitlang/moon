pub mod cmd_builder;
pub mod gen_build;
pub mod gen_bundle;
pub mod gen_check;
pub mod gen_runtest;
pub mod util;

// WORKAROUND for do not test coverage on coverage library itself
const MOON_CORE_COVERAGE_LIB: &str = "moonbitlang/core/coverage";
const MOON_CORE_BUILTIN_LIB: &str = "moonbitlang/core/builtin";

#[test]
fn test_start_with() {
    use moonutil::common::MOONBITLANG_CORE;
    assert!(MOON_CORE_COVERAGE_LIB.starts_with(MOONBITLANG_CORE));
    assert!(MOON_CORE_BUILTIN_LIB.starts_with(MOONBITLANG_CORE));
}

static SKIP_COVERAGE_LIBS: &[&str] = &[];
static SELF_COVERAGE_LIBS: &[&str] = &[MOON_CORE_BUILTIN_LIB, MOON_CORE_COVERAGE_LIB];

fn is_skip_coverage_lib(name: &str) -> bool {
    SKIP_COVERAGE_LIBS.contains(&name)
}

fn is_self_coverage_lib(name: &str) -> bool {
    SELF_COVERAGE_LIBS.contains(&name)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiAlias {
    pub name: String,
    pub alias: String,
}

impl PartialOrd for MiAlias {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.name.cmp(&other.name))
    }
}

impl Ord for MiAlias {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name.cmp(&other.name)
    }
}
