// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
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
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Package and module name related structures

use std::{
    borrow::{Borrow, Cow},
    str::FromStr,
};

use arcstr::ArcStr;
use moonutil::mooncakes::{ModuleName, ModuleSource};
use relative_path::RelativePath;

pub const PACKAGE_SEGMENT_SEP: char = '/';

/// A fully-qualified package name, representing the full name of a package. For
/// example, `moonbitlang/core/builtin`. This type does *not* contain the
/// leading `@` that may occur in MoonBit source code. This type also contains
/// the version information of the module, although it is not by default
/// displayed.
///
/// This type is cheaply clonable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageFQN {
    module: ModuleSource,
    package: PackagePath,
}

impl PackageFQN {
    /// Construct a package FQN from its parts.
    pub fn new(module: ModuleSource, package: PackagePath) -> Self {
        Self { module, package }
    }

    /// Get the module name part of the fully-qualified name
    pub fn module(&self) -> &ModuleSource {
        &self.module
    }

    /// Get the unqualified package path part of the fully-qualified name.
    pub fn package(&self) -> &PackagePath {
        &self.package
    }

    /// Get the short name alias for this fully-qualified name.
    pub fn short_alias(&self) -> &str {
        self.package
            .short_name()
            .unwrap_or_else(|| self.module.name().last_segment())
    }

    /// Same as [`Self::short_alias`], but returns a ref-counted substring,
    /// preventing the frequent cloning of [`String`]s when converting to string.
    pub fn short_alias_owned(&self) -> arcstr::Substr {
        self.package
            .short_name_owned()
            .unwrap_or_else(|| self.module.name().last_segment_owned())
    }

    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.module.name().segments().chain(self.package.segments())
    }

    /// Check if `self` can import `dependency`, according to the
    /// internal-imports rule.
    ///
    /// internal imports only works between packages of the same module. If
    /// packages come from different modules, any `internal` packages should not
    /// be able to be imported.
    ///
    /// See the details of the rule in `docs/dev/reference/modules-packages.md`.
    pub fn can_import(&self, dependency: &Self) -> bool {
        let same_module = self.module == dependency.module;
        self.package.can_import(&dependency.package, same_module)
    }

    pub fn has_internal_segment(&self) -> bool {
        self.package().segments().any(|seg| seg == "internal")
    }
}

impl std::fmt::Display for PackageFQN {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write_package_fqn_to(f, self.module.name(), &self.package)
    }
}

/// Compare a package FQN to a tuple of (username, module, package). This does
/// not compare the module source, just the module name.
impl<'a> PartialEq<(&'a str, &'a str, &'a str)> for PackageFQN {
    fn eq(&self, &(user, module, package): &(&'a str, &'a str, &'a str)) -> bool {
        self.module.name().username == user
            && self.module.name().unqual == module
            && self.package.as_str() == package
    }
}

/// Compare the package name part of a package FQN to a string. This does not
/// compare the module source, just the module name.
impl PartialEq<str> for PackageFQN {
    fn eq(&self, other: &str) -> bool {
        let string_compared_segented = other.split(PACKAGE_SEGMENT_SEP);
        self.segments().eq(string_compared_segented)
    }
}

/// Write a package FQN to a formatter given module and package parts
pub fn write_package_fqn_to<W: std::fmt::Write>(
    f: &mut W,
    module: &ModuleName,
    package: &PackagePath,
) -> std::fmt::Result {
    write!(f, "{}", module)?;
    if package.is_empty() {
        Ok(())
    } else {
        f.write_char(PACKAGE_SEGMENT_SEP)?;
        write!(f, "{}", package)
    }
}

/// Format a package FQN as a string given module and package parts, without
/// constructing a PackageFQN.
pub fn format_package_fqn(module: &ModuleName, package: &PackagePath) -> String {
    let mut result = String::new();
    write_package_fqn_to(&mut result, module, package)
        .expect("writing to String should never fail");
    result
}

/// A wrapper around [`PackageFQN`] that displays the module source and version
/// information instead of just the module name when formatted.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageFQNWithSource {
    fqn: PackageFQN,
}

impl PackageFQNWithSource {
    /// Construct a package FQN with source from its parts.
    pub fn new(module: ModuleSource, package: PackagePath) -> Self {
        Self {
            fqn: PackageFQN::new(module, package),
        }
    }

    /// Construct from an existing PackageFQN.
    pub fn from_fqn(fqn: PackageFQN) -> Self {
        Self { fqn }
    }

