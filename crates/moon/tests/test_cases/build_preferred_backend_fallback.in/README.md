# Build selection fallback

Both modules prefer a backend their package does not support. Without explicit
selectors, their preferred-backend groups become empty and their conflicting
preferences cause the default Wasm fallback to reselect both packages. Explicit
path selectors keep an empty selection unless a supported backend is requested.

These declaration-only packages exercise command selection and planning. They
are not compiler/runtime fixtures.
