# Toolchain packaging layouts and discovery

Status: design note for toolchain packaging and discovery. The goal is to separate immutable toolchain payloads from mutable user state before deciding between a Go-like or rustup-like selection model.

## Core recommendation

Use one logical toolchain root with the same internal layout on every platform:

```text
<toolchain-root>/
  bin/
  lib/
  include/
  share/
```

The package-manager-specific namespace belongs in the install prefix, not inside the toolchain tree.

Examples:

```text
/opt/homebrew/Cellar/moonbit/<ver>/libexec/{bin,lib,include,share}
/usr/lib/moonbit/{bin,lib,include,share}
/nix/store/<hash>-moonbit-<ver>/{bin,lib,include,share}
C:\Program Files\MoonBit\{bin,lib,include,share}
```

This means we do **not** need to redesign the internal tree into `lib/moonbit/...` or `include/moonbit/...` just to satisfy a package manager. The package manager chooses the prefix; the toolchain keeps a stable internal shape.

Mutable user state must stay outside the package-managed prefix.

## Packaging layouts by system

### Homebrew formula

Recommended MoonBit layout:

```text
$(brew --prefix)/bin/moon           -> symlink/wrapper to the real binary
$(brew --prefix)/bin/moonc          -> symlink/wrapper to the real binary
$(brew --opt moonbit)/libexec/bin/
$(brew --opt moonbit)/libexec/lib/
$(brew --opt moonbit)/libexec/include/
$(brew --opt moonbit)/libexec/share/
$(brew --opt moonbit)/share/man/
$(brew --opt moonbit)/share/doc/moonbit/
```

Why:

- Homebrew installs formulae in the Cellar and then symlinks selected files into the main prefix.
- `libexec` is explicitly reserved for private, non-symlinked files.
- Homebrew documents `bin`, `lib`, `include`, `share`, `libexec`, and their `opt_*` stable paths.

Implication for MoonBit:

- The real toolchain root should be `libexec`.
- Public entry points in `$(brew --prefix)/bin` should delegate to `$(brew --opt moonbit)/libexec/bin/...`.
- User state must not live under the Cellar or `libexec`.

### Debian / Ubuntu (`.deb`)

Recommended MoonBit layout:

```text
/usr/bin/moon
/usr/bin/moonc
/usr/lib/moonbit/bin/
/usr/lib/moonbit/lib/
/usr/lib/moonbit/include/
/usr/lib/moonbit/share/
/usr/share/man/man1/
/usr/share/doc/moonbit/
```

Why:

- Debian policy explicitly allows a package-specific subdirectory under `/usr/lib` to hold a mixture of architecture-dependent and architecture-independent files.
- Debian also allows multiarch library/header locations under `/usr/lib/<triplet>` and `/usr/include/<triplet>` when needed.
- Manual pages go under `/usr/share/man`, and additional documentation goes under `/usr/share/doc/<package>`.

Implication for MoonBit:

- A self-contained tree under `/usr/lib/moonbit` is policy-friendly.
- `/usr/bin/moon` and `/usr/bin/moonc` should be the public commands.
- If MoonBit ever needs multiarch-specific headers or libraries, Debian policy already has a place for them.

### Fedora / RPM

Recommended MoonBit layout for a self-contained RPM payload:

```text
%{_bindir}/moon
%{_bindir}/moonc
%{_libdir}/moonbit/bin/
%{_libdir}/moonbit/lib/
%{_libdir}/moonbit/include/
%{_libdir}/moonbit/share/
%{_mandir}/man1/
%{_docdir}/moonbit/
```

Why:

- Fedora follows the FHS closely.
- Fedora packaging guidelines reserve `%{_libexecdir}` for executables that are primarily run by other programs.
- The same guidelines say `%{_libdir}/%{name}` is a valid second choice when `%{_libexecdir}` is not the right fit.

Implication for MoonBit:

