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

use std::ffi::OsStr;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use anyhow::Context;

use crate::async_host::AsyncHostResult;

use super::{config::NetConfig, sandbox_denied, transfer};

#[derive(Clone, Debug)]
pub(crate) struct NetPolicy {
    rule_groups: Vec<Arc<NetRules>>,
}

/// A grant is scoped to one immutable group in this Runtime. DNS results
/// are not inherited, and cannot satisfy another group's checks.
#[derive(Clone, Debug)]
pub(crate) struct ResolvedConnectRule {
    group: usize,
    rule: SocketRule,
}

impl NetPolicy {
    pub(super) fn from_rule_groups(rule_groups: impl IntoIterator<Item = Arc<NetRules>>) -> Self {
        Self {
            rule_groups: rule_groups.into_iter().collect(),
        }
    }

    #[cfg(test)]
    pub(super) fn from_config(config: NetConfig) -> anyhow::Result<Self> {
        Self::from_configs([config])
    }

    #[cfg(test)]
    pub(super) fn from_configs(
        configs: impl IntoIterator<Item = NetConfig>,
    ) -> anyhow::Result<Self> {
        let rule_groups = configs
            .into_iter()
            .map(|config| NetRules::from_config(config).map(Arc::new))
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Self::from_rule_groups(rule_groups))
    }

    pub(crate) fn check_dns(&self, host: &OsStr) -> AsyncHostResult<()> {
        let target = quote_os_str(host);
        let host = normalize_dns_name(&host.to_string_lossy());
        if self.rule_groups.iter().all(|group| {
            group.dns.iter().any(|pattern| pattern.matches(&host))
                || group.connect.iter().any(|rule| rule.allows_lookup(&host))
        }) {
            Ok(())
        } else {
            sandbox_denied("DNS lookup", Some(&target))
        }
    }

    /// Addresses are consumed only when a hostname rule can grant connect authority.
    pub(crate) fn resolved_connect_rules(
        &self,
        host: &OsStr,
        ips: impl IntoIterator<Item = AsyncHostResult<IpAddr>>,
    ) -> AsyncHostResult<Vec<ResolvedConnectRule>> {
        let host = normalize_dns_name(&host.to_string_lossy());
        let rules = self
            .rule_groups
            .iter()
            .enumerate()
            .flat_map(|(index, group)| {
                group
                    .connect
                    .iter()
                    .filter(|rule| rule.allows_resolved_name(&host))
                    .map(move |rule| (index, rule))
            })
            .collect::<Vec<_>>();
        if rules.is_empty() {
            return Ok(Vec::new());
        }
        let mut grants = Vec::new();
        for ip in ips {
            let ip = ip?;
            grants.extend(rules.iter().map(|(index, rule)| ResolvedConnectRule {
                group: *index,
                rule: SocketRule {
                    host: SocketHostRule::Ip(ip),
                    port: rule.port,
                },
            }));
        }
        Ok(grants)
    }

    pub(crate) fn check_socket(
        &self,
        operation: NetOperation,
        addr: SocketAddr,
        resolved_connect: &[ResolvedConnectRule],
    ) -> AsyncHostResult<()> {
        if self.rule_groups.iter().enumerate().all(|(index, group)| {
            let rules = match operation {
                NetOperation::Connect => &group.connect,
                NetOperation::Bind => &group.bind,
            };
            rules.iter().any(|rule| rule.matches(addr))
                || (operation == NetOperation::Connect
                    && resolved_connect
                        .iter()
                        .any(|grant| grant.group == index && grant.rule.matches(addr)))
        }) {
            Ok(())
        } else {
            sandbox_denied(
                operation.sandbox_action(),
                Some(&quote_str(&addr.to_string())),
            )
        }
    }
}
#[derive(Clone, Debug)]
pub(super) struct NetRules {
    dns: Vec<DnsPattern>,
    connect: Vec<SocketRule>,
    bind: Vec<SocketRule>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NetOperation {
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
pub(crate) struct SocketRule {
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

impl NetRules {
    pub(super) fn to_transfer(&self) -> transfer::Network {
        transfer::Network {
            dns: self.dns.iter().map(DnsPattern::to_transfer).collect(),
            connect: self.connect.iter().map(SocketRule::to_transfer).collect(),
            bind: self.bind.iter().map(SocketRule::to_transfer).collect(),
        }
    }

    pub(super) fn from_transfer(value: transfer::Network) -> anyhow::Result<Self> {
        let policy = Self {
            dns: value
                .dns
                .into_iter()
                .map(DnsPattern::from_transfer)
                .collect::<anyhow::Result<_>>()?,
            connect: value
                .connect
                .into_iter()
                .map(SocketRule::from_transfer)
                .collect::<anyhow::Result<_>>()?,
            bind: value
                .bind
                .into_iter()
                .map(SocketRule::from_transfer)
                .collect::<anyhow::Result<_>>()?,
        };
        anyhow::ensure!(
            !policy.bind.iter().any(SocketRule::requires_dns),
            "bind rules cannot use DNS names"
        );
        Ok(policy)
    }

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

        Ok(Self { dns, connect, bind })
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
    fn to_transfer(&self) -> transfer::DnsPattern {
        match self {
            Self::Any => transfer::DnsPattern::Any,
            Self::Exact(name) => transfer::DnsPattern::Exact(name.clone()),
            Self::Subdomain(suffix) => transfer::DnsPattern::Subdomain(suffix.clone()),
        }
    }

    fn from_transfer(value: transfer::DnsPattern) -> anyhow::Result<Self> {
        let pattern = match value {
            transfer::DnsPattern::Any => Self::Any,
            transfer::DnsPattern::Exact(name) => {
                anyhow::ensure!(
                    name != "*" && !name.starts_with("*."),
                    "invalid exact DNS rule"
                );
                Self::Exact(name)
            }
            transfer::DnsPattern::Subdomain(suffix) => Self::Subdomain(suffix),
        };
        if let Self::Exact(name) | Self::Subdomain(name) = &pattern {
            anyhow::ensure!(
                !name.is_empty() && normalize_dns_name(name) == *name,
                "inherited DNS names must be normalized"
            );
        }
        Ok(pattern)
    }

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
    fn to_transfer(&self) -> transfer::SocketRule {
        transfer::SocketRule {
            host: match &self.host {
                SocketHostRule::Any => transfer::SocketHost::Any,
                SocketHostRule::Ip(ip) => transfer::SocketHost::Ip(*ip),
                SocketHostRule::Name(pattern) => transfer::SocketHost::Name(pattern.to_transfer()),
            },
            port: match self.port {
                SocketPortRule::Any => transfer::SocketPort::Any,
                SocketPortRule::Exact(port) => transfer::SocketPort::Exact(port),
            },
        }
    }

    fn from_transfer(value: transfer::SocketRule) -> anyhow::Result<Self> {
        Ok(Self {
            host: match value.host {
                transfer::SocketHost::Any => SocketHostRule::Any,
                transfer::SocketHost::Ip(ip) => SocketHostRule::Ip(ip),
                transfer::SocketHost::Name(pattern) => {
                    SocketHostRule::Name(DnsPattern::from_transfer(pattern)?)
                }
            },
            port: match value.port {
                transfer::SocketPort::Any => SocketPortRule::Any,
                transfer::SocketPort::Exact(port) => SocketPortRule::Exact(port),
            },
        })
    }

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
            SocketHostRule::Ip(ip) => *ip == addr.ip(),
            SocketHostRule::Name(_) => false,
        }) && match self.port {
            SocketPortRule::Any => true,
            SocketPortRule::Exact(port) => port == addr.port(),
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

fn quote_os_str(value: &OsStr) -> String {
    quote_str(&value.to_string_lossy())
}

fn quote_str(value: &str) -> String {
    format!("{value:?}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_host::AsyncHostError;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn dns_addresses_are_consumed_only_for_matching_hostname_rules() {
        let policy = NetPolicy::from_config(NetConfig {
            connect: vec!["api.example.com:443".into()],
            ..Default::default()
        })
        .unwrap();
        let unused = std::iter::from_fn(|| panic!("unmatched DNS results must not be read"));
        assert!(
            policy
                .resolved_connect_rules(OsStr::new("other.example.com"), unused)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            policy
                .resolved_connect_rules(
                    OsStr::new("api.example.com"),
                    [Err(AsyncHostError::Fault)],
                )
                .unwrap_err(),
            AsyncHostError::Fault
        );
    }

    #[test]
    fn composed_dns_grants_cannot_satisfy_another_group() {
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, 443));
        for reverse in [false, true] {
            let mut configs = vec![
                NetConfig {
                    dns: vec!["*".into()],
                    connect: vec!["*.example.com:443".into()],
                    bind: vec!["127.0.0.1:*".into()],
                },
                NetConfig {
                    dns: vec!["api.example.com".into()],
                    connect: vec!["127.0.0.1:443".into()],
                    bind: vec!["*:443".into()],
                },
            ];
            if reverse {
                configs.reverse();
            }
            let policy = NetPolicy::from_configs(configs).unwrap();
            policy.check_dns(OsStr::new("api.example.com")).unwrap();
            assert_eq!(
                policy.check_dns(OsStr::new("other.example.com")),
                Err(AsyncHostError::PermissionDenied)
            );
            let grants = policy
                .resolved_connect_rules(OsStr::new("api.example.com"), [Ok(address.ip())])
                .unwrap();
            policy
                .check_socket(NetOperation::Connect, address, &grants)
                .unwrap();
            policy
                .check_socket(NetOperation::Bind, address, &[])
                .unwrap();
            let wrong_port = SocketAddr::from((Ipv4Addr::LOCALHOST, 80));
            assert_eq!(
                policy.check_socket(NetOperation::Bind, wrong_port, &[]),
                Err(AsyncHostError::PermissionDenied)
            );
            assert_eq!(
                policy.check_socket(NetOperation::Connect, wrong_port, &grants),
                Err(AsyncHostError::PermissionDenied)
            );
            let other_address = SocketAddr::from((Ipv4Addr::new(127, 0, 0, 2), 443));
            let grants = policy
                .resolved_connect_rules(OsStr::new("api.example.com"), [Ok(other_address.ip())])
                .unwrap();
            assert_eq!(
                policy.check_socket(NetOperation::Connect, other_address, &grants),
                Err(AsyncHostError::PermissionDenied)
            );
        }
        let policy = NetPolicy::from_configs([
            NetConfig {
                dns: vec!["*".into()],
                connect: vec!["a.example.com:443".into()],
                ..Default::default()
            },
            NetConfig {
                dns: vec!["*".into()],
                connect: vec!["b.example.com:443".into()],
                ..Default::default()
            },
        ])
        .unwrap();
        let mut grants = policy
            .resolved_connect_rules(OsStr::new("a.example.com"), [Ok(address.ip())])
            .unwrap();
        assert_eq!(
            policy.check_socket(NetOperation::Connect, address, &grants),
            Err(AsyncHostError::PermissionDenied)
        );
        grants.extend(
            policy
                .resolved_connect_rules(OsStr::new("b.example.com"), [Ok(address.ip())])
                .unwrap(),
        );
        policy
            .check_socket(NetOperation::Connect, address, &grants)
            .unwrap();
    }

    #[test]
    fn transfer_preserves_dns_provenance_for_ip_and_wildcard_spelling() {
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, 443));
        for (rule, lookup) in [
            ("*.:443", "127.0.0.1"),
            ("127.0.0.1.:443", "127.0.0.1"),
            ("[::1.]:443", "::1"),
            ("[[name]:443", "[name"),
            ("API.Example.COM.:443", "api.example.com"),
        ] {
            let parent = NetRules::from_config(NetConfig {
                connect: vec![rule.into()],
                ..Default::default()
            })
            .unwrap();
            let encoded = serde_json::to_vec(&parent.to_transfer()).unwrap();
            let child = NetRules::from_transfer(serde_json::from_slice(&encoded).unwrap()).unwrap();
            for group in [parent, child] {
                let policy = NetPolicy::from_rule_groups([Arc::new(group)]);
                assert_eq!(
                    policy.check_socket(NetOperation::Connect, address, &[]),
                    Err(AsyncHostError::PermissionDenied),
                    "{rule}"
                );
                let grants = policy
                    .resolved_connect_rules(lookup.as_ref(), [Ok(address.ip())])
                    .unwrap();
                policy
                    .check_socket(NetOperation::Connect, address, &grants)
                    .unwrap();
            }
        }
    }

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

        assert!(rule.matches(SocketAddr::from((Ipv4Addr::LOCALHOST, 8080))));
        assert!(!rule.matches(SocketAddr::from((Ipv4Addr::new(127, 0, 0, 2), 8080))));
    }

    #[test]
    fn hostname_connect_rules_allow_lookup_and_resolved_connects() {
        let policy = NetPolicy::from_config(NetConfig {
            dns: Vec::new(),
            connect: vec!["api.deepseek.com:443".to_owned()],
            bind: Vec::new(),
        })
        .unwrap();
        let resolved_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
        let allowed_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 443));
        let denied_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 80));

        policy.check_dns(OsStr::new("API.DEEPSEEK.COM.")).unwrap();
        let resolved_connect = policy
            .resolved_connect_rules(OsStr::new("api.deepseek.com"), [Ok(resolved_addr.ip())])
            .unwrap();

        policy
            .check_socket(NetOperation::Connect, allowed_addr, &resolved_connect)
            .unwrap();
        let error = policy
            .check_socket(NetOperation::Connect, denied_addr, &resolved_connect)
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
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 443));

        policy.check_dns(OsStr::new("api.deepseek.com")).unwrap();
        let error = policy
            .check_socket(NetOperation::Connect, addr, &[])
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
        let resolved_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
        let allowed_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 443));

        policy.check_dns(OsStr::new("api.example.com")).unwrap();
        let error = policy.check_dns(OsStr::new("example.com")).unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);

        let resolved_connect = policy
            .resolved_connect_rules(OsStr::new("api.example.com"), [Ok(resolved_addr.ip())])
            .unwrap();
        policy
            .check_socket(NetOperation::Connect, allowed_addr, &resolved_connect)
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
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 443));

        policy.check_dns(OsStr::new("api.deepseek.com")).unwrap();
        policy
            .check_socket(NetOperation::Connect, addr, &[])
            .unwrap();
    }

    #[test]
    fn bind_rules_do_not_allow_connects() {
        let policy = NetPolicy::from_config(NetConfig {
            dns: Vec::new(),
            connect: Vec::new(),
            bind: vec!["127.0.0.1:8080".to_owned()],
        })
        .unwrap();
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 8080));

        policy.check_socket(NetOperation::Bind, addr, &[]).unwrap();
        let error = policy
            .check_socket(NetOperation::Connect, addr, &[])
            .unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn ipv6_socket_rules_require_brackets() {
        let rule = SocketRule::parse("[::1]:443").unwrap();

        assert!(rule.matches(SocketAddr::from((Ipv6Addr::LOCALHOST, 443))));
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
}
