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
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Fixed-width, little-endian records for linear Wasm. The Win32 timezone
//! remains in the Runtime: only the captured OS value can select a registry key.
#[cfg(windows)]
pub(super) use windows::{capture, current_rule, effective_years, for_year, metadata, release};

#[cfg(windows)]
mod windows {
    use crate::async_host::{AsyncHostError, AsyncHostResult};
    use crate::guest_memory::GuestMemory;
    use crate::runtime::{Runtime, windows_time::TimeZone};
    use slotmap::{Key, KeyData};
    use windows_sys::Win32::Foundation::SYSTEMTIME;

    // Three signed minute biases, followed by two eight-word SYSTEMTIMEs.
    const RULE_SIZE: usize = 44;
    fn write_rule(out: &mut [u8], biases: [i32; 3], dates: [SYSTEMTIME; 2]) {
        for (word, bias) in out[..12].chunks_exact_mut(4).zip(biases) {
            word.copy_from_slice(&bias.to_le_bytes());
        }
        let fields = dates.into_iter().flat_map(|d| {
            [
                d.wYear,
                d.wMonth,
                d.wDayOfWeek,
                d.wDay,
                d.wHour,
                d.wMinute,
                d.wSecond,
                d.wMilliseconds,
            ]
        });
        for (word, field) in out[12..].chunks_exact_mut(2).zip(fields) {
            word.copy_from_slice(&field.to_le_bytes());
        }
    }

    pub(in crate::core_api) fn capture(
        rt: &Runtime,
        memory: &mut [u8],
        ptr: u32,
    ) -> AsyncHostResult<()> {
        // Validate the handle slot before acquiring any host-owned state.
        let out = memory.read_exact_mut(ptr, 8)?;
        let zone = TimeZone::capture()?;
        let key = rt.windows_time_zones.borrow_mut().insert(zone);
        out.copy_from_slice(&key.data().as_ffi().to_le_bytes());
        Ok(())
    }

    pub(in crate::core_api) fn metadata(
        rt: &Runtime,
        handle: u64,
        memory: &mut [u8],
        ptr: u32,
        capacity: u32,
    ) -> AsyncHostResult<()> {
        use windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;

        let out = memory.read_exact_mut(ptr, capacity)?;
        let zones = rt.windows_time_zones.borrow();
        let zone = zones
            .get(KeyData::from_ffi(handle).into())
            .ok_or(AsyncHostError::Badf)?;
        let key = &zone.0.TimeZoneKeyName;
        let name = &key[..key.iter().position(|&unit| unit == 0).unwrap_or(key.len())];
        // Win32 bounds the key name at 128 code units. Write only its actual
        // bytes, leaving the rest of the caller's buffer untouched.
        let name_bytes = name.len() * 2;
        let out = out
            .get_mut(..8 + name_bytes)
            .ok_or(AsyncHostError::Native(ERROR_INSUFFICIENT_BUFFER as i32))?;
        out[..4].copy_from_slice(&u32::from(zone.0.DynamicDaylightTimeDisabled).to_le_bytes());
        out[4..8].copy_from_slice(&(name_bytes as u32).to_le_bytes());
        for (dst, unit) in out[8..].chunks_exact_mut(2).zip(name) {
            dst.copy_from_slice(&unit.to_le_bytes());
        }
        Ok(())
    }

    pub(in crate::core_api) fn current_rule(
        rt: &Runtime,
        handle: u64,
        memory: &mut [u8],
        ptr: u32,
    ) -> AsyncHostResult<()> {
        let out = memory.read_exact_mut(ptr, RULE_SIZE as u32)?;
        let zones = rt.windows_time_zones.borrow();
        let d = &zones
            .get(KeyData::from_ffi(handle).into())
            .ok_or(AsyncHostError::Badf)?
            .0;
        write_rule(
            out,
            [d.Bias, d.StandardBias, d.DaylightBias],
            [d.StandardDate, d.DaylightDate],
        );
        Ok(())
    }

    pub(in crate::core_api) fn effective_years(
        rt: &Runtime,
        handle: u64,
        memory: &mut [u8],
        ptr: u32,
    ) -> AsyncHostResult<()> {
        let out = memory.read_exact_mut(ptr, 8)?;
        let zones = rt.windows_time_zones.borrow();
        let zone = zones
            .get(KeyData::from_ffi(handle).into())
            .ok_or(AsyncHostError::Badf)?;
        let (first, last) = zone.effective_years()?;
        out[..4].copy_from_slice(&first.to_le_bytes());
        out[4..].copy_from_slice(&last.to_le_bytes());
        Ok(())
    }

    pub(in crate::core_api) fn for_year(
        rt: &Runtime,
        handle: u64,
        year: u32,
        memory: &mut [u8],
        ptr: u32,
    ) -> AsyncHostResult<()> {
        let out = memory.read_exact_mut(ptr, RULE_SIZE as u32)?;
        let year = u16::try_from(year).map_err(|_| AsyncHostError::Inval)?;
        let zones = rt.windows_time_zones.borrow();
        let zone = zones
            .get(KeyData::from_ffi(handle).into())
            .ok_or(AsyncHostError::Badf)?;
        let rule = zone.for_year(year)?;
        write_rule(
            out,
            [rule.Bias, rule.StandardBias, rule.DaylightBias],
            [rule.StandardDate, rule.DaylightDate],
        );
        Ok(())
    }

    pub(in crate::core_api) fn release(rt: &Runtime, handle: u64) -> AsyncHostResult<()> {
        rt.windows_time_zones
            .borrow_mut()
            .remove(KeyData::from_ffi(handle).into())
            .map(|_| ())
            .ok_or(AsyncHostError::Badf)
    }
}

// Every engine must link the same portable Wasm binary on Unix, even when
// its platform dispatch never executes a Windows import.
#[cfg(unix)]
mod unsupported {
    use crate::async_host::{AsyncHostError, AsyncHostResult};
    use crate::runtime::Runtime;
    const NOT_SUPPORTED: AsyncHostError = AsyncHostError::Native(libc::ENOTSUP);
    pub(in crate::core_api) fn capture(_: &Runtime, _: &mut [u8], _: u32) -> AsyncHostResult<()> {
        Err(NOT_SUPPORTED)
    }
    pub(in crate::core_api) fn metadata(
        _: &Runtime,
        _: u64,
        _: &mut [u8],
        _: u32,
        _: u32,
    ) -> AsyncHostResult<()> {
        Err(NOT_SUPPORTED)
    }
    pub(in crate::core_api) fn current_rule(
        _: &Runtime,
        _: u64,
        _: &mut [u8],
        _: u32,
    ) -> AsyncHostResult<()> {
        Err(NOT_SUPPORTED)
    }
    pub(in crate::core_api) fn effective_years(
        _: &Runtime,
        _: u64,
        _: &mut [u8],
        _: u32,
    ) -> AsyncHostResult<()> {
        Err(NOT_SUPPORTED)
    }
    pub(in crate::core_api) fn for_year(
        _: &Runtime,
        _: u64,
        _: u32,
        _: &mut [u8],
        _: u32,
    ) -> AsyncHostResult<()> {
        Err(NOT_SUPPORTED)
    }
    pub(in crate::core_api) fn release(_: &Runtime, _: u64) -> AsyncHostResult<()> {
        Err(NOT_SUPPORTED)
    }
}
#[cfg(unix)]
pub(super) use unsupported::{capture, current_rule, effective_years, for_year, metadata, release};
