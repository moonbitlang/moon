//! Version utilities
use semver::{Comparator, Op, Version, VersionReq};

/// Converts a version into a semver comparator
pub fn as_comparator(version: Version, op: Op) -> Comparator {
    Comparator {
        op,
        major: version.major,
        minor: Some(version.minor),
        patch: Some(version.patch),
        pre: version.pre,
    }
}

/// Converts a version into a caret comparator
pub fn as_caret_comparator(version: Version) -> Comparator {
    as_comparator(version, Op::Caret)
}

/// Converts a version into a caret version requirement
pub fn as_caret_version_req(version: Version) -> VersionReq {
    VersionReq {
        comparators: vec![as_caret_comparator(version)],
    }
}
