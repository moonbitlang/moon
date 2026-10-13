name = "username/check_skip_bin_dep_user"

source = "src"

options(
  "bin-deps": {
    "username/prebuild_bin_dep": {
      "path": "../author.in",
      "bin_pkg": [ "main" ],
    },
  },
)