    /// Get the underlying PackageFQN.
    pub fn fqn(&self) -> &PackageFQN {
        &self.fqn
    }
}

impl std::fmt::Display for PackageFQNWithSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.fqn, self.fqn.module().version())
    }
}

impl From<PackageFQN> for PackageFQNWithSource {
    fn from(fqn: PackageFQN) -> Self {
        Self::from_fqn(fqn)
    }
}

/// An optional wrapper around [`PackageFQNWithSource`] that displays "unknown" when None.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OptionalPackageFQNWithSource {
    inner: Option<PackageFQNWithSource>,
}

impl OptionalPackageFQNWithSource {
    /// Create a new optional package FQN with source from an Option.
    pub fn new(inner: Option<PackageFQNWithSource>) -> Self {
        Self { inner }
    }

    /// Create from an optional PackageFQN.
    pub fn from_optional_fqn(fqn: Option<PackageFQN>) -> Self {
        Self {
            inner: fqn.map(PackageFQNWithSource::from_fqn),
        }
    }

    /// Get the inner optional PackageFQNWithSource.
    pub fn inner(&self) -> &Option<PackageFQNWithSource> {
        &self.inner
    }
}

impl std::fmt::Display for OptionalPackageFQNWithSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.inner {
            Some(fqn) => write!(f, "{}", fqn),
            None => write!(f, "unknown"),
        }
    }
}

impl From<Option<PackageFQNWithSource>> for OptionalPackageFQNWithSource {
    fn from(inner: Option<PackageFQNWithSource>) -> Self {
        Self::new(inner)
    }
}

impl From<Option<PackageFQN>> for OptionalPackageFQNWithSource {
    fn from(fqn: Option<PackageFQN>) -> Self {
        Self::from_optional_fqn(fqn)
    }
}

/// An unqualified package path, representing the non-module portion of the
/// package. For example, the `builtin` in `moonbitlang/core/builtin`.
/// This path may contain multiple segments (like `immut/linked_list`), or zero
/// segments (representing the root package within the module). Segments are
/// separated by forward slash `/`.
///
/// This type is cheaply clonable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackagePath {
    /// The full name of the package, separated by forward slash `/`
    value: ArcStr,
}

impl PackagePath {
    pub fn validate(s: &str) -> Result<(), PackagePathParseError> {
        if s.is_empty() {
            return Ok(());
        }
        for seg in s.split(PACKAGE_SEGMENT_SEP) {
            if seg.is_empty() {
                return Err(PackagePathParseError::EmptySegment);
            }
            if seg == "." || seg == ".." {
                return Err(PackagePathParseError::PathNotNormalized);
            }
        }

        Ok(())
    }

    /// Construct a new package path from a string slice.
    ///
    /// # Safety
    ///
    /// This constructor does not validate the path.
    pub unsafe fn new_unchecked(s: &str) -> Self {
        Self {
            value: ArcStr::from_str(s).unwrap(),
        }
    }

    /// Construct a new package path without copying string data by cloning an
    /// underlying reference-counted string.
    ///
    /// # Safety
    ///
    /// This constructor does not validate the path.
    pub unsafe fn new_no_copy_unchecked(s: ArcStr) -> Self {
        Self { value: s }
    }

    /// Construct a new package path from a string slice with validation.
    pub fn new(s: &str) -> Result<Self, PackagePathParseError> {
        Self::validate(s)?;
        Ok(unsafe { Self::new_unchecked(s) })
    }

    /// Construct a new package path without copying string data by cloning an
    /// underlying reference-counted string with validation.
    pub fn new_no_copy(s: ArcStr) -> Result<Self, PackagePathParseError> {
        Self::validate(&s)?;
        Ok(unsafe { Self::new_no_copy_unchecked(s) })
    }

    /// Construct a new package path from a [`RelativePath`]. This process
    /// normalizes the path.
    pub fn new_from_rel_path(path: &RelativePath) -> Result<Self, PackagePathParseError> {
        let normalized = if path.is_normalized() {
            Cow::Borrowed(path)
        } else {
            Cow::Owned(path.normalize())
        };
        // Check for parent segments `../`
        // If a normalized path actually descends into its parent directory,
        // checking whether it **starts with** `..` should be enough, because
        // any other will be already normalized out.
        if normalized.starts_with("..") {
            return Err(PackagePathParseError::PathDescendsIntoParent);
        }
        // Note: Specifically, ".".normalize() == "", so we do not need to
        // specially handle the current directory path ".".
        unsafe { Ok(Self::new_unchecked(path.as_str())) }
    }

