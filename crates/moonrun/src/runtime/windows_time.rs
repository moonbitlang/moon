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

//! Owns captured system timezones and their guest-visible handle lifetimes.
use std::cell::{Ref, RefCell};
use std::io;
use std::rc::Rc;

use slotmap::{Key, KeyData};
use windows_sys::Win32::Foundation::SYSTEMTIME;
use windows_sys::Win32::System::Time::{
    DYNAMIC_TIME_ZONE_INFORMATION, GetDynamicTimeZoneInformation,
    GetDynamicTimeZoneInformationEffectiveYears, GetTimeZoneInformationForYear,
    TIME_ZONE_ID_INVALID, TIME_ZONE_INFORMATION,
};

use super::{Handles, HostKeys, HostResourceKind};
use crate::async_host::{AsyncHostError, AsyncHostResult};

pub(crate) struct Metadata {
    pub(crate) key_name: Vec<u16>,
    pub(crate) dynamic_dst_disabled: bool,
}

pub(crate) struct Rule {
    pub(crate) bias: i32,
    pub(crate) standard_bias: i32,
    pub(crate) daylight_bias: i32,
    pub(crate) standard_date: SYSTEMTIME,
    pub(crate) daylight_date: SYSTEMTIME,
}

pub(crate) struct TimeZones {
    captures: RefCell<Handles<DYNAMIC_TIME_ZONE_INFORMATION>>,
}

impl TimeZones {
    pub(crate) fn new(keys: Rc<RefCell<HostKeys>>) -> Self {
        Self {
            captures: RefCell::new(Handles::new(keys, HostResourceKind::WindowsTimeZone)),
        }
    }

    pub(crate) fn capture(&self) -> AsyncHostResult<u64> {
        let mut zone = DYNAMIC_TIME_ZONE_INFORMATION::default();
        if unsafe { GetDynamicTimeZoneInformation(&mut zone) } == TIME_ZONE_ID_INVALID {
            return Err(io::Error::last_os_error().into());
        }
        Ok(self.captures.borrow_mut().insert(zone).data().as_ffi())
    }

    fn captured(&self, handle: u64) -> AsyncHostResult<Ref<'_, DYNAMIC_TIME_ZONE_INFORMATION>> {
        Ref::filter_map(self.captures.borrow(), |captures| {
            captures.get(KeyData::from_ffi(handle).into())
        })
        .map_err(|_| AsyncHostError::Badf)
    }

    pub(crate) fn metadata(&self, handle: u64) -> AsyncHostResult<Metadata> {
        let zone = self.captured(handle)?;
        let key = &zone.TimeZoneKeyName;
        let len = key.iter().position(|&unit| unit == 0).unwrap_or(key.len());
        Ok(Metadata {
            key_name: key[..len].to_vec(),
            dynamic_dst_disabled: zone.DynamicDaylightTimeDisabled,
        })
    }

    pub(crate) fn current_rule(&self, handle: u64) -> AsyncHostResult<Rule> {
        let zone = self.captured(handle)?;
        Ok(Rule {
            bias: zone.Bias,
            standard_bias: zone.StandardBias,
            daylight_bias: zone.DaylightBias,
            standard_date: zone.StandardDate,
            daylight_date: zone.DaylightDate,
        })
    }

    pub(crate) fn effective_years(&self, handle: u64) -> AsyncHostResult<(u32, u32)> {
        let zone = self.captured(handle)?;
        let (mut first, mut last) = (0, 0);
        let error =
            unsafe { GetDynamicTimeZoneInformationEffectiveYears(&*zone, &mut first, &mut last) };
        if error != 0 {
            return Err(AsyncHostError::Native(error as i32));
        }
        Ok((first, last))
    }

    pub(crate) fn for_year(&self, handle: u64, year: u32) -> AsyncHostResult<Rule> {
        let year = u16::try_from(year).map_err(|_| AsyncHostError::Inval)?;
        let zone = self.captured(handle)?;
        let mut rule = TIME_ZONE_INFORMATION::default();
        if unsafe { GetTimeZoneInformationForYear(year, &*zone, &mut rule) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok(Rule {
            bias: rule.Bias,
            standard_bias: rule.StandardBias,
            daylight_bias: rule.DaylightBias,
            standard_date: rule.StandardDate,
            daylight_date: rule.DaylightDate,
        })
    }

    pub(crate) fn release(&self, handle: u64) -> AsyncHostResult<()> {
        self.captures
            .borrow_mut()
            .remove(KeyData::from_ffi(handle).into())
            .map(|_| ())
            .ok_or(AsyncHostError::Badf)
    }

    pub(crate) fn leak_summary(&self) -> Option<String> {
        let count = self.captures.borrow().len();
        (count != 0).then(|| format!("{count} Windows timezone captures"))
    }
}
