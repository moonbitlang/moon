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

//! Generic-stat request validation, Job authorization, and guest result encoding.

use std::ffi::OsString;

#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::filesystem::metadata::{file_kind_code, query_resource_stat};
use crate::filesystem::os::stat::{self, StatValues};
#[cfg(test)]
use crate::filesystem::os::stat::{FileKind, StatTimestamp};
use crate::filesystem::{FsOperation, FsTarget, HostFs};
use crate::resource::ResourceRef;

pub(super) const STAT_FILE_KIND: u32 = 0x0001;
pub(super) const STAT_FILE_SIZE: u32 = 0x0002;
pub(super) const STAT_DEVICE_ID: u32 = 0x0004;
pub(super) const STAT_FILE_ID: u32 = 0x0008;
pub(super) const STAT_ACCESS_TIME: u32 = 0x0010;
pub(super) const STAT_MODIFY_TIME: u32 = 0x0020;
pub(super) const STAT_CHANGE_TIME: u32 = 0x0040;
pub(super) const STAT_CREATE_TIME: u32 = 0x0080;

pub(crate) const STAT_OPEN_IDENTITY: u32 = STAT_FILE_KIND | STAT_DEVICE_ID | STAT_FILE_ID;

const STAT_SUPPORTED_PROPERTY_MASK: u32 = 0x00ff;
const STAT_HEADER_LEN: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StatRequest {
    mask: u32,
    encoded_len: usize,
}

impl StatRequest {
    pub(super) const fn open_identity() -> Self {
        // Each identity property occupies one 64-bit word after the header.
        Self {
            mask: STAT_OPEN_IDENTITY,
            encoded_len: STAT_HEADER_LEN
                + STAT_OPEN_IDENTITY.count_ones() as usize * std::mem::size_of::<u64>(),
        }
    }

    pub(super) fn new(mask: u32, capacity: u32) -> AsyncHostResult<Self> {
        let header_len = u32::try_from(STAT_HEADER_LEN).map_err(|_| AsyncHostError::Fault)?;
        if mask & !STAT_SUPPORTED_PROPERTY_MASK != 0
            || capacity < header_len
            || capacity > i32::MAX as u32
        {
            return Err(AsyncHostError::Inval);
        }
        let property_words = (0..8)
            .filter(|bit| mask & (1 << bit) != 0)
            .map(|bit| if bit < 4 { 1 } else { 2 })
            .sum::<usize>();
        let encoded_len = STAT_HEADER_LEN + property_words * 8;
        if encoded_len > capacity as usize {
            return Err(AsyncHostError::Inval);
        }
        Ok(Self { mask, encoded_len })
    }

    pub(super) fn mask(self) -> u32 {
        self.mask
    }

    pub(super) fn fields(self) -> u32 {
        [
            (STAT_FILE_KIND, stat::KIND),
            (STAT_FILE_SIZE, stat::SIZE),
            (STAT_DEVICE_ID, stat::DEVICE_ID),
            (STAT_FILE_ID, stat::FILE_ID),
            (STAT_ACCESS_TIME, stat::ACCESS_TIME),
            (STAT_MODIFY_TIME, stat::MODIFY_TIME),
            (STAT_CHANGE_TIME, stat::CHANGE_TIME),
            (STAT_CREATE_TIME, stat::CREATE_TIME),
        ]
        .into_iter()
        .filter(|(abi, _)| self.mask & abi != 0)
        .fold(0, |fields, (_, field)| fields | field)
    }

