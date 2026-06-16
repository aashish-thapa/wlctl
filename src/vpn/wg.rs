//! Minimal WireGuard `.conf` parser — enough to import the single-peer tunnels
//! that hosted providers (Proton, Mullvad, …) hand out. Turns the INI-ish text
//! into an [`nm::WgConfig`] that the NM layer maps onto a `wireguard` profile.

use std::net::IpAddr;
use std::str::FromStr;

use anyhow::{Result, anyhow, bail};

use crate::nm::{WgConfig, WgPeerConfig};

/// Parses WireGuard config text into a [`WgConfig`]. Section and key names are
/// matched case-insensitively; comments (`#`/`;`) and blank lines are ignored.
/// Exactly one `[Interface]` and one `[Peer]` are expected.
pub fn parse(text: &str) -> Result<WgConfig> {
    let mut section: Option<String> = None;

    let mut private_key: Option<String> = None;
    let mut addresses: Vec<(IpAddr, u8)> = Vec::new();
    let mut dns: Vec<IpAddr> = Vec::new();

    let mut public_key: Option<String> = None;
    let mut endpoint: Option<String> = None;
    let mut allowed_ips: Vec<String> = Vec::new();
    let mut preshared_key: Option<String> = None;
    let mut keepalive: Option<u32> = None;
    let mut interface_count = 0;
    let mut peer_count = 0;

    for raw in text.lines() {
        // Strip inline comments and surrounding whitespace.
        let line = raw.split(['#', ';']).next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }

        if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let name = name.trim().to_ascii_lowercase();
            match name.as_str() {
                "interface" => {
                    interface_count += 1;
                    if interface_count > 1 {
                        bail!("multiple [Interface] sections are not supported");
                    }
                }
                "peer" => {
                    peer_count += 1;
                    if peer_count > 1 {
                        bail!("multiple [Peer] sections are not supported");
                    }
                }
                _ => {}
            }
            section = Some(name);
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().to_string();
        if value.is_empty() {
            continue;
        }

        match (section.as_deref(), key.as_str()) {
            (Some("interface"), "privatekey") => private_key = Some(value),
            (Some("interface"), "address") => {
                for item in value.split(',') {
                    addresses.push(parse_cidr(item.trim())?);
                }
            }
            (Some("interface"), "dns") => {
                // DNS may also list search domains; keep only IP entries.
                for item in value.split(',') {
                    if let Ok(ip) = IpAddr::from_str(item.trim()) {
                        dns.push(ip);
                    }
                }
            }
            (Some("peer"), "publickey") => public_key = Some(value),
            (Some("peer"), "endpoint") => endpoint = Some(value),
            (Some("peer"), "allowedips") => {
                for item in value.split(',') {
                    let item = item.trim();
                    if item.is_empty() {
                        continue;
                    }
                    // Validate the CIDR; keep the original string for NM.
                    parse_cidr(item)?;
                    allowed_ips.push(item.to_string());
                }
            }
            (Some("peer"), "presharedkey") => preshared_key = Some(value),
            (Some("peer"), "persistentkeepalive") => {
                keepalive = Some(
                    value
                        .parse()
                        .map_err(|_| anyhow!("invalid PersistentKeepalive '{value}'"))?,
                );
            }
            _ => {}
        }
    }

    let private_key = private_key.ok_or_else(|| anyhow!("[Interface] is missing PrivateKey"))?;
    if addresses.is_empty() {
        bail!("[Interface] is missing Address");
    }
    let public_key = public_key.ok_or_else(|| anyhow!("[Peer] is missing PublicKey"))?;
    if endpoint.is_none() {
        bail!("[Peer] is missing Endpoint");
    }
    if allowed_ips.is_empty() {
        // A tunnel that routes nothing is almost certainly a mistake.
        bail!("[Peer] is missing AllowedIPs");
    }

    Ok(WgConfig {
        private_key,
        addresses,
        dns,
        peer: WgPeerConfig {
            public_key,
            endpoint,
            allowed_ips,
            preshared_key,
            persistent_keepalive: keepalive,
        },
    })
}

