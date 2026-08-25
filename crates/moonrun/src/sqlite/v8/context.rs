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

use std::ffi::CString;

use crate::guest_memory::{GuestMemory, GuestMemoryError};
use crate::sqlite::{SqliteHost, SqliteHostError};
use crate::v8::context::{V8ImportError, V8RunContext};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SqliteError {
    Fault,
    InvalidHandle,
    Overflow,
}

pub(super) type SqliteResult<T> = Result<T, SqliteError>;

pub(super) fn with_memory_context<T>(
    scope: &mut v8::HandleScope,
    context: &V8RunContext,
    f: impl FnOnce(&mut ImportContext<'_>) -> SqliteResult<T>,
) -> SqliteResult<T> {
    context.memory_binding().with_memory_mut(scope, |memory| {
        f(&mut ImportContext {
            host: context.runtime().sqlite(),
            memory,
        })
    })
}

pub(super) struct ImportContext<'a> {
    pub(super) host: &'a SqliteHost,
    memory: &'a mut [u8],
}

impl ImportContext<'_> {
    pub(super) fn host_and_memory(&mut self) -> (&SqliteHost, &mut [u8]) {
        (self.host, self.memory)
    }

    /// Borrow a UTF-16 output buffer after validating the Guest Memory range.
    pub(super) fn with_utf16_output<T>(
        &mut self,
        pointer: u32,
        capacity: u32,
        f: impl FnOnce(&SqliteHost, &mut [u16]) -> SqliteResult<T>,
    ) -> SqliteResult<T> {
        let byte_capacity = capacity.checked_mul(2).ok_or(SqliteError::Overflow)?;
        let bytes = if capacity == 0 {
            &mut []
        } else {
            if pointer == 0 {
                return Err(SqliteError::Fault);
            }
            self.memory.read_exact_mut(pointer, byte_capacity)?
        };
        // SAFETY: every bit pattern is a valid `u16`; the alignment check
        // rejects a Guest Memory range that does not satisfy this ABI's
        // promise.
        let (prefix, output, suffix) = unsafe { bytes.align_to_mut::<u16>() };
        if !prefix.is_empty() || !suffix.is_empty() {
            return Err(SqliteError::Fault);
        }
        f(self.host, output)
    }

    /// Borrow one UTF-16 view from a MoonBit backing String.
    ///
    /// Offset and length are UTF-16 code units, not bytes. The caller lowers
    /// signed ABI values first; unaligned backing pointers, overflow, and
    /// out-of-bounds views violate the wasm ABI contract.
    pub(super) fn read_utf16_view(
        &self,
        pointer: u32,
        offset: u32,
        length: u32,
    ) -> SqliteResult<&[u16]> {
        if pointer == 0 || !pointer.is_multiple_of(2) {
            return Err(SqliteError::Fault);
        }
        let byte_offset = offset.checked_mul(2).ok_or(SqliteError::Overflow)?;
        let byte_length = length.checked_mul(2).ok_or(SqliteError::Overflow)?;
        let pointer = pointer
            .checked_add(byte_offset)
            .ok_or(SqliteError::Overflow)?;
        let bytes = self.memory.read_exact(pointer, byte_length)?;
        // SAFETY: every bit pattern is a valid `u16`. The prefix/suffix check
        // below rejects a runtime memory backing that does not provide the
        // alignment promised by this UTF-16 ABI.
        let (prefix, units, suffix) = unsafe { bytes.align_to::<u16>() };
        if !prefix.is_empty() || !suffix.is_empty() {
            return Err(SqliteError::Fault);
        }
        Ok(units)
    }

    /// Borrow one byte view from a MoonBit backing Bytes value.
    pub(super) fn read_bytes_view(
        &self,
        pointer: u32,
        offset: u32,
        length: u32,
    ) -> SqliteResult<&[u8]> {
        if pointer == 0 {
            return Err(SqliteError::Fault);
        }
        let pointer = pointer.checked_add(offset).ok_or(SqliteError::Overflow)?;
        Ok(self.memory.read_exact(pointer, length)?)
    }

    /// Copy one length-delimited UTF-8 string from a MoonBit Bytes value.
    ///
    /// MoonBit's wasm Bytes representation does not provide a trailing NUL, so
    /// the owned result adds the terminator required by SQLite. The remaining
    /// checks reject interior NULs and invalid UTF-8 filenames.
    pub(super) fn read_utf8_c_string(&self, pointer: u32, length: u32) -> SqliteResult<CString> {
        if pointer == 0 {
            return Err(SqliteError::Fault);
        }
        let bytes = self.memory.read_exact(pointer, length)?;
        let value = CString::new(bytes).map_err(|_| SqliteError::Fault)?;
        value.to_str().map_err(|_| SqliteError::Fault)?;
        Ok(value)
    }

    pub(super) fn validate_write(&self, pointer: u32, length: usize) -> SqliteResult<()> {
        if pointer == 0 {
            return Err(SqliteError::Fault);
        }
        let length = u32::try_from(length).map_err(|_| SqliteError::Overflow)?;
        self.memory.read_exact(pointer, length)?;
        Ok(())
    }

    pub(super) fn write_u32(&mut self, pointer: u32, value: u32) -> SqliteResult<()> {
        self.write_exact(pointer, &value.to_le_bytes())
    }

    pub(super) fn write_u64(&mut self, pointer: u32, value: u64) -> SqliteResult<()> {
        self.write_exact(pointer, &value.to_le_bytes())
    }

    fn write_exact(&mut self, pointer: u32, value: &[u8]) -> SqliteResult<()> {
        if pointer == 0 {
            return Err(SqliteError::Fault);
        }
        self.memory.write_exact(pointer, value)?;
        Ok(())
    }
}

