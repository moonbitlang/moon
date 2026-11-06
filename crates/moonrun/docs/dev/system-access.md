# Built-in system access

Moonrun permits reads of trusted Unix timezone data with or without a policy.
An explicit filesystem exclusion or an inherited policy cannot revoke this
built-in read grant. WASI still requires a suitable preopen and descriptor rights.

| System access | Scope |
| --- | --- |
| Unix timezone files | Read-only open, realpath, existence/readability checks, and metadata for regular files in the installed `/usr/share/zoneinfo` tree, including its macOS system aliases, and for a copied `/etc/localtime`. A symlink at `/etc/localtime` must resolve inside the installed tree to qualify. The requested absolute path, each expanded symlink path, and the resolved target must all match. Writes, creation, directory listing, watching, and entry changes still use configured policy. |
| System certificate verification | A host trust provider may read its trust store or contact certificate issuers or revocation services as part of verification. This is provider activity, outside guest socket authorization. It does not grant a guest connection to those hosts. |

The filesystem grant is implemented in `filesystem/authorization/system_reads.rs`.
It assumes the installed timezone tree is administered by the host. For a
read relative to an open directory or a fixed runtime working directory, the
check uses that directory to identify the requested path. Relative names in
a mutable process working directory, aliases in guest-controlled directories,
and paths escaping the tree use normal filesystem authorization. Additional
timezone locations supplied by environment variables also use normal authorization.

The certificate entry documents the boundary, rather than forcing network
fetches or changing verification settings. Windows chain construction can use
platform [retrieval and cache settings](https://learn.microsoft.com/en-us/windows/win32/api/wincrypt/nf-wincrypt-certgetcertificatechain). Apple's Security framework can also
[allow network fetching](https://developer.apple.com/documentation/security/sectrustsetnetworkfetchallowed%28_%3A_%3A%29) where that provider is used; Moonrun's current Unix TLS
backend uses OpenSSL. Guest-specified certificate and private-key files still
go through HostFs authorization.