/// Parses `addr/prefix`, defaulting the prefix to the address family's full
/// width (`/32` for IPv4, `/128` for IPv6) when omitted.
fn parse_cidr(s: &str) -> Result<(IpAddr, u8)> {
    let (addr_part, prefix_part) = match s.split_once('/') {
        Some((a, p)) => (a, Some(p)),
        None => (s, None),
    };
    let ip =
        IpAddr::from_str(addr_part.trim()).map_err(|_| anyhow!("invalid address '{addr_part}'"))?;
    let max = if ip.is_ipv4() { 32 } else { 128 };
    let prefix = match prefix_part {
        Some(p) => {
            let prefix: u8 = p
                .trim()
                .parse()
                .map_err(|_| anyhow!("invalid prefix in '{s}'"))?;
            if prefix > max {
                bail!("prefix /{prefix} out of range in '{s}'");
            }
            prefix
        }
        None => max,
    };
    Ok((ip, prefix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    const PROTON: &str = "\
[Interface]
# Key for proton
PrivateKey = aPrivateKey=
Address = 10.2.0.2/32
DNS = 10.2.0.1

[Peer]
# NL-FREE#247
PublicKey = aPublicKey=
AllowedIPs = 0.0.0.0/0
Endpoint = 1.2.3.4:51820
";

    #[test]
    fn parses_a_typical_single_peer_conf() {
        let cfg = parse(PROTON).unwrap();
        assert_eq!(cfg.private_key, "aPrivateKey=");
        assert_eq!(
            cfg.addresses,
            vec![(IpAddr::V4(Ipv4Addr::new(10, 2, 0, 2)), 32)]
        );
        assert_eq!(cfg.dns, vec![IpAddr::V4(Ipv4Addr::new(10, 2, 0, 1))]);
        assert_eq!(cfg.peer.public_key, "aPublicKey=");
        assert_eq!(cfg.peer.endpoint.as_deref(), Some("1.2.3.4:51820"));
        assert_eq!(cfg.peer.allowed_ips, vec!["0.0.0.0/0"]);
    }

    #[test]
    fn defaults_prefix_by_family_and_handles_lists() {
        let cfg = parse(
            "[Interface]
PrivateKey = k=
Address = 10.0.0.2, fd00::2
[Peer]
PublicKey = p=
Endpoint = h:1
AllowedIPs = 0.0.0.0/0, ::/0
",
        )
        .unwrap();
        assert_eq!(
            cfg.addresses,
            vec![
                (IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)), 32),
                (IpAddr::V6(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 2)), 128),
            ]
        );
        assert_eq!(cfg.peer.allowed_ips, vec!["0.0.0.0/0", "::/0"]);
    }

    #[test]
    fn rejects_missing_required_fields() {
        assert!(parse("[Interface]\nAddress = 10.0.0.2/32\n[Peer]\nPublicKey=p=\nEndpoint=h:1\nAllowedIPs=0.0.0.0/0\n").is_err());
        assert!(parse("[Interface]\nPrivateKey=k=\nAddress=10.0.0.2/32\n[Peer]\nEndpoint=h:1\nAllowedIPs=0.0.0.0/0\n").is_err());
    }

    #[test]
    fn rejects_multiple_peers() {
        let two = "[Interface]\nPrivateKey=k=\nAddress=10.0.0.2/32\n[Peer]\nPublicKey=p=\nEndpoint=h:1\nAllowedIPs=0.0.0.0/0\n[Peer]\nPublicKey=q=\nEndpoint=h:2\nAllowedIPs=0.0.0.0/0\n";
        assert!(parse(two).is_err());
    }

    #[test]
    fn rejects_multiple_interfaces() {
        let two = "[Interface]\nPrivateKey=k=\nAddress=10.0.0.2/32\n[Interface]\nPrivateKey=k2=\nAddress=10.0.0.3/32\n[Peer]\nPublicKey=p=\nEndpoint=h:1\nAllowedIPs=0.0.0.0/0\n";
        assert!(parse(two).is_err());
    }

    #[test]
    fn rejects_malformed_allowed_ips_but_skips_empty_entries() {
        // Trailing comma leaves an empty entry, which is skipped (not an error).
        let cfg = parse(
            "[Interface]\nPrivateKey=k=\nAddress=10.0.0.2/32\n[Peer]\nPublicKey=p=\nEndpoint=h:1\nAllowedIPs=0.0.0.0/0, ::/0,\n",
        )
        .unwrap();
        assert_eq!(cfg.peer.allowed_ips, vec!["0.0.0.0/0", "::/0"]);

        // A non-CIDR entry is rejected.
        assert!(parse(
            "[Interface]\nPrivateKey=k=\nAddress=10.0.0.2/32\n[Peer]\nPublicKey=p=\nEndpoint=h:1\nAllowedIPs=not-an-ip\n"
        )
        .is_err());
    }

    #[test]
    fn rejects_non_numeric_keepalive() {
        assert!(parse(
            "[Interface]\nPrivateKey=k=\nAddress=10.0.0.2/32\n[Peer]\nPublicKey=p=\nEndpoint=h:1\nAllowedIPs=0.0.0.0/0\nPersistentKeepalive=soon\n"
        )
        .is_err());
    }
}
