name = "test/sync_once"

version = "0.1.0"

source = "src"

options(
  "bin-deps": { "test/sync_tool": { "path": "../tool", "bin_pkg": [ "main" ] } },
)