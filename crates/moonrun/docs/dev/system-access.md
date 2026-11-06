# Built-in system access

These system operations are available even when a Run has empty filesystem and
network rules. Keep this inventory separate from configurable policy. A guest
still needs any WASI descriptor rights required by the operation.

| System access | Scope |
| --- | --- |
| Unix timezone files | Read-only open, realpath, existence/readability checks, and metadata for regular files in the installed `/usr/share/zoneinfo` tree, including its macOS system aliases, and for a copied `/etc/localtime`. A symlink at `/etc/localtime` must resolve inside the installed tree to qualify. The requested absolute path, each expanded symlink path, and the resolved target must all match. Writes, creation, directory listing, watching, and entry changes still use configured policy. |
| System certificate verification | A host trust provider may read its trust store or contact certificate issuers or revocation services as part of verification. This is provider activity, outside guest socket authorization. It does not grant a guest connection to those hosts. |

The filesystem grant is implemented in `filesystem/authorization/system_reads.rs`.
It assumes the installed timezone tree is administered by the host. Relative
paths, aliases in guest-controlled directories, and paths escaping the tree use
normal filesystem authorization. Additional timezone locations supplied by
environment variables are not built-in grants.

The certificate entry documents the boundary, rather than forcing network
fetches or changing verification settings. Windows chain construction can use
platform [retrieval and cache settings](https://learn.microsoft.com/en-us/windows/win32/api/wincrypt/nf-wincrypt-certgetcertificatechain). Apple's Security framework can also
[allow network fetching](https://developer.apple.com/documentation/security/sectrustsetnetworkfetchallowed%28_%3A_%3A%29) where that provider is used; Moonrun's current Unix TLS
backend uses OpenSSL. Guest-specified certificate and private-key files still
go through HostFs authorization.
