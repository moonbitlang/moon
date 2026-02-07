use super::*;

#[cfg(unix)]
#[test]
fn test_native_abort_trace() {
    let dir = TestDir::new("native_abort_trace/native_abort_trace.in");

    fn strip_ansi(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut chars = input.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                if chars.peek() == Some(&'[') {
                    chars.next();
                    for c2 in chars.by_ref() {
                        if c2 == 'm' {
                            break;
                        }
                    }
                }
                continue;
            }
            out.push(c);
        }
        out
    }

    fn normalize_line_numbers(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        for line in input.lines() {
            let Some(pos) = line.find("main.c:") else {
                out.push_str(line);
                out.push('\n');
                continue;
            };
            let prefix = &line[..pos];
            let rest = &line[pos + "main.c:".len()..];
            let mut digits_len = 0;
            for ch in rest.chars() {
                if ch.is_ascii_digit() {
                    digits_len += ch.len_utf8();
                } else {
                    break;
                }
            }
            if digits_len == 0 {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            let suffix = &rest[digits_len..];
            out.push_str(prefix);
            out.push_str("main.c:<line>");
            out.push_str(suffix);
            out.push('\n');
        }
        out
    }

    let output = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["run", "--target", "native", "cmd/main"])
        .assert()
        .success()
        .get_output()
        .to_owned();

    let mut out = String::from_utf8_lossy(&output.stdout).to_string();
    out.push_str(&String::from_utf8_lossy(&output.stderr));
    let out = replace_dir(&normalize_line_numbers(&strip_ansi(&out)), &dir);

    check(
        &out,
        expect![[r#"
            Hello
            RUNTIME ERROR: abort() called
            $MOON_HOME/lib/core/builtin/option.mbt:38 at @moonbitlang/core/option.Option::unwrap[Int]
            $ROOT/cmd/main/main.mbt:14 by @username/scratch/cmd/main.g
            $ROOT/cmd/main/main.mbt:9 by @username/scratch/cmd/main.f
            $ROOT/cmd/main/main.mbt:4 by main
        "#]],
    );
}
