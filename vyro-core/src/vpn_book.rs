//! VPNBook provider: public no-signup OpenVPN configs and rotating credentials.
//! The provider publishes configs at `/api/openvpn` and its current shared
//! password on the Free OpenVPN page. Nothing is bundled or invented.

use serde::{Deserialize, Serialize};
use std::time::Duration;

const BASE: &str = "https://www.vpnbook.com";
const PAGE: &str = "https://www.vpnbook.com/freevpn/openvpn";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VpnBookServer {
    pub hostname: &'static str,
    pub country: &'static str,
    pub code: &'static str,
}

pub const SERVERS: &[VpnBookServer] = &[
    VpnBookServer { hostname: "us16.vpnbook.com", country: "United States", code: "US" },
    VpnBookServer { hostname: "us178.vpnbook.com", country: "United States", code: "US" },
    VpnBookServer { hostname: "ca149.vpnbook.com", country: "Canada", code: "CA" },
    VpnBookServer { hostname: "ca196.vpnbook.com", country: "Canada", code: "CA" },
    VpnBookServer { hostname: "uk205.vpnbook.com", country: "United Kingdom", code: "GB" },
    VpnBookServer { hostname: "uk68.vpnbook.com", country: "United Kingdom", code: "GB" },
    VpnBookServer { hostname: "de20.vpnbook.com", country: "Germany", code: "DE" },
    VpnBookServer { hostname: "de220.vpnbook.com", country: "Germany", code: "DE" },
    VpnBookServer { hostname: "fr200.vpnbook.com", country: "France", code: "FR" },
    VpnBookServer { hostname: "fr231.vpnbook.com", country: "France", code: "FR" },
];

pub const PROTOCOLS: &[&str] = &["tcp443", "tcp80", "udp53", "udp25000"];

fn http_get(url: &str) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).build();
    let attempt = agent
        .get(url)
        .call()
        .map_err(|e| e.to_string())
        .and_then(|r| r.into_string().map_err(|e| e.to_string()));
    match attempt {
        Ok(body) => Ok(body),
        Err(error) => {
            #[cfg(windows)]
            let program = "curl.exe";
            #[cfg(not(windows))]
            let program = "curl";
            let output = std::process::Command::new(program)
                .args(["-sSL", "--max-time", "40", url])
                .output()
                .map_err(|e| format!("VPNBook fetch failed: {} (curl: {})", error, e))?;
            if !output.status.success() {
                return Err(format!("VPNBook fetch failed: {}", error));
            }
            String::from_utf8(output.stdout).map_err(|e| e.to_string())
        }
    }
}

/// Current shared password published by VPNBook (rotates every 1-2 weeks).
pub fn password() -> Result<String, String> {
    let html = http_get(PAGE)?;
    let marker = "Password</label>";
    let from = html
        .find(marker)
        .ok_or("VPNBook password label not found")?
        + marker.len();
    let tag = html[from..].find("<code").ok_or("VPNBook password code not found")? + from;
    let open = html[tag..].find('>').ok_or("VPNBook password tag malformed")? + tag + 1;
    let close = html[open..].find("</code>").ok_or("VPNBook password value not found")? + open;
    let value = html[open..close].trim();
    if value.is_empty() {
        return Err("VPNBook password is empty".into());
    }
    Ok(value.to_string())
}

/// Download an OpenVPN config for a server/protocol pair.
pub fn config(hostname: &str, protocol: &str) -> Result<String, String> {
    if !PROTOCOLS.contains(&protocol) {
        return Err(format!("Unknown VPNBook protocol: {}", protocol));
    }
    if !SERVERS.iter().any(|s| s.hostname == hostname) {
        return Err(format!("Unknown VPNBook server: {}", hostname));
    }
    let url = format!("{}/api/openvpn?hostname={}&protocol={}", BASE, hostname, protocol);
    http_get(&url)
}

/// Two-line auth-user-pass file content.
pub fn auth_file(password: &str) -> String {
    format!("vpnbook\n{}\n", password)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "hits the live VPNBook site"]
    fn live_password_and_config() {
        let pass = password().expect("live password fetch failed");
        assert_eq!(pass.len() >= 4, true);
        let cfg = config("us16.vpnbook.com", "tcp443").expect("live config fetch failed");
        assert!(cfg.contains("remote us16.vpnbook.com 443"));
        assert!(cfg.contains("auth-user-pass"));
    }

    #[test]
    fn server_and_protocol_catalog() {
        assert_eq!(SERVERS.len(), 10);
        assert!(PROTOCOLS.contains(&"tcp443"));
        assert!(config("bad.example", "tcp443").is_err());
        assert!(config("us16.vpnbook.com", "bad").is_err());
    }

    #[test]
    fn parses_password_from_markup() {
        let html = "x Password</label><div><code class=\"c\">abc123</code></div>";
        let marker = "Password</label>";
        let from = html.find(marker).unwrap() + marker.len();
        let tag = html[from..].find("<code").unwrap() + from;
        let open = html[tag..].find('>').unwrap() + tag + 1;
        let close = html[open..].find("</code>").unwrap() + open;
        assert_eq!(&html[open..close], "abc123");
    }

    #[test]
    fn auth_file_has_two_lines() {
        assert_eq!(auth_file("secret"), "vpnbook\nsecret\n");
    }
}
