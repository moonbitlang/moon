// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Backend-neutral state and diagnostics for `moonbit:ffi/memory-sanitizer`.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::rc::Rc;

pub(crate) const MEMORY_SANITIZER_MODULE: &str = "moonbit:ffi/memory-sanitizer";

#[derive(Debug)]
struct ObjectRecord {
    size: u32,
    alloc_stack: Rc<SanitizerStack>,
}

#[derive(Debug, Default)]
struct MemorySanitizerState {
    live: BTreeMap<u32, ObjectRecord>,
    // Allocations usually come from a small number of call stacks. Share their
    // frame data so each live object only needs to retain an Rc.
    alloc_stacks: HashSet<Rc<SanitizerStack>>,
}

impl MemorySanitizerState {
    fn register_object_alloc(
        &mut self,
        size: u32,
        ptr: u32,
        capture_alloc_stack: impl FnOnce() -> SanitizerStack,
    ) -> Result<(), MemorySanitizerError> {
        if let Some(record) = self.live.get(&ptr) {
            return Err(MemorySanitizerError::DuplicateObject {
                ptr,
                size: record.size,
                alloc_stack: Rc::clone(&record.alloc_stack),
            });
        }

        let alloc_stack = capture_alloc_stack();
        let alloc_stack = if let Some(interned) = self.alloc_stacks.get(&alloc_stack) {
            Rc::clone(interned)
        } else {
            let alloc_stack = Rc::new(alloc_stack);
            self.alloc_stacks.insert(Rc::clone(&alloc_stack));
            alloc_stack
        };
        self.live.insert(ptr, ObjectRecord { size, alloc_stack });
        Ok(())
    }

    fn register_object_free(&mut self, ptr: u32) -> Result<(), MemorySanitizerError> {
        let record = self
            .live
            .remove(&ptr)
            .ok_or(MemorySanitizerError::InvalidObject { ptr })?;
        if Rc::strong_count(&record.alloc_stack) == 2 {
            self.alloc_stacks.remove(record.alloc_stack.as_ref());
        }
        Ok(())
    }

    fn object_is_valid(&self, ptr: u32) -> bool {
        self.live.contains_key(&ptr)
    }
}

#[derive(Clone, Default)]
pub(crate) struct MemorySanitizer {
    // Engine adapters invoke these imports synchronously on one run thread.
    // RefCell provides the interior mutability required by their callbacks.
    state: Rc<RefCell<MemorySanitizerState>>,
}

impl MemorySanitizer {
    pub(crate) fn register_object_alloc(
        &self,
        size: u32,
        ptr: u32,
        capture_alloc_stack: impl FnOnce() -> SanitizerStack,
    ) -> Result<(), MemorySanitizerError> {
        self.state
            .borrow_mut()
            .register_object_alloc(size, ptr, capture_alloc_stack)
    }

    pub(crate) fn register_object_free(&self, ptr: u32) -> Result<(), MemorySanitizerError> {
        self.state.borrow_mut().register_object_free(ptr)
    }

    pub(crate) fn object_is_valid(&self, ptr: u32) -> bool {
        self.state.borrow().object_is_valid(ptr)
    }

    pub(crate) fn check_for_leaks(&self) -> anyhow::Result<()> {
        let state = self.state.borrow();
        if state.live.is_empty() {
            return Ok(());
        }

        let total_size: u64 = state
            .live
            .values()
            .map(|object| u64::from(object.size))
            .sum();
        let mut report = format!(
            "moonrun memory sanitizer detected {} leaked object{} ({total_size} bytes)",
            state.live.len(),
            if state.live.len() == 1 { "" } else { "s" }
        );
        for (&ptr, object) in &state.live {
            write!(&mut report, "\nleaked object {ptr} ({} bytes)", object.size).unwrap();
            object
                .alloc_stack
                .write_to(&mut report, "allocation stack")
                .unwrap();
        }
        Err(anyhow::anyhow!(report))
    }
}

#[derive(Debug)]
pub(crate) enum MemorySanitizerError {
    DuplicateObject {
        ptr: u32,
        size: u32,
        alloc_stack: Rc<SanitizerStack>,
    },
    InvalidObject {
        ptr: u32,
    },
}

impl std::fmt::Display for MemorySanitizerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateObject {
                ptr,
                size,
                alloc_stack,
            } => {
                write!(f, "object {ptr} is already live with size {size}")?;
                alloc_stack.write_to(f, "previous allocation stack")
            }
            Self::InvalidObject { ptr } => write!(f, "invalid object {ptr}"),
        }
    }
}

#[derive(Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct SanitizerStack {
    frames: Vec<SanitizerStackFrame>,
}

impl SanitizerStack {
    pub(crate) fn new(frames: Vec<SanitizerStackFrame>) -> Self {
        Self { frames }
    }

    fn write_to(&self, f: &mut impl std::fmt::Write, title: &str) -> std::fmt::Result {
        if self.frames.is_empty() {
            return Ok(());
        }
        write!(f, "\n{title}:")?;
        for frame in &self.frames {
            if frame.is_wasm {
                write!(
                    f,
                    "\n    at {}",
                    moonutil::demangle::demangle_mangled_function_name(&frame.raw_function)
                )?;
            } else {
                write!(f, "\n    at {}", frame.raw_function)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Eq, Hash, PartialEq)]
pub(crate) struct SanitizerStackFrame {
    raw_function: String,
    is_wasm: bool,
}

impl SanitizerStackFrame {
    pub(crate) fn new(raw_function: String, is_wasm: bool) -> Self {
        Self {
            raw_function,
            is_wasm,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(function: &str) -> SanitizerStack {
        SanitizerStack {
            frames: vec![SanitizerStackFrame {
                raw_function: function.to_string(),
                is_wasm: true,
            }],
        }
    }

    #[test]
    fn duplicate_allocation_does_not_capture_an_unused_stack() {
        let mut state = MemorySanitizerState::default();
        state
            .register_object_alloc(16, 1024, || stack("first"))
            .unwrap();

        let error = state.register_object_alloc(32, 1024, || {
            panic!("duplicate allocation should reuse the previous stack")
        });

        assert!(matches!(
            error,
            Err(MemorySanitizerError::DuplicateObject {
                ptr: 1024,
                size: 16,
                ..
            })
        ));
    }

    #[test]
    fn allocation_stacks_are_shared_while_live() {
        let mut state = MemorySanitizerState::default();
        state
            .register_object_alloc(16, 1024, || stack("shared"))
            .unwrap();
        state
            .register_object_alloc(16, 2048, || stack("shared"))
            .unwrap();

        assert_eq!(state.alloc_stacks.len(), 1);
        assert!(Rc::ptr_eq(
            &state.live[&1024].alloc_stack,
            &state.live[&2048].alloc_stack
        ));

        state.register_object_free(1024).unwrap();
        assert_eq!(state.alloc_stacks.len(), 1);
        state.register_object_free(2048).unwrap();
        assert!(state.alloc_stacks.is_empty());
    }
}
