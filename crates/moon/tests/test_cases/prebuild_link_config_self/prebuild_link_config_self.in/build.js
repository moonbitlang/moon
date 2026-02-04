const output = {
  link_configs: [
    {
      package: 'prebuild_link_config_self/main',
      link_flags: '-l__prebuild_self_link_flag__',
      link_libs: ['prebuildselflib'],
      link_search_paths: ['/prebuild-self-path'],
    },
  ],
}
console.log(JSON.stringify(output))
