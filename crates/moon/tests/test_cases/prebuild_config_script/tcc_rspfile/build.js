const output = {
  link_configs: [
    {
      package: 'username/hello/dep',
      link_libs: ['added_by_config_script'],
      link_search_paths: ['/added-by-config-script'],
    },
  ],
}
console.log(JSON.stringify(output))