    /// Whether this is an empty package path (i.e. root package within a module)
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// Construct an empty package path.
    pub fn empty() -> Self {
        unsafe { Self::new_no_copy_unchecked(arcstr::literal!("")) }
    }

    /// Returns an iterator of segments of the package path.
    pub fn segments(&self) -> SegmentIter<'_> {
        if self.is_empty() {
            SegmentIter::Empty
        } else {
            SegmentIter::HasSegments(self.value.split(PACKAGE_SEGMENT_SEP))
        }
    }

    /// Get the short name of this package, which is the last segment of the
    /// package. If this is the root package, returns [None].
    pub fn short_name(&self) -> Option<&str> {
        self.segments().next_back()
    }

    pub fn short_name_owned(&self) -> Option<arcstr::Substr> {
        self.short_name().map(|x| self.value.substr_from(x))
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }

    pub fn underlying(&self) -> &ArcStr {
        &self.value
    }

    /// Construct the parent package path of self. If empty, returns [None].
    ///
    /// As the package paths do not imply any parent/child relationship, this
    /// function is expected to be called sparingly.
    pub fn parent(&self) -> Option<PackagePath> {
        if self.is_empty() {
            return None;
        }
        let last_slash_start = self.value.rfind(PACKAGE_SEGMENT_SEP).unwrap_or(0);
        unsafe { Some(Self::new_unchecked(&self.value[..last_slash_start])) }
    }

    /// Check if `self` can import `dependency`, according to the
    /// internal-imports rule.
    ///
    /// internal imports only works between packages of the same module. If
    /// packages come from different modules, any `internal` packages should not
    /// be able to be imported.
    ///
    /// See the details of the rule in `docs/dev/reference/modules-packages.md`.
    pub fn can_import(&self, dependency: &PackagePath, same_module: bool) -> bool {
        // Determine if `dependency` has an internal component, get the last
        // `internal` segment within the path.
        let mut internal_pos = None;
        for (ix, seg) in dependency.segments().enumerate() {
            if seg == "internal" {
                internal_pos = Some(ix)
            }
        }

        // If no internal is detected, import is allowed
        let Some(internal_pos) = internal_pos else {
            return true;
        };
        // If not in the same module, internal import is forbidden.
        if !same_module {
            return false;
        }

        // Now we check if path segments match before the internal segment
        self.segments()
            .take(internal_pos)
            .eq(dependency.segments().take(internal_pos))
    }
}

impl std::fmt::Display for PackagePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.value)
    }
}

impl FromStr for PackagePath {
    type Err = PackagePathParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::validate(s)?;
        Ok(unsafe { Self::new_unchecked(s) })
    }
}

impl Borrow<str> for PackagePath {
    fn borrow(&self) -> &str {
        &self.value
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PackagePathParseError {
    #[error("The package path being parsed contains an empty segment")]
    EmptySegment,

    #[error("The provided path is not normalized")]
    PathNotNormalized,

    #[error("The provided path descends into its parent directory `..`")]
    PathDescendsIntoParent,
}

pub enum SegmentIter<'a> {
    Empty,
    HasSegments(std::str::Split<'a, char>),
}

impl<'a> Iterator for SegmentIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Empty => None,
            Self::HasSegments(x) => x.next(),
        }
    }
}

