name = "testuser/runner"

version = "1.2.3"

source = "src"

import {
  "testuser/dependency@1.0.0",
}

options(
  scripts: { "postadd": "moonx-postadd-must-not-run" },
)