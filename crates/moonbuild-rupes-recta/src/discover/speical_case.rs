use moonutil::mooncakes::result::ResolvedEnv;
use tracing::instrument;

use crate::{discover::DiscoverResult, special_cases::CORE_MODULE_TUPLE};

/// Inject `moonbitlang/core/abort` to the package graph, so that user packages
/// can override it.
#[instrument(skip_all)]
pub fn inject_std_abort(env: &ResolvedEnv, packages: &mut DiscoverResult) {
    // Don't inject if we or anybody we know is already core
    if env.all_modules().any(|x| x.name() == &CORE_MODULE_TUPLE) {
        return;
    }
}
