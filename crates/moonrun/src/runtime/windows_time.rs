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

//! Win32 operations on a capture of the configured system timezone.
use std::io;
use windows_sys::Win32::System::Time::{
    DYNAMIC_TIME_ZONE_INFORMATION, GetDynamicTimeZoneInformation,
    GetDynamicTimeZoneInformationEffectiveYears, GetTimeZoneInformationForYear,
    TIME_ZONE_ID_INVALID, TIME_ZONE_INFORMATION,
};

pub(crate) struct TimeZone(pub(crate) DYNAMIC_TIME_ZONE_INFORMATION);

impl TimeZone {
    pub(crate) fn capture() -> io::Result<Self> {
        let mut zone = DYNAMIC_TIME_ZONE_INFORMATION::default();
        if unsafe { GetDynamicTimeZoneInformation(&mut zone) } == TIME_ZONE_ID_INVALID {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(zone))
    }

    pub(crate) fn effective_years(&self) -> io::Result<(u32, u32)> {
        let (mut first, mut last) = (0, 0);
        let error =
            unsafe { GetDynamicTimeZoneInformationEffectiveYears(&self.0, &mut first, &mut last) };
        if error != 0 {
            return Err(io::Error::from_raw_os_error(error as i32));
        }
        Ok((first, last))
    }

    pub(crate) fn for_year(&self, year: u16) -> io::Result<TIME_ZONE_INFORMATION> {
        let mut rule = TIME_ZONE_INFORMATION::default();
        if unsafe { GetTimeZoneInformationForYear(year, &self.0, &mut rule) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(rule)
    }
}
