// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Terminal checkbox prompts shared by interactive commands.

use std::io;

use dialoguer::console::{Key, Term, style};

/// Runs a vertical checkbox prompt and returns the confirmed item indices.
///
/// The list is rendered one row per item with a `[ ]`/`[x]` checkbox. Arrow
/// keys (or `j`/`k`) move the highlight, Space toggles the highlighted row, `a`
/// toggles every row, and Enter confirms the current selection. Escape aborts
/// with [`io::ErrorKind::Interrupted`].
///
/// Every item starts unselected. Callers must only run this when the input and
/// rendering streams are terminals; this function does not perform that check.
pub fn checkbox_select(prompt: &str, items: &[String]) -> io::Result<Vec<usize>> {
    let term = Term::stderr();
    let mut checked = vec![false; items.len()];
    let mut cursor = 0usize;

    term.hide_cursor()?;
    let result = checkbox_select_loop(&term, prompt, items, &mut checked, &mut cursor);
    term.show_cursor()?;
    result
}

fn checkbox_select_loop(
    term: &Term,
    prompt: &str,
    items: &[String],
    checked: &mut [bool],
    cursor: &mut usize,
) -> io::Result<Vec<usize>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }

    let marker = style("?").for_stderr().cyan().bold();
    let title = style(prompt).for_stderr().bold();
    let hint = "(space: toggle, a: toggle all, enter: confirm)";
    term.write_line(&format!("{marker} {title} {hint}"))?;
    render_checkboxes(term, items, checked, *cursor)?;

    loop {
        let mut redraw = true;
        match term.read_key()? {
            Key::ArrowUp | Key::Char('k') => *cursor = cursor.saturating_sub(1),
            Key::ArrowDown | Key::Char('j') => *cursor = (*cursor + 1).min(items.len() - 1),
            Key::Char(' ') => checked[*cursor] = !checked[*cursor],
            Key::Char('a') | Key::Char('A') => {
                let select_all = !checked.iter().all(|selected| *selected);
                checked.fill(select_all);
            }
            Key::Enter => {
                let mut selected = Vec::new();
                for (index, is_checked) in checked.iter().enumerate() {
                    if *is_checked {
                        selected.push(index);
                    }
                }
                return Ok(selected);
            }
            Key::Escape => {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
            }
            _ => redraw = false,
        }

        if redraw {
            term.clear_last_lines(items.len())?;
            render_checkboxes(term, items, checked, *cursor)?;
        }
    }
}

fn render_checkboxes(
    term: &Term,
    items: &[String],
    checked: &[bool],
    cursor: usize,
) -> io::Result<()> {
    for (index, item) in items.iter().enumerate() {
        let checkbox = if checked[index] { "[x]" } else { "[ ]" };
        if index == cursor {
            term.write_line(&format!(
                "{} {} {}",
                style(">").for_stderr().cyan(),
                checkbox,
                style(item).for_stderr().bold(),
            ))?;
        } else {
            term.write_line(&format!("  {} {}", checkbox, item))?;
        }
    }
    Ok(())
}
