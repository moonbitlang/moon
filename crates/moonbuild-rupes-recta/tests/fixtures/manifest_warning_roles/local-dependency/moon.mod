name = "test/dep"
options(
  "dependency-future": true,
  include: ["src/**"],
  exclude: ["test/**"],
  scripts: { "prebuild": "unused-command" },
  "--moonbit-unstable-prebuild": "build.js",
)