    pub(super) fn encoded_len(self) -> usize {
        self.encoded_len
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct PackedStat {
    request: StatRequest,
    bytes: Box<[u8]>,
}

impl PackedStat {
    pub(super) fn new(request: StatRequest, values: StatValues) -> Self {
        let mut bytes = vec![0; request.encoded_len()];
        let mut returned_mask = 0_u32;
        let mut offset = STAT_HEADER_LEN;

        for bit in 0..8 {
            let property = 1_u32 << bit;
            if request.mask() & property == 0 {
                continue;
            }
            let mut write_word = |value: [u8; 8]| {
                bytes[offset..offset + 8].copy_from_slice(&value);
                offset += 8;
            };
            match property {
                STAT_FILE_KIND => {
                    if let Some(value) = values.kind {
                        returned_mask |= property;
                        write_word(u64::from(file_kind_code(value) as u32).to_le_bytes());
                    } else {
                        offset += 8;
                    }
                }
                STAT_FILE_SIZE => {
                    if let Some(value) = values.size {
                        returned_mask |= property;
                        write_word(value.to_le_bytes());
                    } else {
                        offset += 8;
                    }
                }
                STAT_DEVICE_ID => {
                    if let Some(value) = values.device_id {
                        returned_mask |= property;
                        write_word(value.to_le_bytes());
                    } else {
                        offset += 8;
                    }
                }
                STAT_FILE_ID => {
                    if let Some(value) = values.file_id {
                        returned_mask |= property;
                        write_word(value.to_le_bytes());
                    } else {
                        offset += 8;
                    }
                }
                STAT_ACCESS_TIME | STAT_MODIFY_TIME | STAT_CHANGE_TIME | STAT_CREATE_TIME => {
                    let value = match property {
                        STAT_ACCESS_TIME => values.access_time,
                        STAT_MODIFY_TIME => values.modify_time,
                        STAT_CHANGE_TIME => values.change_time,
                        STAT_CREATE_TIME => values.create_time,
                        _ => unreachable!(),
                    };
                    if let Some(value) = value {
                        returned_mask |= property;
                        write_word(value.seconds.to_le_bytes());
                        write_word(value.nanoseconds.to_le_bytes());
                    } else {
                        offset += 16;
                    }
                }
                _ => unreachable!(),
            }
        }

        bytes[0..4].copy_from_slice(&(request.encoded_len() as u32).to_le_bytes());
        bytes[4..8].copy_from_slice(&returned_mask.to_le_bytes());
        Self {
            request,
            bytes: bytes.into_boxed_slice(),
        }
    }

    pub(super) fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(super) fn scalar(&self, property: u32) -> Option<u64> {
        if property.count_ones() != 1 || self.request.mask() & property == 0 {
            return None;
        }
        // Read the fixed slot, not the returned-property mask: legacy open
        // getters exposed an unavailable identity value as zero.
        let property_index = property.trailing_zeros();
        let preceding_words = (0..property_index)
            .filter(|bit| self.request.mask() & (1 << bit) != 0)
            .map(|bit| if bit < 4 { 1 } else { 2 })
            .sum::<usize>();
        self.bytes
            .get(STAT_HEADER_LEN + preceding_words * 8..STAT_HEADER_LEN + (preceding_words + 1) * 8)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u64::from_le_bytes)
    }
}

pub(super) fn run_fstatx_job(
    filesystem: &HostFs,
    file: &mut Option<ResourceRef>,
    request: StatRequest,
    result: &mut Option<PackedStat>,
) -> AsyncHostResult<i64> {
    let target = FsTarget::File(file.as_deref().ok_or(AsyncHostError::Badf)?.file_grant());
    filesystem.authorize(if request.mask() == STAT_FILE_KIND {
        FsOperation::Kind(target)
    } else {
        FsOperation::Metadata(target)
    })?;
    let file = file.take().ok_or(AsyncHostError::Badf)?;
    *result = Some(PackedStat::new(
        request,
        query_resource_stat(&file, request.fields())?,
    ));
    Ok(0)
}

pub(super) fn run_statx_job(
    filesystem: &HostFs,
    parent: &mut Option<ResourceRef>,
    path: &mut OsString,
    request: StatRequest,
    follow_symlink: bool,
    result: &mut Option<PackedStat>,
) -> AsyncHostResult<i64> {
    let target = super::job_path(parent.as_deref(), path)?;
    filesystem.authorize(if follow_symlink {
        FsOperation::Metadata(FsTarget::Path(target))
    } else {
        FsOperation::InspectEntry(target)
    })?;
    let path = super::take_execution_path(filesystem, parent.as_deref(), path);
    let parent = parent.take();
    let parent = parent.as_deref();

    #[cfg(unix)]
    let parent = parent
        .map(|resource| resource.as_file().map(|file| file.as_raw_fd()))
        .transpose()?;
    #[cfg(windows)]
    let parent = parent
        .map(|resource| resource.as_file().map(|file| file.as_raw_handle()))
        .transpose()?;

    let values = stat::query_path(
        parent,
        &path,
        follow_symlink,
        request.fields(),
        &super::CancellableIo,
    )?;
    *result = Some(PackedStat::new(request, values));
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::Resource;

    #[test]
    fn request_layout_is_stable_for_every_supported_mask() {
        for mask in 0..=STAT_SUPPORTED_PROPERTY_MASK {
            let expected_words = (0..8)
                .filter(|bit| mask & (1 << bit) != 0)
                .map(|bit| if bit < 4 { 1 } else { 2 })
                .sum::<usize>();
            let expected_len = STAT_HEADER_LEN + expected_words * 8;

            let request = StatRequest::new(mask, u32::try_from(expected_len).unwrap()).unwrap();

            assert_eq!(request.mask(), mask);
            assert_eq!(request.encoded_len(), expected_len);
        }
    }

    #[test]
    fn request_rejects_unknown_properties_and_short_buffers() {
        assert_eq!(StatRequest::new(0x0100, 8), Err(AsyncHostError::Inval));
        assert_eq!(StatRequest::new(0, 7), Err(AsyncHostError::Inval));
        assert_eq!(StatRequest::new(0, u32::MAX), Err(AsyncHostError::Inval));
        assert_eq!(
            StatRequest::new(STAT_CREATE_TIME, 23),
            Err(AsyncHostError::Inval)
        );
    }

    #[test]
    fn individual_queries_preserve_native_properties_and_abi_slots() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), b"moonrun stat").unwrap();
        let filesystem = HostFs::default();
        let mut full = crate::filesystem::Job::statx(
            None,
            temp.path().as_os_str().to_owned(),
            STAT_SUPPORTED_PROPERTY_MASK,
            104,
            true,
        )
        .unwrap();
        full.run(&filesystem).unwrap();
        let full = full.stat_result().unwrap().as_bytes();
        let available = u32::from_le_bytes(full[4..8].try_into().unwrap());

        for (property, slot) in [
            (STAT_FILE_KIND, 8..16),
            (STAT_FILE_SIZE, 16..24),
            (STAT_DEVICE_ID, 24..32),
            (STAT_FILE_ID, 32..40),
            (STAT_ACCESS_TIME, 40..56),
            (STAT_MODIFY_TIME, 56..72),
            (STAT_CHANGE_TIME, 72..88),
            (STAT_CREATE_TIME, 88..104),
        ] {
            let capacity = (STAT_HEADER_LEN + slot.len()) as u32;
            let mut job = crate::filesystem::Job::statx(
                None,
                temp.path().as_os_str().to_owned(),
                property,
                capacity,
                true,
            )
            .unwrap();
            job.run(&filesystem).unwrap();
            let bytes = job.stat_result().unwrap().as_bytes();
            assert_eq!(bytes.len(), capacity as usize);
            assert_eq!(&bytes[..4], &capacity.to_le_bytes());
            assert_eq!(&bytes[4..8], &(available & property).to_le_bytes());
            if property <= STAT_FILE_ID {
                assert_eq!(&bytes[8..], &full[slot]);
            }
        }
    }

    #[test]
    fn packed_result_uses_fixed_little_endian_slots_and_returned_mask() {
        let mask = STAT_FILE_KIND | STAT_FILE_SIZE | STAT_MODIFY_TIME | STAT_CREATE_TIME;
        let request = StatRequest::new(mask, 56).unwrap();
        let packed = PackedStat::new(
            request,
            StatValues {
                kind: Some(FileKind::Directory),
                size: None,
                modify_time: Some(StatTimestamp {
                    seconds: -2,
                    nanoseconds: 123,
                }),
                create_time: None,
                ..StatValues::default()
            },
        );

        assert_eq!(packed.as_bytes().len(), 56);
        assert_eq!(&packed.as_bytes()[0..4], &56_u32.to_le_bytes());
        assert_eq!(
            &packed.as_bytes()[4..8],
            &(STAT_FILE_KIND | STAT_MODIFY_TIME).to_le_bytes()
        );
        assert_eq!(&packed.as_bytes()[8..16], &2_u64.to_le_bytes());
        assert_eq!(&packed.as_bytes()[16..24], &[0; 8]);
        assert_eq!(&packed.as_bytes()[24..32], &(-2_i64).to_le_bytes());
        assert_eq!(&packed.as_bytes()[32..40], &123_i64.to_le_bytes());
        assert_eq!(&packed.as_bytes()[40..56], &[0; 16]);
        assert_eq!(packed.scalar(STAT_FILE_KIND), Some(2));
        assert_eq!(packed.scalar(STAT_FILE_SIZE), Some(0));
        assert_eq!(packed.scalar(STAT_DEVICE_ID), None);
    }

    #[cfg(unix)]
    #[test]
    fn console_kind_distinguishes_a_terminal_from_other_character_devices() {
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        let (mut master, mut slave) = (-1, -1);
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            },
            0
        );
        let _master = unsafe { OwnedFd::from_raw_fd(master) };
        let slave = std::sync::Arc::new(Resource::new(slave));
        let request = StatRequest::new(STAT_FILE_KIND, 16).unwrap();
        let mut result = None;
        run_fstatx_job(
            &HostFs::default(),
            &mut Some(std::sync::Arc::clone(&slave)),
            request,
            &mut result,
        )
        .unwrap();
        assert_eq!(HostFs::default().resource_stdio_kind(&slave).unwrap(), 8);
        assert_eq!(result.unwrap().scalar(STAT_FILE_KIND), Some(8));

        let null = std::fs::File::open("/dev/null").unwrap();
        let null = std::sync::Arc::new(Resource::new(unsafe { libc::dup(null.as_raw_fd()) }));
        let mut result = None;
        run_fstatx_job(
            &HostFs::default(),
            &mut Some(std::sync::Arc::clone(&null)),
            request,
            &mut result,
        )
        .unwrap();
        assert_eq!(HostFs::default().resource_stdio_kind(&null).unwrap(), 7);
        assert_eq!(result.unwrap().scalar(STAT_FILE_KIND), Some(7));
    }

    #[cfg(unix)]
    #[test]
    fn completed_job_keeps_stat_in_host_memory_until_copy_out() {
        use std::os::fd::AsRawFd;

        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), b"moonrun stat").unwrap();
        let raw = unsafe { libc::dup(temp.as_file().as_raw_fd()) };
        assert!(raw >= 0);
        let request = STAT_FILE_KIND | STAT_FILE_SIZE | STAT_DEVICE_ID | STAT_FILE_ID;
        let mut job = crate::filesystem::Job::fstatx(
            std::sync::Arc::new(Resource::new(raw)),
            request,
            u32::try_from(StatRequest::new(request, 40).unwrap().encoded_len()).unwrap(),
        )
        .unwrap();

        assert_eq!(job.run(&HostFs::default()), Ok(0));

        let result = job.stat_result().unwrap();
        assert_eq!(result.as_bytes()[0..4], 40_u32.to_le_bytes());

        let mut guest = [0_u8; 48];
        job.copy_stat_result(0, &mut guest[..], 4, 40).unwrap();
        assert_eq!(guest[0..4], [0; 4]);
        assert_eq!(guest[4..8], 40_u32.to_le_bytes());
        assert_eq!(guest[8..12], request.to_le_bytes());
        assert_eq!(i64::from_le_bytes(guest[20..28].try_into().unwrap()), 12);
    }

    #[test]
    fn invalid_open_stat_request_is_rejected_before_open() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("must-not-be-created");
        let result = crate::filesystem::Job::open(
            None,
            path.clone().into_os_string(),
            1,
            3,
            false,
            0,
            0o600,
            STAT_SUPPORTED_PROPERTY_MASK + 1,
            STAT_HEADER_LEN as u32,
        );
        assert_eq!(result.unwrap_err(), AsyncHostError::Inval);
        assert!(!path.exists());

        // Invalid arguments take precedence over a missing parent directory.
        let result = crate::filesystem::Job::open(
            None,
            temp.path().join("missing/file").into_os_string(),
            1,
            3,
            false,
            0,
            0o600,
            STAT_SUPPORTED_PROPERTY_MASK + 1,
            STAT_HEADER_LEN as u32,
        );
        assert_eq!(result.unwrap_err(), AsyncHostError::Inval);
    }

    #[cfg(windows)]
    #[test]
    fn windows_parent_relative_query_resolves_from_directory_handle() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES,
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        };

        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("child"), b"child").unwrap();
        let parent_path: Vec<u16> = temp
            .path()
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let parent = unsafe {
            CreateFileW(
                parent_path.as_ptr(),
                FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(parent, INVALID_HANDLE_VALUE);
        let request = StatRequest::new(STAT_FILE_KIND | STAT_FILE_SIZE, 24).unwrap();

        let parent = std::sync::Arc::new(Resource::new(parent));
        let mut result = None;
        run_statx_job(
            &HostFs::default(),
            &mut Some(parent),
            &mut OsString::from("child"),
            request,
            true,
            &mut result,
        )
        .unwrap();
        let result = result.unwrap();
        assert_eq!(result.scalar(STAT_FILE_KIND), Some(1));
        assert_eq!(result.scalar(STAT_FILE_SIZE), Some(5));
    }

    #[cfg(windows)]
    #[test]
    fn windows_fstatx_reports_socket_kind_before_get_file_type_error() {
        assert_eq!(crate::async_sys::internal::event_loop::io::init_wsa(), 0);
        let raw_socket =
            crate::network::os::socket::make_tcp_socket(crate::network::os::IpFamily::V4).unwrap();
        let resource = std::sync::Arc::new(Resource::tcp_socket(raw_socket, 4));
        let request = StatRequest::new(STAT_FILE_KIND, 16).unwrap();
        let mut result = None;

        run_fstatx_job(
            &HostFs::default(),
            &mut Some(std::sync::Arc::clone(&resource)),
            request,
            &mut result,
        )
        .unwrap();

        assert_eq!(result.unwrap().scalar(STAT_FILE_KIND), Some(4));

        let request = StatRequest::new(STAT_FILE_SIZE, 16).unwrap();
        let mut result = None;
        run_fstatx_job(
            &HostFs::default(),
            &mut Some(std::sync::Arc::clone(&resource)),
            request,
            &mut result,
        )
        .unwrap();
        assert_eq!(&result.unwrap().as_bytes()[4..8], &0_u32.to_le_bytes());

        drop(resource);
        assert_eq!(crate::async_sys::internal::event_loop::io::cleanup_wsa(), 0);
    }
}