impl<'a> DoubleEndedIterator for SegmentIter<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            Self::Empty => None,
            Self::HasSegments(x) => x.next_back(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod test {
    use super::*;
    // Be explicit to avoid any module path ambiguity in tests.
    use PackagePathParseError::*;

    fn assert_valid_pkg_path(s: &str) -> PackagePath {
        s.parse()
            .unwrap_or_else(|e| panic!("Expected {:?} to be a valid package path, got {:?}", s, e))
    }

    fn assert_invalid_pkg_path(s: &str, e: PackagePathParseError) {
        let parsed = s.parse::<PackagePath>();
        match parsed {
            Ok(res) => panic!(
                "Expected {:?} to be an invalid package path, got {:?}",
                s, res
            ),
            Err(actual) => {
                if e != actual {
                    panic!(
                        "Expected {:?} to be an invalid package path with error {:?}, got {:?}",
                        s, e, actual
                    );
                }
            }
        }
    }

    #[test]
    fn test_pkg_path_ctor() {
        assert_valid_pkg_path("");
        assert_valid_pkg_path("path");
        assert_valid_pkg_path("two/levels");
        assert_valid_pkg_path("i/am/pretty/deep");

        assert_invalid_pkg_path("/i_start_with_empty", EmptySegment);
        assert_invalid_pkg_path("i//contain_empty", EmptySegment);
        assert_invalid_pkg_path("i_end_with_empty/", EmptySegment);
    }

    #[test]
    fn test_pkg_path_segments() {
        let empty_path = assert_valid_pkg_path("");
        assert_eq!(
            empty_path.segments().collect::<Vec<_>>(),
            Vec::<&str>::new()
        );

        let single_path = assert_valid_pkg_path("single");
        assert_eq!(single_path.segments().collect::<Vec<_>>(), vec!["single"]);

        let two_level_path = assert_valid_pkg_path("two/levels");
        assert_eq!(
            two_level_path.segments().collect::<Vec<_>>(),
            vec!["two", "levels"]
        );

        let deep_path = assert_valid_pkg_path("very/deep/nested/path");
        assert_eq!(
            deep_path.segments().collect::<Vec<_>>(),
            vec!["very", "deep", "nested", "path"]
        );

        // Test that segments iterator can be used multiple times
        let path = assert_valid_pkg_path("a/b/c");
        let segments1: Vec<_> = path.segments().collect();
        let segments2: Vec<_> = path.segments().collect();
        assert_eq!(segments1, segments2);
        assert_eq!(segments1, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_pkg_path_parent() {
        // Empty path has no parent
        let empty_path = assert_valid_pkg_path("");
        assert_eq!(empty_path.parent(), None);

        // Single segment path has empty parent
        let single_path = assert_valid_pkg_path("single");
        let parent = single_path.parent().unwrap();
        assert_eq!(parent.as_str(), "");
        assert!(parent.is_empty());

        // Two level path
        let two_level_path = assert_valid_pkg_path("two/levels");
        let parent = two_level_path.parent().unwrap();
        assert_eq!(parent.as_str(), "two");

        // Deep nested path
        let deep_path = assert_valid_pkg_path("very/deep/nested/path");
        let parent = deep_path.parent().unwrap();
        assert_eq!(parent.as_str(), "very/deep/nested");

        // Test chaining parent calls
        let path = assert_valid_pkg_path("a/b/c/d");
        let parent1 = path.parent().unwrap();
        assert_eq!(parent1.as_str(), "a/b/c");

        let parent2 = parent1.parent().unwrap();
        assert_eq!(parent2.as_str(), "a/b");

        let parent3 = parent2.parent().unwrap();
        assert_eq!(parent3.as_str(), "a");

        let parent4 = parent3.parent().unwrap();
        assert_eq!(parent4.as_str(), "");
        assert!(parent4.is_empty());

        let parent5 = parent4.parent();
        assert_eq!(parent5, None);
    }

    #[test]
    fn test_pkg_path_short_name() {
        // Empty path has no short name
        let empty_path = assert_valid_pkg_path("");
        assert_eq!(empty_path.short_name(), None);

        // Single segment path
        let single_path = assert_valid_pkg_path("single");
        assert_eq!(single_path.short_name(), Some("single"));

        // Two level path
        let two_level_path = assert_valid_pkg_path("two/levels");
        assert_eq!(two_level_path.short_name(), Some("levels"));

        // Deep nested path
        let deep_path = assert_valid_pkg_path("very/deep/nested/path");
        assert_eq!(deep_path.short_name(), Some("path"));

        // Path with similar segments
        let similar_path = assert_valid_pkg_path("foo/bar/foo");
        assert_eq!(similar_path.short_name(), Some("foo"));
    }

    #[test]
    fn test_package_fqn_shortname() {
        // Helper to create ModuleName
        let create_module = |s: &str| ModuleSource::from_str(s).unwrap();

        // Package with non-empty path - should use package short name
        let module = create_module("moonbitlang/core@0.1.0");
        let package = assert_valid_pkg_path("collections/list");
        let fqn = PackageFQN::new(module, package);
        assert_eq!(fqn.short_alias(), "list");

        // Package with empty path - should use module last segment
        let module = create_module("moonbitlang/core@0.1.0");
        let package = assert_valid_pkg_path("");
        let fqn = PackageFQN::new(module, package);
        assert_eq!(fqn.short_alias(), "core");

        // Single segment package path
        let module = create_module("myorg/mymodule@1.2.3");
        let package = assert_valid_pkg_path("utils");
        let fqn = PackageFQN::new(module, package);
        assert_eq!(fqn.short_alias(), "utils");

        // Deep package path
        let module = create_module("company/project@2.0.0");
        let package = assert_valid_pkg_path("internal/data/structures");
        let fqn = PackageFQN::new(module, package);
        assert_eq!(fqn.short_alias(), "structures");

        // Edge cases -- legacy module names
        // Single segment module name with empty package
        let module = create_module("single@1.0.0");
        let package = assert_valid_pkg_path("");
        let fqn = PackageFQN::new(module, package);
        assert_eq!(fqn.short_alias(), "single");

        // Single segment module name with package
        let module = create_module("single@1.0.0");
        let package = assert_valid_pkg_path("subpackage");
        let fqn = PackageFQN::new(module, package);
        assert_eq!(fqn.short_alias(), "subpackage");

        // Triple segment module name with empty package
        let module = create_module("org/project/module@1.5.2");
        let package = assert_valid_pkg_path("");
        let fqn = PackageFQN::new(module, package);
        assert_eq!(fqn.short_alias(), "module");

        // Triple segment module name with package
        let module = create_module("org/project/module@1.5.2");
        let package = assert_valid_pkg_path("utils/helpers");
        let fqn = PackageFQN::new(module, package);
        assert_eq!(fqn.short_alias(), "helpers");

        // Four segment module name with empty package
        let module = create_module("company/division/project/module@3.1.4");
        let package = assert_valid_pkg_path("");
        let fqn = PackageFQN::new(module, package);
        assert_eq!(fqn.short_alias(), "module");

        // Four segment module name with single segment package
        let module = create_module("company/division/project/module@3.1.4");
        let package = assert_valid_pkg_path("core");
        let fqn = PackageFQN::new(module, package);
        assert_eq!(fqn.short_alias(), "core");
    }

    fn mk_fqn(module: &str, pkg_path: &str) -> PackageFQN {
        let module = ModuleSource::from_str(module).unwrap();
        let pkg: PackagePath = pkg_path.parse().unwrap();
        PackageFQN::new(module, pkg)
    }

    // Tests directly mirroring the examples table in the documentation:
    // docs/dev/reference/modules-packages.md (Internal packages).
    #[test]
    fn test_internal_examples_from_docs() {
        // user/pkg/a            -> user/pkg/b            : Yes, no internal involved
        assert!(mk_fqn("user/pkg@0.1.0", "a").can_import(&mk_fqn("user/pkg@0.1.0", "b")));

        // user/another/e        -> user/pkg/a            : Yes, no internal involved
        assert!(mk_fqn("user/another@0.1.0", "e").can_import(&mk_fqn("user/pkg@0.1.0", "a")));

        // user/pkg/a            -> user/pkg/a/internal   : Yes, shares common prefix
        assert!(mk_fqn("user/pkg@0.1.0", "a").can_import(&mk_fqn("user/pkg@0.1.0", "a/internal")));

        // user/pkg/a            -> user/pkg/a/internal/b : Yes, shares common prefix
        assert!(
            mk_fqn("user/pkg@0.1.0", "a").can_import(&mk_fqn("user/pkg@0.1.0", "a/internal/b"))
        );

        // user/pkg/a/internal/b -> user/pkg/a/internal/c : Yes, shares common prefix
        assert!(
            mk_fqn("user/pkg@0.1.0", "a/internal/b")
                .can_import(&mk_fqn("user/pkg@0.1.0", "a/internal/c"))
        );

        // user/pkg/a/internal/b -> user/pkg/a            : Yes, no internal involved
        assert!(
            mk_fqn("user/pkg@0.1.0", "a/internal/b").can_import(&mk_fqn("user/pkg@0.1.0", "a"))
        );

        // user/pkg/a/internal/b -> user/pkg/d            : Yes, no internal involved
        assert!(
            mk_fqn("user/pkg@0.1.0", "a/internal/b").can_import(&mk_fqn("user/pkg@0.1.0", "d"))
        );

        // user/pkg/d            -> user/pkg/a/internal/b : No, no common prefix up to internal
        assert!(
            !mk_fqn("user/pkg@0.1.0", "d").can_import(&mk_fqn("user/pkg@0.1.0", "a/internal/b"))
        );

        // user/pkg/d/internal/f -> user/pkg/a/internal/b : No, no common prefix up to internal
        assert!(
            !mk_fqn("user/pkg@0.1.0", "d/internal/f")
                .can_import(&mk_fqn("user/pkg@0.1.0", "a/internal/b"))
        );

        // user/another/e        -> user/pkg/a/internal/b : No, different module
        assert!(
            !mk_fqn("user/another@0.1.0", "e")
                .can_import(&mk_fqn("user/pkg@0.1.0", "a/internal/b"))
        );
    }
}
