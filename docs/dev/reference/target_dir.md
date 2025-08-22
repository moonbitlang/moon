# Target directory layout

## Legacy layout

> This is the layout of the current, legacy implementation.

The default target directory root is `<project_root>/target/<backend>`,
which we will refer as `<target>` below.

The root directory is `<target>/<mode>/<operation>`,
such as `<target>/release/build` or `<target>/debug/bundle`.

Default modes:

|        | Debug | Release |
| ------ | ----- | ------- |
| Check  |       | x       |
| Build  |       | x       |
| Bundle |       | x       |
| Test   | x     |         |
| Bench  | x     |         |
