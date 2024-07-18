// Copyright 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

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
