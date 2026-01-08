# Add Test Case

Add a new test case to the moonbuild test suite.

## Directory Structure

Test cases are located in `crates/moon/tests/test_cases/`. Each test case follows this structure:

```
crates/moon/tests/test_cases/<test_name>/
├── mod.rs                           # Test module with Rust test functions
└── <test_name>.in/                  # Input files for the test
    ├── moon.mod.json                # Module definition
    ├── moon.pkg.json                # Package definition (often empty {})
    ├── src.mbt                      # Source file(s)
    └── <subpkg>/                    # Optional subpackages
        ├── moon.pkg.json
        └── src.mbt
```

## Step-by-Step Instructions

### 1. Create the input directory

```bash
mkdir -p crates/moon/tests/test_cases/<test_name>/<test_name>.in
```

### 2. Create moon.mod.json

```json
{
  "name": "test_name"
}
```

### 3. Create moon.pkg.json

For a simple package:
```json
{}
```

For a main package (executable):
```json
{
  "is-main": true
}
```

### 4. Create source files (.mbt)

For test files, use the `///|` marker:
```moonbit
///|
test "test_name" {
  assert_eq(1, 1)
}
```

For main files:
```moonbit
///|
fn main {
  println(42)
}
```

### 5. Create the test module (mod.rs)

Create `crates/moon/tests/test_cases/<test_name>/mod.rs`:

```rust
use super::*;

#[test]
fn test_<test_name>() {
    let dir = TestDir::new("<test_name>/<test_name>.in");

    // Run a moon command and capture stdout
    let stdout = get_stdout(&dir, ["build", "--target", "js"]);

    // Verify output using expect-test
    check(
        &stdout,
        expect![[r#"
            expected output here
        "#]],
    );
}
```

### 6. Register the module

Add the module to `crates/moon/tests/test_cases/mod.rs`:

```rust
mod <test_name>;
```

Keep modules in alphabetical order.

## Test Utilities Reference

### TestDir::new(path)
Creates a temporary test directory from the input files.

### get_stdout(&dir, args)
Runs `moon` with the given arguments and returns stdout.
```rust
let stdout = get_stdout(&dir, ["test", "--target", "js", "--build-only"]);
```

### get_stderr(&dir, args)
Runs `moon` with the given arguments and returns stderr.

### check(&output, expect![[...]])
Verifies output matches expected value using expect-test.
The `$ROOT` placeholder is automatically replaced with the actual path.

### snapbox_cmd(&dir, args)
Creates a command for more complex assertions.

## Example: Adding a Build Test

```rust
use super::*;

#[test]
fn test_my_feature() {
    let dir = TestDir::new("my_feature/my_feature.in");

    // Test build succeeds
    let stdout = get_stdout(&dir, ["build"]);
    check(&stdout, expect![[r#""#]]);

    // Test run output
    let stdout = get_stdout(&dir, ["run", "main"]);
    check(
        &stdout,
        expect![[r#"
            42
        "#]],
    );
}
```

## Running the Test

```bash
cargo test -p moon --test test test_<test_name>
```

Or to update expect values:
```bash
UPDATE_EXPECT=1 cargo test -p moon --test test test_<test_name>
```
