import json
import os
import sys
from pathlib import Path

build_input = json.load(sys.stdin)
assert set(build_input) == {"env", "paths"}
assert build_input["env"]["MOON_CC"] == os.environ["MOON_CC"]
assert build_input["env"]["MOON_MOD"] == "inherited-module-manifest"
assert build_input["env"]["MOON_BUILD_DIR"] == "inherited-build-dir"
assert build_input["env"]["MOON_BACKEND"] == "inherited-backend"
assert os.environ["MOON_BACKEND"] == "native"
assert os.environ["MOON_PROFILE"] == "debug"
assert os.environ["MOON_JOBS"] == "3"
assert build_input["paths"]["module_root"] == os.getcwd()
assert Path(os.environ["MOON_MOD"]) == Path.cwd() / "moon.mod.json"
assert build_input["paths"]["out_dir"] == os.environ["MOON_BUILD_DIR"]
build_dir = Path(os.environ["MOON_BUILD_DIR"])
assert build_dir.is_absolute() and build_dir.is_dir()
(build_dir / "generated.txt").write_text("ready")

output = {
    "vars": {"HELLO": "------this-is-added-by-config-script------"},
    "link_configs": [
        {
            "package": "username/hello/dep",
            "link_flags": "-l______this_is_added_by_config_script_______",
            "link_libs": ["mylib"],
            "link_search_paths": ["/my-search-path"],
        }
    ],
}
print(json.dumps(output))