- Because MoonBit needs one mixed tree containing `bin`, `lib`, `include`, and `share`, `%{_libdir}/moonbit` is the simpler fit for the full toolchain root.
- If we later split helper executables from the rest of the payload, `%{_libexecdir}/moonbit` becomes a possible home for those helper executables only.

### Nix

Recommended MoonBit layout:

```text
/nix/store/<hash>-moonbit-<ver>/bin/
/nix/store/<hash>-moonbit-<ver>/lib/
/nix/store/<hash>-moonbit-<ver>/include/
/nix/store/<hash>-moonbit-<ver>/share/
~/.nix-profile/bin/moon            -> profile symlink into the store
```

Why:

- Nix installs packages into immutable store paths under `/nix/store`.
- User- or system-visible environments are profiles, which are trees of symlinks into the store.

Implication for MoonBit:

- The actual toolchain root is the store path.
- Relative-path inference is acceptable as long as the real executable path resolves into the store tree, or the profile tree preserves the expected top-level layout.
- Mutable user state must never be written into the Nix store.

### Windows installer (MSI / EXE / ZIP)

Recommended MoonBit layout for machine-wide install:

```text
%ProgramFiles%\MoonBit\bin\moon.exe
%ProgramFiles%\MoonBit\bin\moonc.exe
%ProgramFiles%\MoonBit\lib\
%ProgramFiles%\MoonBit\include\
%ProgramFiles%\MoonBit\share\
```

Recommended MoonBit layout for per-user install:

```text
%LOCALAPPDATA%\Programs\MoonBit\bin\moon.exe
%LOCALAPPDATA%\Programs\MoonBit\bin\moonc.exe
%LOCALAPPDATA%\Programs\MoonBit\lib\
%LOCALAPPDATA%\Programs\MoonBit\include\
%LOCALAPPDATA%\Programs\MoonBit\share\
```

Recommended mutable user state:

```text
%LOCALAPPDATA%\MoonBit\...
```

Why:

- Windows exposes fixed install roots such as `%ProgramFiles%` for machine-wide software.
- Windows also exposes per-user roots such as `%LOCALAPPDATA%` and `%LOCALAPPDATA%\Programs`.

Implication for MoonBit:

- The immutable toolchain payload should live under Program Files or the per-user Programs directory.
- Mutable user data should live under LocalAppData, not next to the installed binaries.

### WinGet

WinGet has two relevant cases.

Non-portable installers (`msi`, `exe`, `msix`, etc.):

- WinGet launches the package's installer.
- The package layout should therefore match the normal Windows installer layout above.

Portable installers:

- WinGet documents default portable roots of:
  - `%LOCALAPPDATA%/Microsoft/WinGet/Packages/` for user scope
  - `%PROGRAMFILES%/WinGet/Packages/` for machine scope

Recommended portable MoonBit layout:

```text
%LOCALAPPDATA%\Microsoft\WinGet\Packages\MoonBit...\bin\moon.exe
%LOCALAPPDATA%\Microsoft\WinGet\Packages\MoonBit...\lib\
%LOCALAPPDATA%\Microsoft\WinGet\Packages\MoonBit...\include\
%LOCALAPPDATA%\Microsoft\WinGet\Packages\MoonBit...\share\
```

Implication for MoonBit:

- WinGet portable packages also fit the same internal `{bin,lib,include,share}` tree.
- The only difference is the package-manager-controlled outer prefix.

## How Go and Rust discover a toolchain

### Go

Go uses two related mechanisms.

1. `cmd/go` discovers `GOROOT`.

   `findGOROOT` uses the following order:

   - explicit `GOROOT`
   - infer a `GOROOT` from the current `go` executable path
   - fall back to the `runtime.GOROOT()` value baked into the binary

2. `cmd/go` may switch toolchains.

   `toolchain.Select` and `toolchain.Exec` can:

   - keep using the local `go` binary
   - look for a requested toolchain in `PATH`
   - download a `golang.org/toolchain` module into the module cache
   - re-exec the selected `go` binary

