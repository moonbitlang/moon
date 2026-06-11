# Installing GitHub permalinks: Go, Cargo, and hosted archives

Researched on 2026-09-18 against official documentation and first-party source.
This is non-normative design research for PR #1502. The
[binary-install reference](../reference/moon-install-binary.md) defines the
implemented behavior.

## Scope established in discussion

- Change only the new pasted-permalink input; retain existing installation forms.
- Support GitHub only for this new input.
- Require a full 40-character commit SHA to separate revision from directory
  without guessing where a slash-containing branch name ends.
- Public repositories are the MVP.
- Correctness includes requesting the chosen revision and faithfully handling
  its distributed archive, including the author's export attributes.

## Chosen design

Use the GitHub archive URL for the new permalink form. The export attributes
`export-ignore` and `export-subst` express the repository author's distribution
intent. A correct installer preserves that intent rather than treating every
difference from a checkout as a defect.

Download the exact commit's source archive, then apply Moon's existing
filesystem package selection and native installation behavior. Do not resolve
branches or fall back to cloning. A missing selected directory is an archive
installation error; users do not have to manually split the URL or override
export attributes.

Public GitHub repositories and full-SHA permalinks define the supported MVP.
Existing generic Git installations keep their transport and authentication.
Private HTTP credentials and additional hosts remain separate work.

This is a deliberate Moon feature, not an established Go/Cargo permalink
standard. Their relevant precedents are explicit revision selection and clear
source-format contracts. GitHub supports ZIP and tar.gz; the implementation
uses tar.gz with a maintained extractor for streaming extraction, executable
modes, and symlinks. The existing ZIP extractor materializes symlinks as files,
so it does not meet the chosen source contract without additional machinery.

## Comparison

| Tool | User input | Source acquisition | Relevance |
| --- | --- | --- | --- |
| Go | Package path plus version/revision query | Module-proxy ZIPs; direct VCS fallback | Separate package identity, revision policy, and archive format |
| Cargo | Repository URL, revision flag, crate selection | Cached Git database and revision checkout | Acquire the requested revision explicitly |
| degit | Hosted repository/subdirectory inputs, including GitHub tree URLs | Hosted tar archives by default | Closer precedent for pasted browser URLs |

Evidence and qualifications follow below.

## Go

