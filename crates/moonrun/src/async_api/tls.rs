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

use std::path::PathBuf;

use crate::async_host::{
    AsyncHostError, AsyncHostResult, GuestMemory, read_u16,
    tls::{TlsFileType, TlsTrust},
};

use super::context::ImportContext;
use super::provenance::ported_imports;

ported_imports! {
#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/client_new")]
pub(super) fn client_new(
    context: &mut ImportContext<'_, '_>,
    host: i32,
    host_len: i32,
    sni: i32,
    trust_abi: i32,
    custom_roots: i32,
    custom_roots_len: i32,
) -> AsyncHostResult<u64> {
    context.with_host_and_memory_mut(|host_state, memory| {
        let hostname = read_guest_string(memory, host, host_len)?;
        let trust = TlsTrust::from_abi(trust_abi)?;
        let custom_roots = if trust == TlsTrust::CustomRoot {
            memory.read_exact(custom_roots, custom_roots_len)?
        } else {
            &[]
        };
        Ok(host_state.tls_client_new(&hostname, sni != 0, trust, custom_roots))
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/server_new")]
pub(super) fn server_new(
    context: &mut ImportContext<'_, '_>,
    private_key_file: i32,
    private_key_file_len: i32,
    private_key_type_abi: i32,
    certificate_file: i32,
    certificate_file_len: i32,
    certificate_type_abi: i32,
) -> AsyncHostResult<u64> {
    context.with_host_and_memory_mut(|host, memory| {
        let private_key_file = read_guest_os_path(memory, private_key_file, private_key_file_len)?;
        let certificate_file = read_guest_os_path(memory, certificate_file, certificate_file_len)?;
        Ok(host.tls_server_new(
            private_key_file,
            TlsFileType::from_abi(private_key_type_abi)?,
            certificate_file,
            TlsFileType::from_abi(certificate_type_abi)?,
        ))
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/server_pfx_new")]
pub(super) fn server_pfx_new(
    context: &mut ImportContext<'_, '_>,
    pfx_content: i32,
    pfx_content_len: i32,
) -> AsyncHostResult<u64> {
    context.with_host_and_memory_mut(|host, memory| {
        Ok(host.tls_server_pfx_new(
            memory.read_exact(pfx_content, pfx_content_len)?.to_vec(),
        ))
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/free")]
pub(super) fn free(context: &mut ImportContext<'_, '_>, tls: u64) -> AsyncHostResult<()> {
    context.host.tls_free(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/take_error")]
pub(super) fn take_error(context: &mut ImportContext<'_, '_>, tls: u64) -> AsyncHostResult<u64> {
    context.host.tls_take_error(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/error/take_global")]
pub(super) fn take_global_error(context: &mut ImportContext<'_, '_>) -> u64 {
    context.host.tls_take_global_error()
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/read_tls")]
pub(super) fn read_tls(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
    src: i32,
    offset: i32,
    len: i32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host, memory| {
        let src = memory.read_exact(guest_offset(src, offset)?, len)?;
        host.tls_read_tls(tls, src)
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/write_tls")]
pub(super) fn write_tls(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
    dst: i32,
    offset: i32,
    len: i32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host, memory| {
        let dst = memory.read_exact_mut(guest_offset(dst, offset)?, len)?;
        host.tls_write_tls(tls, dst)
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/read_plain")]
pub(super) fn read_plain(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
    dst: i32,
    offset: i32,
    len: i32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host, memory| {
        let dst = memory.read_exact_mut(guest_offset(dst, offset)?, len)?;
        host.tls_read_plain(tls, dst)
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/write_plain")]
pub(super) fn write_plain(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
    src: i32,
    offset: i32,
    len: i32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host, memory| {
        let src = memory.read_exact(guest_offset(src, offset)?, len)?;
        host.tls_write_plain(tls, src)
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/wants_read")]
pub(super) fn wants_read(context: &mut ImportContext<'_, '_>, tls: u64) -> AsyncHostResult<i32> {
    context.host.tls_wants_read(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/wants_write")]
pub(super) fn wants_write(context: &mut ImportContext<'_, '_>, tls: u64) -> AsyncHostResult<i32> {
    context.host.tls_wants_write(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/is_handshaking")]
pub(super) fn is_handshaking(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
) -> AsyncHostResult<i32> {
    context.host.tls_is_handshaking(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/send_close_notify")]
pub(super) fn send_close_notify(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
) -> AsyncHostResult<i32> {
    context.host.tls_send_close_notify(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/peer_certificate_len")]
pub(super) fn peer_certificate_len(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
) -> AsyncHostResult<i32> {
    context.host.tls_peer_certificate_len(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/peer_certificate")]
pub(super) fn peer_certificate(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
    dst: i32,
    offset: i32,
    len: i32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host, memory| {
        let dst = memory.read_exact_mut(guest_offset(dst, offset)?, len)?;
        host.tls_copy_peer_certificate(tls, dst)
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/unique_channel_binding_len")]
pub(super) fn unique_channel_binding_len(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
) -> AsyncHostResult<i32> {
    context.host.tls_unique_channel_binding_len(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/unique_channel_binding")]
pub(super) fn unique_channel_binding(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
    dst: i32,
    offset: i32,
    len: i32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host, memory| {
        let dst = memory.read_exact_mut(guest_offset(dst, offset)?, len)?;
        host.tls_copy_unique_channel_binding(tls, dst)
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/server_endpoint_channel_binding_len")]
pub(super) fn server_endpoint_channel_binding_len(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
) -> AsyncHostResult<i32> {
    context.host.tls_server_endpoint_channel_binding_len(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/server_endpoint_channel_binding")]
pub(super) fn server_endpoint_channel_binding(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
    dst: i32,
    offset: i32,
    len: i32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host, memory| {
        let dst = memory.read_exact_mut(guest_offset(dst, offset)?, len)?;
        host.tls_copy_server_endpoint_channel_binding(tls, dst)
    })
}
}

fn guest_offset(ptr: i32, offset: i32) -> AsyncHostResult<i32> {
    if offset < 0 {
        return Err(AsyncHostError::Fault);
    }
    ptr.checked_add(offset).ok_or(AsyncHostError::Fault)
}

fn read_guest_string(memory: &[u8], ptr: i32, len: i32) -> AsyncHostResult<String> {
    let units = read_u16(memory, ptr, len)?;
    Ok(std::char::decode_utf16(units)
        .map(|unit| unit.unwrap_or(std::char::REPLACEMENT_CHARACTER))
        .collect())
}

fn read_guest_os_path(memory: &[u8], ptr: i32, len: i32) -> AsyncHostResult<PathBuf> {
    let units = read_u16(memory, ptr, len)?;

    #[cfg(unix)]
    {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let path = std::char::decode_utf16(units)
            .map(|unit| unit.unwrap_or(std::char::REPLACEMENT_CHARACTER))
            .collect::<String>();
        Ok(PathBuf::from(OsString::from_vec(path.into_bytes())))
    }

    #[cfg(windows)]
    {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;

        Ok(PathBuf::from(OsString::from_wide(&units)))
    }
}

#[cfg(test)]
pub(super) fn ported_symbols() -> Vec<crate::async_sys::PortedSymbol> {
    PORTED_IMPORTS
        .iter()
        .flat_map(|import| {
            import
                .sources
                .iter()
                .map(move |source| crate::async_sys::PortedSymbol {
                    rust_module: import.rust_module,
                    rust_symbol: import.rust_symbol,
                    native_symbol: import
                        .native_symbol
                        .expect("TLS ABI provenance uses explicit wasm import symbols"),
                    source: source.path,
                })
        })
        .collect()
}