Implications:

- Go is **not** purely “just infer from the current executable”.
- It does use relative-path-style discovery for the local installation root.
- But modern Go also has an early selection-and-reexec step when `GOTOOLCHAIN`, `go.mod`, or `go.work` require another toolchain.
- When Go downloads another toolchain, it stores it as a `golang.org/toolchain` module in the module cache instead of using a separate rustup-style toolchain registry.

### Rust

Rust also uses two related mechanisms, but the split is different.

1. `rustup` selects the active toolchain.

   `rustup` installs proxies in `~/.cargo/bin` and chooses the active toolchain using rules such as:

   - `cargo +nightly`
   - `RUSTUP_TOOLCHAIN`
   - directory overrides
   - `rust-toolchain.toml`
   - the default toolchain

   In a normal rustup install, commands such as `cargo`, `rustc`, and `rustdoc` in `~/.cargo/bin` are those proxies.

2. The real `rustc` discovers its sysroot.

   `rustc_session::filesearch::default_sysroot`:

   - first tries to infer the sysroot from `argv[0]` without canonicalizing path components
   - if that does not work, falls back to deriving the sysroot from the loaded `rustc_driver` library path

Implications:

- Rust toolchain **selection** is proxy-based and handled by `rustup`, not by `cargo` or `rustc` alone.
- But the compiler payload discovery inside the selected toolchain still uses relative-path inference.

## What this means for MoonBit

The following parts are independent of the eventual selection model.

We should do them regardless:

1. Separate immutable toolchain payloads from mutable user state.
2. Keep a stable internal toolchain tree: `{bin,lib,include,share}`.
3. Resolve toolchain-relative assets from a toolchain root, not from `MOON_HOME`.
4. Keep `MOON_HOME` for mutable state only.

The following can still be decided later:

1. Go-like model: `moon` auto-selects and possibly re-execs another toolchain.
2. rustup-like model: `moonup` proxies select the toolchain first.

The important observation is that **relative-path inference for the selected toolchain root is acceptable in both worlds**.

## References

### Package layout references

- Homebrew Formula Cookbook: <https://docs.brew.sh/Formula-Cookbook>
- Debian Policy, file system hierarchy: <https://www.debian.org/doc/debian-policy/ch-opersys.html>
- Debian Policy, documentation: <https://www.debian.org/doc/debian-policy/ch-docs.html>
- Fedora Packaging Guidelines: <https://docs.fedoraproject.org/en-US/packaging-guidelines/>
- Nix profiles reference: <https://releases.nixos.org/nix/nix-2.33.0/manual/command-ref/files/profiles.html>
- Windows known folders (`Program Files`, `LocalAppData`): <https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid>
- WinGet settings (`portablePackageUserRoot`, `portablePackageMachineRoot`): <https://learn.microsoft.com/en-us/windows/package-manager/winget/settings>

### Go references

- Go toolchain docs: <https://go.dev/doc/toolchain>
- `findGOROOT` source permalink: <https://github.com/golang/go/blob/a3688ab13e76762a168f43e91ca9422c847ee896/src/cmd/go/internal/cfg/cfg.go#L548-L615>
- `toolchain.Exec` source permalink: <https://github.com/golang/go/blob/a3688ab13e76762a168f43e91ca9422c847ee896/src/cmd/go/internal/toolchain/select.go#L302-L422>

### Rust references

- rustup concepts: <https://rust-lang.github.io/rustup/concepts/index.html>
- rustup proxies: <https://rust-lang.github.io/rustup/concepts/proxies.html>
- rustup overrides: <https://rust-lang.github.io/rustup/overrides.html>
- `rustc` sysroot discovery source permalink (rustc 1.90.0): <https://github.com/rust-lang/rust/blob/1159e78c4747b02ef996e55082b704c09b970588/compiler/rustc_session/src/filesearch.rs#L187-L255>
