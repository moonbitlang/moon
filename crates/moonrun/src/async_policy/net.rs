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

use std::ffi::OsStr;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::{Arc, Mutex};

use anyhow::Context;

use crate::async_host::{AsyncHostError, AsyncHostResult};

use super::config::NetConfig;

#[derive(Clone, Debug)]
pub(super) struct NetPolicy {
    dns: Vec<DnsPattern>,
    connect: Vec<SocketRule>,
    bind: Vec<SocketRule>,
    resolved_connect: Arc<Mutex<Vec<SocketRule>>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NetOperation {
    Connect,
    Bind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DnsPattern {
    Any,
    Exact(String),
    Subdomain(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SocketRule {
    host: SocketHostRule,
    port: SocketPortRule,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SocketHostRule {
    Any,
    Ip(IpAddr),
    Name(DnsPattern),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SocketPortRule {
    Any,
    Exact(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SocketAddr {
    ip: IpAddr,
    port: u16,
}

impl NetPolicy {
    pub(super) fn from_config(config: NetConfig) -> anyhow::Result<Self> {
        let dns = config
            .dns
            .into_iter()
            .map(|pattern| DnsPattern::parse(&pattern))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let connect = config
            .connect
            .into_iter()
            .map(|rule| SocketRule::parse(&rule))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let bind = config
            .bind
            .into_iter()
            .map(|rule| SocketRule::parse(&rule))
            .collect::<anyhow::Result<Vec<_>>>()?;
        for rule in &bind {
            if rule.requires_dns() {
                anyhow::bail!("bind policy rules must use an IP address or *");
            }
        }

        Ok(Self {
            dns,
            connect,
            bind,
            resolved_connect: Arc::default(),
        })
    }

    pub(super) fn resolve_dns(&self, host: &OsStr) -> AsyncHostResult<()> {
        let target = quote_os_str(host);
        let host = host.to_string_lossy();
        let host = normalize_dns_name(&host);
        if self.dns.iter().any(|pattern| pattern.matches(&host))
            || self.connect.iter().any(|rule| rule.allows_lookup(&host))
        {
            Ok(())
        } else {
            sandbox_denied("DNS lookup", &target)
        }
    }

    pub(super) fn register_dns_result(
        &self,
        host: &OsStr,
        addrs: &[Box<[u8]>],
    ) -> AsyncHostResult<()> {
        let host = host.to_string_lossy();
        let host = normalize_dns_name(&host);
        let rules = self
            .connect
            .iter()
            .filter(|rule| rule.allows_resolved_name(&host))
            .collect::<Vec<_>>();
        if rules.is_empty() {
            return Ok(());
        }

        let mut resolved_connect = self.resolved_connect.lock().unwrap();
        for addr in addrs {
            let addr = parse_socket_addr(addr)?;
            for rule in &rules {
                resolved_connect.push(SocketRule {
                    host: SocketHostRule::Ip(addr.ip),
                    port: rule.port,
                });
            }
        }
        Ok(())
    }

    pub(super) fn allows_socket(
        &self,
        operation: NetOperation,
        addr: &[u8],
    ) -> AsyncHostResult<()> {
        let addr = parse_socket_addr(addr)?;
        let target = quote_str(&addr.describe());
        let rules = match operation {
            NetOperation::Connect => &self.connect,
            NetOperation::Bind => &self.bind,
        };
        let resolved_connect = self.resolved_connect.lock().unwrap();
        if rules.iter().any(|rule| rule.matches(addr))
            || (operation == NetOperation::Connect
                && resolved_connect.iter().any(|rule| rule.matches(addr)))
        {
            Ok(())
        } else {
            sandbox_denied(operation.sandbox_action(), &target)
        }
    }
}

impl NetOperation {
    fn sandbox_action(self) -> &'static str {
        match self {
            Self::Connect => "network connect",
            Self::Bind => "network bind",
        }
    }
}

impl DnsPattern {
    fn parse(pattern: &str) -> anyhow::Result<Self> {
        let pattern = normalize_dns_name(pattern);
        if pattern.is_empty() {
            anyhow::bail!("empty DNS policy pattern");
        }
        if pattern == "*" {
            Ok(Self::Any)
        } else if let Some(suffix) = pattern.strip_prefix("*.") {
            if suffix.is_empty() {
                anyhow::bail!("empty DNS wildcard suffix");
            }
            Ok(Self::Subdomain(suffix.to_owned()))
        } else {
            Ok(Self::Exact(pattern))
        }
    }

    fn matches(&self, host: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(pattern) => host == pattern,
            Self::Subdomain(suffix) => {
                host.len() > suffix.len()
                    && host.ends_with(suffix)
                    && host.as_bytes()[host.len() - suffix.len() - 1] == b'.'
            }
        }
    }
}

impl SocketRule {
    fn parse(rule: &str) -> anyhow::Result<Self> {
        let (host, port) = split_host_port(rule)?;
        let host = if host == "*" {
            SocketHostRule::Any
        } else if let Ok(ip) = host.parse() {
            SocketHostRule::Ip(ip)
        } else {
            SocketHostRule::Name(DnsPattern::parse(host)?)
        };
        let port = if port == "*" {
            SocketPortRule::Any
        } else {
            SocketPortRule::Exact(
                port.parse()
                    .with_context(|| format!("invalid socket policy port {port:?}"))?,
            )
        };

        Ok(Self { host, port })
    }

    fn allows_lookup(&self, host: &str) -> bool {
        match &self.host {
            SocketHostRule::Any => true,
            SocketHostRule::Ip(ip) => host.parse::<IpAddr>().is_ok_and(|host_ip| host_ip == *ip),
            SocketHostRule::Name(pattern) => pattern.matches(host),
        }
    }

    fn allows_resolved_name(&self, host: &str) -> bool {
        match &self.host {
            SocketHostRule::Name(pattern) => pattern.matches(host),
            SocketHostRule::Any | SocketHostRule::Ip(_) => false,
        }
    }

    fn requires_dns(&self) -> bool {
        matches!(self.host, SocketHostRule::Name(_))
    }

    fn matches(&self, addr: SocketAddr) -> bool {
        (match &self.host {
            SocketHostRule::Any => true,
            SocketHostRule::Ip(ip) => *ip == addr.ip,
            SocketHostRule::Name(_) => false,
        }) && match self.port {
            SocketPortRule::Any => true,
            SocketPortRule::Exact(port) => port == addr.port,
        }
    }
}

impl SocketAddr {
    fn describe(self) -> String {
        match self.ip {
            IpAddr::V4(ip) => format!("{ip}:{}", self.port),
            IpAddr::V6(ip) => format!("[{ip}]:{}", self.port),
        }
    }
}

fn split_host_port(value: &str) -> anyhow::Result<(&str, &str)> {
    if let Some(rest) = value.strip_prefix('[') {
        let Some((host, port)) = rest.split_once("]:") else {
            anyhow::bail!("IPv6 socket policy rules must use [addr]:port syntax");
        };
        if host.is_empty() {
            anyhow::bail!("empty socket policy address");
        }
        return Ok((host, port));
    }

    let Some((host, port)) = value.rsplit_once(':') else {
        anyhow::bail!("socket policy rules must include a port");
    };
    if host.is_empty() || host.contains(':') {
        anyhow::bail!("IPv6 socket policy rules must use [addr]:port syntax");
    }
    Ok((host, port))
}

fn normalize_dns_name(name: &str) -> String {
    name.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn sandbox_denied(action: &str, target: &str) -> AsyncHostResult<()> {
    eprintln!("Sandbox policy blocked {action}: {target}");
    Err(AsyncHostError::PermissionDenied)
}

fn quote_os_str(value: &OsStr) -> String {
    quote_str(&value.to_string_lossy())
}

fn quote_str(value: &str) -> String {
    format!("{value:?}")
}

#[cfg(unix)]
fn parse_socket_addr(addr: &[u8]) -> AsyncHostResult<SocketAddr> {
    if addr.len() < std::mem::size_of::<libc::sockaddr>() {
        return Err(AsyncHostError::Fault);
    }
    let family = unsafe { addr.as_ptr().cast::<libc::sockaddr>().read_unaligned() }.sa_family;
    match i32::from(family) {
        libc::AF_INET => {
            if addr.len() < std::mem::size_of::<libc::sockaddr_in>() {
                return Err(AsyncHostError::Fault);
            }
            let addr = unsafe { addr.as_ptr().cast::<libc::sockaddr_in>().read_unaligned() };
            Ok(SocketAddr {
                ip: IpAddr::V4(Ipv4Addr::from(u32::from_be(addr.sin_addr.s_addr))),
                port: u16::from_be(addr.sin_port),
            })
        }
        libc::AF_INET6 => {
            if addr.len() < std::mem::size_of::<libc::sockaddr_in6>() {
                return Err(AsyncHostError::Fault);
            }
            let addr = unsafe { addr.as_ptr().cast::<libc::sockaddr_in6>().read_unaligned() };
            Ok(SocketAddr {
                ip: IpAddr::V6(Ipv6Addr::from(addr.sin6_addr.s6_addr)),
                port: u16::from_be(addr.sin6_port),
            })
        }
        _ => Err(AsyncHostError::Inval),
    }
}

#[cfg(windows)]
fn parse_socket_addr(addr: &[u8]) -> AsyncHostResult<SocketAddr> {
    use windows_sys::Win32::Networking::WinSock as ws;

    if addr.len() < std::mem::size_of::<ws::SOCKADDR>() {
        return Err(AsyncHostError::Fault);
    }
    let family = unsafe { addr.as_ptr().cast::<ws::SOCKADDR>().read_unaligned() }.sa_family;
    match family {
        ws::AF_INET => {
            if addr.len() < std::mem::size_of::<ws::SOCKADDR_IN>() {
                return Err(AsyncHostError::Fault);
            }
            let addr = unsafe { addr.as_ptr().cast::<ws::SOCKADDR_IN>().read_unaligned() };
            Ok(SocketAddr {
                ip: IpAddr::V4(Ipv4Addr::from(u32::from_be(unsafe {
                    addr.sin_addr.S_un.S_addr
                }))),
                port: u16::from_be(addr.sin_port),
            })
        }
        ws::AF_INET6 => {
            if addr.len() < std::mem::size_of::<ws::SOCKADDR_IN6>() {
                return Err(AsyncHostError::Fault);
            }
            let addr = unsafe { addr.as_ptr().cast::<ws::SOCKADDR_IN6>().read_unaligned() };
            Ok(SocketAddr {
                ip: IpAddr::V6(Ipv6Addr::from(unsafe { addr.sin6_addr.u.Byte })),
                port: u16::from_be(addr.sin6_port),
            })
        }
        _ => Err(AsyncHostError::Inval),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dns_wildcard_matches_only_subdomains() {
        let pattern = DnsPattern::parse("*.example.com").unwrap();

        assert!(pattern.matches("api.example.com"));
        assert!(!pattern.matches("example.com"));
        assert!(!pattern.matches("notexample.com"));
    }

    #[test]
    fn socket_rules_match_ip_and_port() {
        let rule = SocketRule::parse("127.0.0.1:*").unwrap();

        assert!(rule.matches(SocketAddr {
            ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 8080,
        }));
        assert!(!rule.matches(SocketAddr {
            ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)),
            port: 8080,
        }));
    }

    #[test]
    fn hostname_connect_rules_allow_lookup_and_resolved_connects() {
        let policy = NetPolicy::from_config(NetConfig {
            dns: Vec::new(),
            connect: vec!["api.deepseek.com:443".to_owned()],
            bind: Vec::new(),
        })
        .unwrap();
        let resolved_addr = ipv4_addr(Ipv4Addr::LOCALHOST, 0);
        let allowed_addr = ipv4_addr(Ipv4Addr::LOCALHOST, 443);
        let denied_addr = ipv4_addr(Ipv4Addr::LOCALHOST, 80);

        policy.resolve_dns(OsStr::new("API.DEEPSEEK.COM.")).unwrap();
        policy
            .register_dns_result(OsStr::new("api.deepseek.com"), &[resolved_addr])
            .unwrap();

        policy
            .allows_socket(NetOperation::Connect, &allowed_addr)
            .unwrap();
        let error = policy
            .allows_socket(NetOperation::Connect, &denied_addr)
            .unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn dns_rules_do_not_allow_socket_connects() {
        let policy = NetPolicy::from_config(NetConfig {
            dns: vec!["api.deepseek.com".to_owned()],
            connect: Vec::new(),
            bind: Vec::new(),
        })
        .unwrap();
        let addr = ipv4_addr(Ipv4Addr::LOCALHOST, 443);

        policy.resolve_dns(OsStr::new("api.deepseek.com")).unwrap();
        let error = policy
            .allows_socket(NetOperation::Connect, &addr)
            .unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn subdomain_connect_rules_do_not_match_parent_domain() {
        let policy = NetPolicy::from_config(NetConfig {
            dns: Vec::new(),
            connect: vec!["*.example.com:443".to_owned()],
            bind: Vec::new(),
        })
        .unwrap();
        let resolved_addr = ipv4_addr(Ipv4Addr::LOCALHOST, 0);
        let allowed_addr = ipv4_addr(Ipv4Addr::LOCALHOST, 443);

        policy.resolve_dns(OsStr::new("api.example.com")).unwrap();
        let error = policy.resolve_dns(OsStr::new("example.com")).unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);

        policy
            .register_dns_result(OsStr::new("api.example.com"), &[resolved_addr])
            .unwrap();
        policy
            .allows_socket(NetOperation::Connect, &allowed_addr)
            .unwrap();
    }

    #[test]
    fn wildcard_connect_rule_allows_any_lookup_and_connect() {
        let policy = NetPolicy::from_config(NetConfig {
            dns: Vec::new(),
            connect: vec!["*:*".to_owned()],
            bind: Vec::new(),
        })
        .unwrap();
        let addr = ipv4_addr(Ipv4Addr::LOCALHOST, 443);

        policy.resolve_dns(OsStr::new("api.deepseek.com")).unwrap();
        policy.allows_socket(NetOperation::Connect, &addr).unwrap();
    }

    #[test]
    fn bind_rules_do_not_allow_connects() {
        let policy = NetPolicy::from_config(NetConfig {
            dns: Vec::new(),
            connect: Vec::new(),
            bind: vec!["127.0.0.1:8080".to_owned()],
        })
        .unwrap();
        let addr = ipv4_addr(Ipv4Addr::LOCALHOST, 8080);

        policy.allows_socket(NetOperation::Bind, &addr).unwrap();
        let error = policy
            .allows_socket(NetOperation::Connect, &addr)
            .unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn ipv6_socket_rules_require_brackets() {
        let rule = SocketRule::parse("[::1]:443").unwrap();

        assert!(rule.matches(SocketAddr {
            ip: IpAddr::V6(Ipv6Addr::LOCALHOST),
            port: 443,
        }));
        assert!(
            SocketRule::parse("::1:443")
                .unwrap_err()
                .to_string()
                .contains("IPv6 socket policy rules must use [addr]:port syntax")
        );
    }

    #[test]
    fn bind_rules_reject_hostnames() {
        let error = NetPolicy::from_config(NetConfig {
            dns: Vec::new(),
            connect: Vec::new(),
            bind: vec!["localhost:8080".to_owned()],
        })
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("bind policy rules must use an IP address or *")
        );
    }

    fn ipv4_addr(ip: Ipv4Addr, port: u16) -> Box<[u8]> {
        let mut addr = vec![0; crate::async_sys::socket::ipv4_addr_size() as usize];
        crate::async_sys::socket::init_ip_addr(&mut addr, u32::from(ip) as i32, i32::from(port))
            .unwrap();
        addr.into_boxed_slice()
    }
}
