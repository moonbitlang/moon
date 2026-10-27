# Moon home layout

`MOON_HOME` is the root of mutable per-user state. It is logically distinct
from the selected MoonBit toolchain root, which owns immutable installed files
such as `bin`, `lib`, `include`, and `share`. The default self-contained
installation currently selects `MOON_HOME` as its toolchain root, so the two
trees overlap unless `MOON_TOOLCHAIN_ROOT` or executable discovery selects a
different installation.

When `MOON_HOME` is unset, Moon uses `.moon` under the operating system's user
home directory. When it is set, its value is used directly.

## Implemented layout

The complete persistent tree that production code currently creates is:

```text
$MOON_HOME/
  bin/
  cache/
    build/
      .moon-cache
    deps/
      .moon-cache
      .moon-lock
      v1/sources/<username>/<encoded-module>/<version>/
        .moon-source-archive-checksum
        ...
  registry/
    .moon-lock
    .registry-update-state.json
    index/
      .git/
      user/<username>/<module>.index
      ...
    symbols/
      ...
    cache/
      <username>/<module>/<version>.zip
      <username>/<module>/<version>.zip.lock
      assets/<username>/<module>/<version>/<package...>/
        .moon-lock
        <artifact>
  config.json
  credentials.json
```

- `bin` contains executables installed for the user. In the default
  self-contained installation, the selected toolchain root is also
  `MOON_HOME`, so this directory additionally contains toolchain executables.
  Package-managed toolchains separate those roles by selecting another
  toolchain root.
- `cache/build` and `cache/deps` are the default Moon-owned global cache roots.
  `MOON_BUILD_CACHE` and `MOON_DEP_CACHE` may replace or disable those roots.
  Each initialized root contains a `.moon-cache` ownership marker. The build
  cache has no artifact layout yet. The dependency cache uses one root lock
  and stores immutable source trees below `v1/sources`; `/` in a module's
  unqualified name is encoded as `+` there.
- `registry/index` is the local Git checkout of the registry index. New clones
  are shallow and exclude tags. Updates fetch `main` with depth 1 and reset the
  tracked checkout to the fetched commit, reusing cached Git objects without
  merging or rebasing. Existing full clones also get a shallow boundary at the
  fetched commit; previously stored objects are not immediately removed.
- Module metadata is read from
  `registry/index/user/<username>/<module>.index`. Each JSONL release record
  requires a version and checksum. An omitted `deps` field means an empty
  dependency map; a present `deps` field must be an object.
- `registry/symbols` is the symbols archive materialized by registry sync.
- Verified source archives use
  `registry/cache/<username>/<module>/<version>.zip`. The adjacent
  `.zip.lock` serializes publication of one module version.
- Cached executable artifacts use a separate `assets` namespace. Module,
  version, and package segments form directories below it; `.moon-lock`
  serializes publication within one package directory.
- `registry/.moon-lock` serializes index and symbols updates.
  `.registry-update-state.json` lets callers that waited for that lock reuse a
  concurrent update. Lock files remain in place after unlocking so all
  processes continue to lock the same filesystem object.
- `config.json` stores registry configuration. New configurations use independent
  `api`, `index`, and `download` URLs: dynamic registry operations use `api`,
  the local Git checkout uses `index`, and immutable package archives,
  prebuilt Wasm artifacts, checksums, and `symbols.zip` use `download`.
  Existing configurations with `registry`, `index`, and optional `symbols`
  remain supported with their original URL layout.
- `credentials.json` stores Mooncakes login credentials (`token`, optional
  `username`, and optional `registry_api`). Dynamic registry API reads attach
  the saved token as Bearer authentication only when `registry_api` matches
  the configured API base URL. Older credentials without this binding apply
  only to `https://mooncakes.io`; custom registry users must log in again with
  a client that records the binding. This remains one saved login, not a
  multiple-registry credential store.
- Credential matching normalizes URL spelling, including default ports and
  trailing slashes, but preserves scheme, port, and API base path. Authenticated
  requests require HTTPS except on loopback addresses. API redirects may stay
  within the same origin and API base path; redirects outside that boundary
  are not followed. Git index, symbols, source archive and prebuilt artifact
  requests do not load or send these API credentials.
- Missing or empty credentials leave API reads anonymous. Malformed or
  unreadable credentials produce a local error without including token text.
  Token validity and public/private access decisions belong to the registry;
  the client neither validates a token ahead of the request nor retries a
  rejected authenticated request anonymously.

The layout deliberately records current behavior rather than claiming every
choice is ideal. In particular, it makes three asymmetries visible for future
design work:

- registry source archives and executable artifacts use different nesting and
  lock scopes;
- the dependency source cache encodes nested module names with `+`, while the
  registry cache represents them as path segments; and
- registry source downloads and downloaded Wasm executables share one `registry/cache` tree.

This refactor does not change those paths. Changes to them require a migration
and compatibility decision rather than an incidental `PathBuf::join` edit.

## Code ownership

`moonutil::MoonHomeLayout` owns paths relative to one Moon home root.
`moonutil::MOON_HOME` is the process-wide layout selected from the environment.

`MoonHomeLayout` owns every fixed data path rooted directly in `MOON_HOME`,
including registry entries, archives, executable artifacts, and update state.
`moonutil::locks` owns the adjacent `<filename>.lock` and directory-local
`.moon-lock` conventions. Configurable cache implementations own their
versioned contents below the selected cache root. Behavioral owners remain
separate:

- `RegistryClient` owns registry synchronization, validation, downloads, and
  cache publication.
- `moonutil::registry::RegistryCredentials` owns saved credential loading and
  persistence, legacy compatibility, and API destination matching. Login and
  API consumers share these operations; HTTP headers and redirect policy stay
  in the request layer. `registry_api_url` validates and normalizes their API
  base URLs without selecting the configured registry.
- `moonutil::cache` owns cache-root selection, ownership validation, and
  lifecycle rules.
- `moonutil::toolchain` owns installed toolchain paths and must not be used to
  locate mutable Moon home state.
