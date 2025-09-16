#!/usr/bin/env python3
import sys
import re


def normalize_path(p):
    # Treat "@/path" as "./path"
    if p.startswith("@/"):
        return p[2:]
    return p


def parse_failing_tests(input_path):
    """
    Build a set of failing test names from argv[1].
    We only accept lines that:
      - start with exactly two spaces
      - contain '::' (to look like a fully qualified test name)
    """
    failing = set()
    try:
        with open(input_path, "r", encoding="utf-8") as f:
            for raw in f:
                if raw.startswith("  ") and not raw.startswith("   "):
                    name = raw.rstrip("\n")[2:].strip()
                    # ignore markers if someone appended them
                    if name.endswith(" [fixed]"):
                        name = name[: -len(" [fixed]")].rstrip()
                    if "::" in name and name:
                        failing.add(name)
    except FileNotFoundError:
        sys.exit(f"input file not found: {input_path}")
    return failing


def update_todo_file(todo_path, failing):
    """
    Update argv[2] todo file in-place:
      - For each line that contains a test name (detected by ' --' and '::' before it),
        append ' [fixed]' to the test name if it's NOT in the failing set.
      - Remove trailing ' [fixed]' from the test name if it IS in the failing set.
    Layout/whitespace is preserved as much as possible:
      - We only modify the left side (before the first ' --') and keep any bullets/indentation.
    """
    try:
        with open(todo_path, "r", encoding="utf-8") as f:
            lines = f.readlines()
    except FileNotFoundError:
        sys.exit(f"todo file not found: {todo_path}")

    fixed_tag = " [fixed]"
    changed = False
    out = []

    for line in lines:
        nl = "\n" if line.endswith("\n") else ""
        body = line.rstrip("\n")

        idx = body.find(" --")
        if idx == -1:
            # not a test entry (as defined), keep as-is
            out.append(line)
            continue

        left_full = body[:idx]
        right_full = body[idx:]  # keep original spacing and reason/comment

        # Preserve bullets/indentation on the left
        m = re.match(r"^(\s*[-*•]?\s*)(.*)$", left_full)
        if not m:
            out.append(line)
            continue

        prefix = m.group(1)
        name_with_maybe_fixed = m.group(2).strip()

        # If empty after prefix, leave as-is
        if not name_with_maybe_fixed:
            out.append(line)
            continue

        # Strip one trailing " [fixed]" if present to get the canonical test name
        has_fixed = False
        if name_with_maybe_fixed.endswith(fixed_tag):
            has_fixed = True
            name_core = name_with_maybe_fixed[: -len(fixed_tag)].rstrip()
        else:
            name_core = name_with_maybe_fixed

        # Only operate on plausible test names containing '::'
        if "::" not in name_core:
            out.append(line)
            continue

        # Decide desired fixed flag based on presence in failing set
        is_failing = name_core in failing
        want_fixed = not is_failing

        # Reconstruct if we need to change
        new_left = prefix + name_core + (fixed_tag if want_fixed else "")
        if new_left != left_full:
            changed = True
            out.append(new_left + right_full + nl)
        else:
            out.append(line)

    if changed:
        with open(todo_path, "w", encoding="utf-8") as f:
            f.writelines(out)

    return changed


def main():
    if len(sys.argv) < 3:
        sys.stderr.write("Usage: mark_fixed.py <input_fail_list> <todo_file>\n")
        sys.exit(2)

    input_path = normalize_path(sys.argv[1])
    todo_path = normalize_path(sys.argv[2])

    failing = parse_failing_tests(input_path)
    changed = update_todo_file(todo_path, failing)
    sys.stdout.write("updated\n" if changed else "no changes\n")


if __name__ == "__main__":
    main()