impl From<GuestMemoryError> for SqliteError {
    fn from(_error: GuestMemoryError) -> Self {
        Self::Fault
    }
}

impl From<V8ImportError> for SqliteError {
    fn from(_error: V8ImportError) -> Self {
        Self::Fault
    }
}

impl From<SqliteHostError> for SqliteError {
    fn from(error: SqliteHostError) -> Self {
        match error {
            SqliteHostError::InvalidHandle => Self::InvalidHandle,
            SqliteHostError::InvalidInput => Self::Fault,
            SqliteHostError::Overflow => Self::Overflow,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    use crate::policy::Policy;
    use crate::runtime::HostKeys;

    fn host() -> SqliteHost {
        SqliteHost::with_keys(
            Arc::new(Policy::allow_all()),
            Rc::new(RefCell::new(HostKeys::default())),
        )
    }

    #[test]
    fn utf16_view_uses_code_unit_offsets_and_lengths() {
        let host = host();
        let mut memory = (0_u8..16).collect::<Vec<_>>();
        let context = ImportContext {
            host: &host,
            memory: &mut memory,
        };

        assert_eq!(
            context.read_utf16_view(2, 2, 2),
            Ok(&[u16::from_le_bytes([6, 7]), u16::from_le_bytes([8, 9])][..])
        );
        assert_eq!(context.read_utf16_view(3, 0, 1), Err(SqliteError::Fault));
        assert_eq!(
            context.read_utf16_view(2, u32::MAX, 1),
            Err(SqliteError::Overflow)
        );
    }

    #[test]
    fn utf8_c_string_adds_termination_after_the_explicit_length() {
        let host = host();
        let mut memory = b"x:memory:".to_vec();
        let context = ImportContext {
            host: &host,
            memory: &mut memory,
        };

        assert_eq!(context.read_utf8_c_string(1, 8), Ok(c":memory:".to_owned()));
    }

    #[test]
    fn utf8_c_string_rejects_interior_nul_and_invalid_utf8() {
        let host = host();
        let mut memory = b"xabc\0def\xff".to_vec();
        let context = ImportContext {
            host: &host,
            memory: &mut memory,
        };

        assert_eq!(context.read_utf8_c_string(1, 7), Err(SqliteError::Fault));
        assert_eq!(context.read_utf8_c_string(8, 1), Err(SqliteError::Fault));
    }

    #[test]
    fn byte_view_uses_byte_offsets_and_lengths() {
        let host = host();
        let mut memory = (0_u8..8).collect::<Vec<_>>();
        let context = ImportContext {
            host: &host,
            memory: &mut memory,
        };

        assert_eq!(context.read_bytes_view(1, 2, 3), Ok(&[3, 4, 5][..]));
        assert_eq!(
            context.read_bytes_view(1, u32::MAX, 1),
            Err(SqliteError::Overflow)
        );
    }
}
