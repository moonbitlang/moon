# Own Resources Behind Boundary Handles

Moonrun will keep MoonBit ABI values as opaque Resource Handles, resolve them at the async import boundary into Rust-owned Resources, and let Jobs carry `Arc<Resource>` rather than guest `u64` handles or duplicated OS fd/HANDLE values. This preserves Native Behavior for valid MoonBit async programs while protecting moonrun from Untrusted Guests: closing a Resource Handle removes future guest reachability immediately, and any already-submitted Job may finish through the Resource it already acquired.

The guiding rule is strict native behavior on the normal path, with defensive checks only at the boundary. Valid MoonBit async code should observe the same behavior as native execution; unexpected guest calls may be rejected before they can turn opaque ABI handles into stale, duplicated, or lifetime-less host access.