`go install github.com/owner/repo/cmd/tool@VERSION` uses a package path and
requires main packages. The documented input has no `/tree/SHA/path`
translation. This conclusion follows from the package-path contract, rather
than a tested rejection of every browser URL spelling.
[Go command documentation](https://pkg.go.dev/cmd/go#hdr-Compile_and_install_packages_and_dependencies)

Revision queries accept hashes and branches, translating them to tagged
versions or pseudo-versions. A pseudo-version's commit must be reachable from
a repository branch or tag: PR-only commits are intentionally excluded.
Public modules normally use `GOPROXY`; its default proxy/direct sequence falls
back after HTTP 404/410. Private-module configuration normally bypasses the
public proxy and checksum database. Module verification hashes file names and
contents rather than compressed ZIP bytes.
[Pseudo-versions](https://go.dev/ref/mod#pseudo-versions),
[version queries](https://go.dev/ref/mod#version-queries),
[private modules](https://go.dev/ref/mod#private-modules),
[authentication](https://go.dev/ref/mod#authenticating)

The inspected Git implementation initializes a cached bare repository,
fetches allowed refs, and produces an archive locally. It explicitly excludes
arbitrary PR/CL hashes from the module-version model and disables
`export-ignore` and `export-subst` during archiving. Go does not establish a
precedent for replacing direct Git access with GitHub ZIPs.
[Go Git implementation](https://go.dev/src/cmd/go/internal/modfetch/codehost/git.go)

Go module ZIPs have a specified module/version prefix, path and size
constraints, omit symlinks, and exclude nested modules and most vendor files.
They are not whole-repository snapshots.
[Module ZIP specification](https://pkg.go.dev/golang.org/x/mod/zip)

For private HTTPS access, `GOAUTH` supports `.netrc`, Git credential helpers,
and custom header-producing commands. HTTP authentication can be designed
separately from source selection; Moon need not implement it in this MVP.
[GOAUTH documentation](https://pkg.go.dev/cmd/go#hdr-GOAUTH_environment_variable)

## Cargo

Cargo documents registry, local `--path`, and `--git URL` installation. Git
revision flags are separate from the URL; a positional crate name selects
among packages, while `--bin` selects executables within a package. Its
install interface does not define browser-permalink parsing.
[Cargo install manual](https://doc.rust-lang.org/cargo/commands/cargo-install.html)

Cargo's Git dependency model uses repository-root URLs and discovers nested
crates by manifests and names. Its revision syntax includes explicit refs
such as `refs/pull/493/head`; submodules are recursively fetched.
[Git dependencies](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#specifying-dependencies-from-git-repositories)

Cargo stores bare Git databases and per-revision checkouts separately from
registry `.crate` archives. Registry downloads are therefore not evidence
that `cargo install --git` downloads GitHub source ZIPs.
[Cargo cache layout](https://doc.rust-lang.org/cargo/guide/cargo-home.html)

The inspected implementation explicitly fetches requested `refs/*`. Its
GitHub-specific full-SHA path requests `+SHA:refs/commit/SHA` without needing
GitHub API resolution. It does not assume fetching branches obtained every
requested commit. This is implementation evidence, conditional on the remote
serving the object, not an indefinite-availability guarantee.
[Cargo fetch implementation](https://doc.rust-lang.org/stable/nightly-rustc/src/cargo/sources/git/utils.rs.html#1039-1109),
[GitHub full-SHA handling](https://doc.rust-lang.org/stable/nightly-rustc/src/cargo/sources/git/utils.rs.html#1514-1563)

Private Git sources use HTTPS credential helpers or SSH agents. Cargo can
delegate fetching to Git for authentication outside its built-in support.
Those credentials do not automatically authenticate HTTP archive downloads.
[Cargo Git authentication](https://doc.rust-lang.org/cargo/appendix/git-authentication.html)

## A closer precedent: degit

degit documents pasted GitHub tree URLs with subdirectories and downloads tar
snapshots by default. It currently falls back to SSH cloning when archive
retrieval or extraction fails. This supplies input-UX precedent, but does not
establish that archives preserve all committed source files.
[degit usage](https://github.com/Rich-Harris/degit/blob/master/docs/USAGE.md#clone-a-subdirectory),
[degit overview](https://github.com/Rich-Harris/degit#readme)

## GitHub's archive contract

GitHub documents `/owner/repo/archive/FULL_SHA.zip` and `.tar.gz`. These are
repository snapshots without Git history. Commit pinning fixes the selected
revision; compressed bytes may change and repository renames change the
wrapper directory. Extraction should discover that wrapper rather than assume
its name.
[GitHub source archives](https://docs.github.com/en/repositories/working-with-files/using-files/downloading-source-code-archives)

The REST archive endpoints redirect to downloads. Public access needs no
token; private access requires suitable credentials, including Contents read
permission for fine-grained tokens. Private download links expire. Public-only
is therefore an implementation scope choice, not an archive limitation.
[GitHub archive API](https://docs.github.com/en/rest/repos/contents#download-a-repository-archive-zip)

The documented archive URL avoids a separate metadata lookup and its use of
the REST API's unauthenticated 60-requests-per-hour budget. It does not imply
unlimited archive-download capacity or resolve the content differences below.
[REST rate limits](https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api#primary-rate-limit-for-unauthenticated-users)

### Archive contents are a product contract

GitHub identifies `git archive` as the source of its snapshots. Git's
`export-ignore` attribute omits paths and `export-subst` expands placeholders.
A directory visible in the browser can consequently be absent from the
archive; an archived file can differ from its committed blob. Go overrides
these attributes. The documented GitHub archive endpoint offers no equivalent
consumer-controlled override.
[GitHub generation](https://docs.github.com/en/repositories/working-with-files/using-files/downloading-source-code-archives),
[Git archive attributes](https://git-scm.com/docs/git-archive#_attributes),
[Go archiving](https://go.dev/src/cmd/go/internal/modfetch/codehost/git.go)

GitHub administrators can choose whether archives contain Git LFS objects or
pointer files. A pinned SHA is therefore not a guarantee of a completely
reproducible build environment. LFS/submodule hydration and Git history
requirements need their own defined behavior; a source permalink alone does
not guarantee that every project builds in every environment.
[GitHub LFS archive settings](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/managing-repository-settings/managing-git-lfs-objects-in-archives-of-your-repository)

### PR merge commit spot check

On the research date, GitHub's API reported PR #1502 open with head
`9f96bd265eb015b64c3029d4252798ee04f70e3d` and test merge commit
`45f8c92ef7c945d7fac7f3a589d226a0a871c2eb`. An unauthenticated HTTP HEAD request
to that merge commit's ZIP URL followed a redirect and returned HTTP 200 at
`codeload.github.com/moonbitlang/moon/zip/45f8c92ef7c945d7fac7f3a589d226a0a871c2eb`.
[PR API](https://api.github.com/repos/moonbitlang/moon/pulls/1502),
[tested archive URL](https://github.com/moonbitlang/moon/archive/45f8c92ef7c945d7fac7f3a589d226a0a871c2eb.zip)

This checks availability for one synthetic merge commit. It does not validate
downloaded contents, prove absence from every branch's history, or promise
retention of obsolete merge commits. Support must remain conditional on
GitHub serving the selected SHA.

### Local correctness probe

A temporary Git repository reproduced both archive differences: a tracked
`cmd/tool/main.mbt` marked `export-ignore` was absent from its ZIP, while
`version.txt` marked `export-subst` had its commit placeholder replaced.
The selected commit was reachable only through `refs/pull/1/merge` after
returning the default branch to its earlier commit.

Initializing another repository, fetching that exact SHA with `--depth=1`
over the `file://` Git transport, and checking out the SHA succeeded. Both
the selected source file and literal version placeholder were preserved.
This is a local mechanism check, not a GitHub availability guarantee or an
end-to-end Moon installation test.

## Consequences for PR #1502

1. Retain existing installation forms and authentication behavior.
2. Make the new parser GitHub-specific, with full-SHA selection and one
   decoding pass for the directory path.
3. Download the selected archive without a clone/checkout fallback.
4. Preserve the author's archive exclusions and substitutions. Clearly report
   selected directories absent from the archive rather than bypassing them.
5. Preserve Moon's package selection, wildcard behavior, nested-module
   selection, and repository path containment after extraction.
6. Accept explicitly pasted PR-only SHAs that GitHub serves. This differs
   intentionally from Go's module-version ancestry policy: the user selected
   a snapshot rather than a published module version.
7. Defer private HTTP credentials and broader hosted-URL forms.

Validation should cover exact archive URLs, encoded paths, redirects, nested
modules, archive-only source acquisition, error behavior without a Git
fallback, executable modes, symlinks, and extraction containment.
