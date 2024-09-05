# MoonBit 构建系统教程

`moon` 是 MoonBit 语言的构建系统，目前基于 [n2](https://github.com/evmar/n2) 项目。`moon` 支持并行构建和增量构建，此外它还支持管理和构建 [mooncakes.io](https://mooncakes.io/) 上的第三方包。

## 准备工作

在开始之前，请确保安装好以下内容：

1. **MoonBit CLI 工具**: 从[这里](https://www.moonbitlang.cn/download/)下载。该命令行工具用于创建和管理 MoonBit 项目。

    使用 `moon help` 命令可查看使用说明。

    ```bash
    $ moon help
    The build system and package manager for MoonBit.

    Usage: moon [OPTIONS] <COMMAND>

    Commands:
    new                    Create a new MoonBit module
    build                  Build the current package
    check                  Check the current package, but don't build object files
    run                    Run a main package
    test                   Test the current package
    clean                  Remove the target directory
    fmt                    Format source code
    doc                    Generate documentation
    info                   Generate public interface (`.mbti`) files for all packages in the module
    add                    Add a dependency
    remove                 Remove a dependency
    install                Install dependencies
    tree                   Display the dependency tree
    login                  Log in to your account
    register               Register an account at mooncakes.io
    publish                Publish the current package
    update                 Update the package registry index
    coverage               Code coverage utilities
    generate-build-matrix  Generate build matrix for benchmarking (legacy feature)
    upgrade                Upgrade toolchains
    shell-completion       Generate shell completion for bash/elvish/fish/pwsh/zsh to stdout
    version                Print version info and exit
    help                   Print this message or the help of the given subcommand(s)

    Options:
    -C, --directory <SOURCE_DIR>   The source code directory. Defaults to the current directory
        --target-dir <TARGET_DIR>  The target directory. Defaults to `source_dir/target`
    -q, --quiet                    Suppress output
    -v, --verbose                  Increase verbosity
        --trace                    Trace the execution of the program
        --dry-run                  Do not actually run the command
    -h, --help                     Print help
    ```

2. **Moonbit Language** Visual Studio Code 插件: 可以从 VS Code 市场安装。该插件为 MoonBit 提供了丰富的开发环境，包括语法高亮、代码补全、交互式除错和测试等功能。

安装完成后，让我们开始创建一个新的 MoonBit 模块。

## 创建一个新模块

使用 `moon new` 创建一个新项目，默认的配置是：

```bash
$ moon new
Enter the path to create the project (. for current directory): my-project
Select the create mode: exec
Enter your username: username
Enter your project name: hello
Enter your license: Apache-2.0
Created my-project
```

这会在 `my-project` 下创建一个名为 `username/hello` 的新模块。上述过程也可以使用 `moon new my-project` 代替。

## 了解模块目录结构

上一步所创建的模块目录结构如下所示：

```bash
my-project
├── LICENSE
├── README.md
├── moon.mod.json
└── src
    ├── lib
    │   ├── hello.mbt
    │   ├── hello_test.mbt
    │   └── moon.pkg.json
    └── main
        ├── main.mbt
        └── moon.pkg.json
```

这里简单解释一下目录结构：

- `moon.mod.json` 用来标记这个目录是一个模块。它包含了模块的元信息，例如模块名、版本等。
  ```json
  {
    "name": "username/hello",
    "version": "0.1.0",
    "readme": "README.md",
    "repository": "",
    "license": "Apache-2.0",
    "keywords": [],
    "description": "",
    "source": "src"
  }
  ```

  `source` 字段指定了模块的源代码目录，默认值是 `src`。该字段存在的原因是因为 MoonBit 模块中的包名与文件路径相关。例如，当前模块名为 `username/hello`，而其中一个包所在目录为 `lib/moon.pkg.json`，那么在导入该包时，需要写包的全名为 `username/hello/lib`。有时，为了更好地组织项目结构，我们想把源码放在 `src` 目录下，例如 `src/lib/moon.pkg.json`，这时需要使用 `username/hello/src/lib` 导入该包。但一般来说我们并不希望 `src` 出现在包的路径中，此时可以通过指定 `"source": "src"` 来忽略 `src` 这层目录，便可使用 `username/hello/lib` 导入该包。

- `src/lib` 和 `src/main` 目录：这是模块内的包。每个包可以包含多个 `.mbt` 文件，这些文件是 MoonBit 语言的源代码文件。但是，无论一个包有多少 `.mbt` 文件，它们都共享一个 `moon.pkg.json` 文件。`lib/*_test.mbt` 是 `lib` 包中的独立测试文件，这些文件用于黑盒测试，无法直接访问 `lib` 包的私有成员。

- `moon.pkg.json` 是包描述文件。它定义了包的属性，例如，是否是 `main` 包，所导入的其他包。

  - `main/moon.pkg.json`:

    ```json
    {
      "is_main": true,
      "import": [
        "username/hello/lib"
      ]
    }
    ```

  这里，`"is_main: true"` 表示这个包需要被链接成目标文件。在 `wasm/wasm-gc` 后端，会被链接成一个 `wasm` 文件，在 `js` 后端，会被链接成一个 `js` 文件。

  - `lib/moon.pkg.json`:

    ```json
    {}
    ```

  这个文件只是为了告诉构建系统当前目录是一个包。

## 如何使用包

我们的 `username/hello` 模块包含两个包：`username/hello/lib` 和 `username/hello/main`。

包 `username/hello/lib` 包含 `hello.mbt` 和 `hello_test.mbt` 文件：

  `hello.mbt`

  ```moonbit
  pub fn hello() -> String {
      "Hello, world!"
  }
  ```

  `hello_test.mbt`

  ```moonbit
  test "hello" {
    if @lib.hello() != "Hello, world!" {
      fail!("@lib.hello() != \"Hello, world!\"")
    }
  }
  ```

包 `username/hello/main` 只包含一个 `main.mbt` 文件：

  ```moonbit
  fn main {
    println(@lib.hello())
  }
  ```

为了执行程序，需要在 `moon run` 命令中指定 `username/hello/main` 包的**文件系统路径**：

```bash
$ moon run ./src/main
Hello, world!
```

你也可以省略 `./`

```bash
$ moon run src/main
Hello, world!
```

你可以使用 `moon test` 命令进行测试：

```bash
$ moon test
Total tests: 1, passed: 1, failed: 0.
```

## 如何导入包

在 MoonBit 的构建系统中，模块名用于引用其内部包。

如果想在 `src/main/main.mbt` 中使用 `username/hello/lib` 包，你需要在 `src/main/moon.pkg.json` 中指定：

```json
{
  "is_main": true,
  "import": [
    "username/hello/lib"
  ]
}
```

这里，`username/hello/lib` 指定了从 `username/hello` 模块导入 `username/hello/lib` 包，所以你可以在 `main/main.mbt` 中使用 `@lib.hello()`。

注意，`src/main/moon.pkg.json` 中导入的包名是 `username/hello/lib`，在 `src/main/main.mbt` 中使用 `@lib` 来引用这个包。这里的导入实际上为包名 `username/hello/lib` 生成了一个默认别名。在接下来的章节中，你将学习如何为包自定义别名。

## 创建和使用包

首先，在 `lib` 下创建一个名为 `fib` 的新目录：

```bash
mkdir src/lib/fib
```

现在，你可以在 `src/lib/fib` 下创建新文件：

`a.mbt`:

```moonbit
pub fn fib(n : Int) -> Int {
  match n {
    0 => 0
    1 => 1
    _ => fib(n - 1) + fib(n - 2)
  }
}
```

`b.mbt`:

```moonbit
pub fn fib2(num : Int) -> Int {
  fn aux(n, acc1, acc2) {
    match n {
      0 => acc1
      1 => acc2
      _ => aux(n - 1, acc2, acc1 + acc2)
    }
  }

  aux(num, 0, 1)
}
```

`moon.pkg.json`:

```json
{}
```

在创建完这些文件后，你的目录结构应该如下所示：

```bash
my-project
├── LICENSE
├── README.md
├── moon.mod.json
└── src
    ├── lib
    │   ├── fib
    │   │   ├── a.mbt
    │   │   ├── b.mbt
    │   │   └── moon.pkg.json
    │   ├── hello.mbt
    │   ├── hello_test.mbt
    │   └── moon.pkg.json
    └── main
        ├── main.mbt
        └── moon.pkg.json
```

在 `src/main/moon.pkg.json` 文件中，导入 `username/hello/lib/fib` 包，并自定义别名为 `my_awesome_fibonacci`：

```json
{
  "is_main": true,
  "import": [
    "username/hello/lib",
    {
      "path": "username/hello/lib/fib",
      "alias": "my_awesome_fibonacci"
    }
  ]
}
```

这行导入了 `username/hello/lib/fib` 包。导入后，你可以在 `main/main.mbt` 中使用 `fib` 包了。

将 `main/main.mbt` 的文件内容替换为：

```moonbit
fn main {
  let a = @my_awesome_fibonacci.fib(10)
  let b = @my_awesome_fibonacci.fib2(11)
  println("fib(10) = \{a}, fib(11) = \{b}")

  println(@lib.hello())
}
```

为了执行程序，需要在 `moon run` 命令中指定 `username/hello/main` 包的文件系统路径：

```bash
$ moon run ./src/main
fib(10) = 55, fib(11) = 89
Hello, world!
```

## 添加测试

让我们添加一些测试来验证我们的 fib 实现。在 `src/lib/fib/a.mbt` 中添加以下内容：

`src/lib/fib/a.mbt`

```moonbit
test {
  assert_eq!(fib(1), 1)
  assert_eq!(fib(2), 1)
  assert_eq!(fib(3), 2)
  assert_eq!(fib(4), 3)
  assert_eq!(fib(5), 5)
}
```

这些代码测试了斐波那契数列的前五项。`test { ... }` 定义了一个内联测试块。内联测试块中的代码在测试模式下执行。

内联测试块在非测试模式下（`moon build` 和 `moon run`）会被丢弃，因此不会导致生成的代码大小膨胀。

## 用于黑盒测试的独立测试文件

除了内联测试，MoonBit 还支持独立测试文件。以 `_test.mbt` 结尾的源文件被认为是黑盒测试的测试文件。例如，在 `src/lib/fib` 目录中，创建一个名为 `fib_test.mbt` 的文件，并粘贴以下代码：

`src/lib/fib/fib_test.mbt`

```moonbit
test {
  assert_eq!(@fib.fib(1), 1)
  assert_eq!(@fib.fib2(2), 1)
  assert_eq!(@fib.fib(3), 2)
  assert_eq!(@fib.fib2(4), 3)
  assert_eq!(@fib.fib(5), 5)
}
```

注意，构建系统会自动为以 `_test.mbt` 结尾的文件创建一个新的包，用于黑盒测试，并且自动导入当前包。因此，在测试块中需要使用 `@fib` 来引用 `username/hello/lib/fib` 包，而不需要在 `moon.pkg.json` 中显式地导入该包。

最后，使用 `moon test` 命令，它会扫描整个项目，识别并运行所有内联测试以及以 `_test.mbt` 结尾的文件。如果一切正常，你会看到：

```bash
$ moon test
Total tests: 3, passed: 3, failed: 0.
$ moon test -v
test username/hello/lib/hello_test.mbt::hello ok
test username/hello/lib/fib/a.mbt::0 ok
test username/hello/lib/fib/fib_test.mbt::0 ok
Total tests: 3, passed: 3, failed: 0.
```
