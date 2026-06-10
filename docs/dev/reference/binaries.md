# Finding toolchain binaries for `moon`

`moon` employs the following fallback list for finding toolchain binaries:

1. Use the path specified by override env var: `{binary.toupper()}_OVERRIDE`
2. Find the executable in the resolved toolchain root: `$MOON_TOOLCHAIN_ROOT/bin/{binary}`, the inferred current-executable toolchain root, or the legacy `$MOON_HOME/bin/{binary}` fallback
3. Resolve the executable from `PATH` to an absolute path
4. Fallback: just use the plain binary name, and rely on `PATH`

The code entry point is `moonutil::toolchain::BINARIES`, which re-exports the
cached binary resolver from `moonutil`. Build engines should use this entry
point when they need MoonBit tool executables, rather than duplicating binary
lookup rules locally.
