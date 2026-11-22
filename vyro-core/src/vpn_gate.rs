//! VPN Gate provider integration using the public Vpngate-Scraper-API JSON feed.
//!
//! The feed contains base64-encoded OpenVPN configs. VYRO never embeds private
//! keys and never claims a connection is active unless the OpenVPN process is
//! actually launched by the configured provider command.

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const DATA_URL: &str =
    "https://bagussatoto.github.io/Vpngate-Scraper-API/json/data.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpnGateServer {
    /// Index into the fetched list — used to select a server in the UI.
    pub index: usize,
    pub hostname: String,
    pub ip: String,
    pub ping: String,
    pub speed: String,
    pub country: String,
    pub country_code: String,
    /// Base64 OpenVPN config from the feed. Kept out of the UI payload.
    #[serde(skip_serializing)]
    pub config_b64: String,
}

impl VpnGateServer {
    /// Decode the embedded `.ovpn` configuration.
    pub fn config(&self) -> Result<String, String> {
        let clean: String = self
            .config_b64
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(clean)
            .map_err(|e| format!("Invalid VPN Gate config: {}", e))?;
        String::from_utf8(bytes).map_err(|e| format!("Invalid VPN Gate config text: {}", e))
    }
}

/// Parse the scraper's `{ "servers": [...] }` top-level array.
pub fn parse(data: &str) -> Result<Vec<VpnGateServer>, String> {
    let root: serde_json::Value =
        serde_json::from_str(data).map_err(|e| format!("Invalid VPN Gate feed: {}", e))?;
    let groups = root
        .as_array()
        .ok_or_else(|| "VPN Gate feed is not an array".to_string())?;
    let mut out = Vec::new();
    for group in groups {
        if let Some(list) = group.get("servers").and_then(|v| v.as_array()) {
            for s in list {
                let get = |k: &str| s.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let config_b64 = get("openvpn_configdata_base64");
                if config_b64.is_empty() {
                    continue;
                }
                out.push(VpnGateServer {
                    index: out.len(),
                    hostname: get("hostname"),
                    ip: get("ip"),
                    ping: get("ping"),
                    speed: get("speed"),
                    country: get("countrylong"),
                    country_code: get("countryshort"),
                    config_b64,
                });
            }
        }
    }
    Ok(out)
}

fn http_get(url: &str) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .build();
    let attempt = agent
        .get(url)
        .call()
        .map_err(|e| e.to_string())
        .and_then(|r| r.into_string().map_err(|e| e.to_string()));
    match attempt {
        Ok(body) => Ok(body),
        Err(ureq_error) => {
            // Some networks stall the pure-Rust client. Windows ships curl.exe;
            // use it as a genuine fallback rather than inventing server data.
            #[cfg(windows)]
            let program = "curl.exe";
            #[cfg(not(windows))]
            let program = "curl";
            let output = std::process::Command::new(program)
                .args(["-sSL", "--max-time", "45", url])
                .output()
                .map_err(|e| format!("VPN Gate fetch failed: {} (curl unavailable: {})", ureq_error, e))?;
            if !output.status.success() {
                return Err(format!(
                    "VPN Gate fetch failed: {} (curl: {})",
                    ureq_error,
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
            String::from_utf8(output.stdout).map_err(|e| e.to_string())
        }
    }
}

/// Fetch the current server list over HTTPS.
pub fn fetch_servers() -> Result<Vec<VpnGateServer>, String> {
    let body = http_get(DATA_URL)?;
    parse(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "hits the live VPN Gate feed"]
    fn live_fetch_returns_servers() {
        let servers = fetch_servers().expect("live VPN Gate fetch failed");
        assert!(!servers.is_empty());
        assert!(servers.iter().filter_map(|s| s.config().ok()).any(|c| !c.is_empty()));
        println!("fetched {} servers", servers.len());
    }

    #[test]
    fn parses_feed_and_decodes_config() {
        let cfg = base64::engine::general_purpose::STANDARD.encode("dev tun\nremote 1.2.3.4 1194\n");
        let json = format!(
            r#"[{{"servers":[{{"hostname":"vpn1","ip":"1.2.3.4","ping":"10","speed":"1000","countrylong":"Japan","countryshort":"JP","openvpn_configdata_base64":"{}"}}]}}]"#,
            cfg
        );
        let servers = parse(&json).unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].country_code, "JP");
        assert!(servers[0].config().unwrap().contains("remote 1.2.3.4"));
    }
}
