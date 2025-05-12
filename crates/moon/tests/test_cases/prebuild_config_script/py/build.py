import json

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
