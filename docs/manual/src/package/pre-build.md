# Pre-build

The `"pre-build"` field is used to specify pre-build commands, which will be executed before build commands such as `moon check|build|test`.

`"pre-build"` is an array, where each element is an object containing `input`, `output`, and `command` fields. The `input` and `output` fields can be strings or arrays of strings, while the `command` field is a string. In the `command`, you can use any shell commands, as well as the `$input` and `$output` variables, which represent the input and output files, respectively. If these fields are arrays, they will be joined with spaces by default.

Currently, there is a built-in special command `:embed`, which converts files into MoonBit source code. The `--text` parameter is used to embed text files, and `--binary` is used for binary files. `--text` is the default and can be omitted. The `--name` parameter is used to specify the generated variable name, with `resource` being the default. The command is executed in the directory where the `moon.pkg.json` file is located.

```json
{
  "pre-build": [
    {
      "input": "a.txt",
      "output": "a.mbt",
      "command": ":embed -i $input -o $output"
    }
  ]
}
```

If the content of `a.txt` in the current package directory is:
```
hello,
world
```

After running `moon build`, the following `a.mbt` file will be generated in the directory where the `moon.pkg.json` is located:

```
let resource : String =
  #|hello,
  #|world
  #|
```