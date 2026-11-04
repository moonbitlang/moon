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
    registry/metadata/
      .moon-cache
      v1/<api-hash>/<module-hash>/
        releases/<version-hash>.json
        releases/<version-hash>.json.lock
        latest.json
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
- With `MOON_REGISTRY_API` unset, metadata uses the Git index for any module it
  contains, and HTTP for private modules absent from that index. A missing
  version of an indexed module never falls back to HTTP. Index read errors are
  preserved, and empty entries still belong to Git. HTTP responses and cached
  records for public modules absent from Git are rejected with a suggestion to
  update the index or select full HTTP mode.
  `MOON_REGISTRY_API=1` selects HTTP for every module, even when a Git entry
  exists. `MOON_REGISTRY_API=0` retains pure Git mode.
  HTTP queries use the configured API's matching saved login. All modes use
  the same registry configuration; there is no automatic mirror-to-origin or
  cross-registry fallback.
- On a cache miss, API resolution requests
  `GET /api/v0/registry/modules/<owner>/<module>?version=<exact>` for the declared
  version. Unversioned commands may omit the query to ask for
  latest. Neither operation lists version history or performs a range lookup.
  Exact version identity, dependencies and SHA-256 are validated before caching.
  A response for an already cached release must preserve its dependencies,
  checksum and visibility. Conflicting responses fail without replacing the
  cache; only deprecation status may change.
- API release records are keyed by normalized API URL, module and exact version
  beneath `cache/registry/metadata/v1`. The optional `latest.json` contains only a version
  pointer. Exact requests share release records directly; no generation or
  range-selection cache is needed. Every current latest query contacts the server,
  including repeated queries on the same client. Reading a persisted latest
  pointer cannot suppress a current query.
  Locally-known-latest executable selectors deliberately prefer a cached pointer.
- `MOON_REGISTRY_CACHE` selects an absolute HTTP metadata cache root, or `off`
  to disable persistence while retaining in-process reuse. A relative path is
  rejected. The default root has a `.moon-cache` ownership marker, as do custom
  roots. Initialization rejects unrecognized non-empty roots.
- `moon clean --registry-cache` removes the selected HTTP metadata cache.
  Git indexes, source archives, downloaded executables and credentials
  remain untouched. Ordinary `moon clean` leaves this cache alone. As with the
  other global caches, explicit cleanup is not coordinated with active users.
- In mixed mode, `moon update` synchronizes Git and symbols once, then resolves
  the selected module or entire workspace. Public dependency status and latest
  versions come from that synchronized index; private dependencies are refreshed
  through HTTP. Outside a project, only Git and symbols are synchronized.
  In full HTTP mode, `moon update` requires a project, refreshes advisory status
  for each selected exact registry version, and queries
  each module identity's current latest version. It reports available updates
  without changing manifests, downloading sources, or running hooks. Shared
  dependencies are refreshed once. Partial refresh failures preserve successful
  writes and return a nonzero status instead of reporting stale cached answers
  as current. A failure to resolve the graph stops the operation before refresh.
  Explicit Git mode retains only whole-index and symbols synchronization,
  including outside a project.
- Ordinary resolution keeps reusing cached exact records. Frozen module
  synchronization reads only cached metadata and verified sources; a missing
  exact release or source fails without HTTP or guessing another version.
- Metadata is atomically written through owner-only temporary files; Unix
  directories are owner-only. Each release's adjacent `.json.lock` serializes
  comparison and publication, so concurrent responses cannot overwrite conflicting
  immutable content. Cached deprecation state is advisory and can be
  stale: refreshing latest does not invalidate immutable release records.
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
  are not followed. Git index, symbols, public source archive and prebuilt artifact
  requests do not send these API credentials.
- Each `RegistryClient` loads its credentials at construction. Missing or empty
  credentials leave API reads anonymous. Malformed or unreadable credentials
  produce a User Log warning without including token text, and the client
  continues anonymously. Private metadata still requires a matching owner login,
  including on cache hits. The warning respects `--quiet` and is captured in
  machine-readable command results.
  Token validity and public/private access decisions belong to the registry;
  the client neither validates a token ahead of the request nor retries a
  rejected authenticated request anonymously.
- Private metadata also checks that the saved login has an explicit API binding
  and identifies the module owner, including on cache hits. Queries, cache access
  checks and delegated downloads use the client's captured credentials;
  replacing the credentials file affects newly created clients. These
  local checks do not detect remote token expiry or revocation while offline.
- Private archive misses delegate one download to the external `mooncake`
  executable. The request carries the exact version, checksum, staging path and
  captured login through stdin. Moon verifies the resulting archive before
  publishing it to the cache. Warm cache reads start no child process. Public
  and private modules share the existing archive and source-tree cache layouts;
  visibility and credentials do not change their paths. Reuse requires the
  checksum selected by metadata, with private access checked before reuse.
  Prebuilt private Wasm artifacts are not supported.

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

- `RegistryClient` selects available transports at construction and routes whole
  modules to one backend. Exact metadata, latest, checksums and source downloads
  share that routing rule. It also owns authenticated API commands and verified
  downloads. `Registry` exposes exact-version and latest queries without requiring
  a complete version history. `registry::git::GitIndex`
  owns Git synchronization, JSONL compatibility, and the parsed index cache;
  `registry::http::HttpMetadata` owns HTTP queries and response validation. Its
  internal `MetadataCache` owns disk layout, memory reuse, atomic writes and
  immutable-content checks. Memory entries use module/version keys within one
  API namespace; filesystem hashes are only computed for disk access.
  Cached latest reads and current latest queries are
  separate operations; there is no latest query deduplication or TTL. Only the
  Git backend enumerates all versions.
- `moonutil::registry::RegistryCredentials` owns saved credential loading and
  persistence, legacy compatibility, and API destination matching. Login and
  API consumers share these operations; HTTP headers and redirect policy stay
  in the request layer. `registry_api_url` validates and normalizes their API
  base URLs without selecting the configured registry.
- `RegistryClient` owns the captured credentials and shares them immutably with
  its HTTP metadata backend. `registry::authentication` constructs API clients
  from those credentials without loading files or holding global state. The
  external mooncake executable owns private source delivery; Moon owns archive
  verification and cache publication.
- `moonutil::cache` owns cache-root selection, ownership validation, and
  lifecycle rules.
- `moonutil::toolchain` owns installed toolchain paths and must not be used to
  locate mutable Moon home state.
