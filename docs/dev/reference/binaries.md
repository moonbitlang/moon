# Finding toolchain binaries for `moon`

`moon` employs the following fallback list for finding toolchain binaries:

1. Use the path specified by override env var: `{binary.toupper()}_OVERRIDE`
2. Find the executable next to `moon`: `{current_exe}/../{binary}`
3. Find the executable in `MOON_HOME` (`~/.moon` if unset): `$MOON_HOME/bin/{binary}`
4. Fallback: just use the plain binary name, and rely on PATH
