//! Modular, provider-agnostic VPN integration layer.
//!
//! IMPORTANT: VYRO does **not** simulate VPN connections and ships no provider
//! credentials. A provider must be explicitly configured with an external
//! tunneling command (e.g. `wg-quick`, `openvpn`, or a vendor CLI). When no
//! provider or command is configured, `connect` returns a clear error.
//! Credentials are read from an environment variable and passed to the child
//! process as `VYRO_VPN_SECRET`; they are never stored or logged by VYRO.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VpnServer {
    pub id: String,
    pub country: String,
    pub city: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VpnProviderConfig {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub servers: Vec<VpnServer>,
    /// Command executed to establish the tunnel. Placeholders: {server}, {country}.
    pub connect_command: Option<String>,
    /// Command executed to tear the tunnel down.
    pub disconnect_command: Option<String>,
    /// Name of the environment variable holding the provider credential.
    pub credential_env: Option<String>,
    /// Launch a long-lived process (e.g. OpenVPN) without waiting for exit.
    #[serde(default)]
    pub background: bool,
    /// Direct executable path, used instead of `connect_command` in background mode.
    #[serde(default)]
    pub connect_program: Option<String>,
    /// Arguments passed to `connect_program` (no shell involved).
    #[serde(default)]
    pub connect_args: Vec<String>,
}

impl Default for VpnProviderConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            protocol: String::new(),
            servers: Vec::new(),
            connect_command: None,
            disconnect_command: None,
            credential_env: None,
            background: false,
            connect_program: None,
            connect_args: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VpnSession {
    pub provider_id: String,
    pub server_id: String,
    pub connected_at: u64,
    #[serde(default)]
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VpnStatus {
    Disconnected,
    Connecting,
    Connected(VpnSession),
    Reconnecting,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VpnError {
    NoProviderConfigured,
    ProviderNotFound(String),
    ServerNotFound(String),
    MissingConnectCommand(String),
    MissingCredential(String),
    CommandFailed(String),
    NotConnected,
}

impl fmt::Display for VpnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VpnError::NoProviderConfigured => write!(
                f,
                "No VPN provider configured. Add a provider with a tunneling command in Settings → VPN."
            ),
            VpnError::ProviderNotFound(id) => write!(f, "VPN provider '{}' is not configured.", id),
            VpnError::ServerNotFound(id) => write!(f, "Server '{}' is not in the provider list.", id),
            VpnError::MissingConnectCommand(id) => write!(
                f,
                "Provider '{}' has no connect_command configured; VYRO will not simulate a connection.",
                id
            ),
            VpnError::MissingCredential(name) => {
                write!(f, "VPN credential environment variable '{}' is not set.", name)
            }
            VpnError::CommandFailed(msg) => write!(f, "VPN provider command failed: {}", msg),
            VpnError::NotConnected => write!(f, "No active VPN connection."),
        }
    }
}
impl std::error::Error for VpnError {}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn run_command(template: &str, server: &str, country: &str, secret: Option<&str>) -> Result<String, VpnError> {
    let cmd = template
        .replace("{server}", server)
        .replace("{server_id}", server)
        .replace("{country}", country);

    #[cfg(windows)]
    let mut child = {
        let mut c = Command::new("cmd");
        c.args(["/C", &cmd]);
        c
    };
    #[cfg(not(windows))]
    let mut child = {
        let mut c = Command::new("sh");
        c.args(["-c", &cmd]);
        c
    };

    if let Some(secret) = secret {
        child.env("VYRO_VPN_SECRET", secret);
    }
    let output = child
        .output()
        .map_err(|e| VpnError::CommandFailed(e.to_string()))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(VpnError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug)]
pub struct VpnManager {
    pub providers: Vec<VpnProviderConfig>,
    pub status: VpnStatus,
    pub kill_switch: bool,
    pub dns_leak_protection: bool,
    pub auto_reconnect: bool,
    selected_server: Option<String>,
}

impl Default for VpnManager {
    fn default() -> Self {
        Self {
            providers: Vec::new(),
            status: VpnStatus::Disconnected,
            kill_switch: false,
            dns_leak_protection: true,
            auto_reconnect: true,
            selected_server: None,
        }
    }
}

impl VpnManager {
    pub fn add_provider(&mut self, provider: VpnProviderConfig) {
        self.providers.push(provider);
    }

    pub fn provider(&self, id: &str) -> Option<&VpnProviderConfig> {
        self.providers.iter().find(|p| p.id == id)
    }

    pub fn countries(&self) -> Vec<String> {
        let mut c: Vec<String> = self
            .providers
            .iter()
            .flat_map(|p| p.servers.iter().map(|s| s.country.clone()))
            .collect();
        c.sort();
        c.dedup();
        c
    }

    pub fn servers_for(&self, provider_id: &str) -> Vec<VpnServer> {
        self.provider(provider_id)
            .map(|p| p.servers.clone())
            .unwrap_or_default()
    }

    pub fn select_server(&mut self, server_id: impl Into<String>) {
        self.selected_server = Some(server_id.into());
    }

    fn secret_for(&self, provider: &VpnProviderConfig) -> Result<Option<String>, VpnError> {
        match &provider.credential_env {
            Some(name) => {
                let value = std::env::var(name)
                    .map_err(|_| VpnError::MissingCredential(name.clone()))?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    /// Establish a real tunnel via the provider's configured command.
    pub fn connect(&mut self, provider_id: &str, server_id: &str) -> Result<(), VpnError> {
        if self.providers.is_empty() {
            return Err(VpnError::NoProviderConfigured);
        }
        let provider = self
            .provider(provider_id)
            .ok_or_else(|| VpnError::ProviderNotFound(provider_id.to_string()))?
            .clone();
        if !provider.servers.is_empty() && !provider.servers.iter().any(|s| s.id == server_id) {
            return Err(VpnError::ServerNotFound(server_id.to_string()));
        }
        let secret = self.secret_for(&provider)?;
        let country = provider
            .servers
            .iter()
            .find(|s| s.id == server_id)
            .map(|s| s.country.clone())
            .unwrap_or_default();

        self.status = VpnStatus::Connecting;
        // Long-lived providers (OpenVPN) are spawned, never waited on — the
        // command would otherwise block until the tunnel is torn down.
        let launched = if provider.background {
            let program = provider
                .connect_program
                .clone()
                .ok_or_else(|| VpnError::MissingConnectCommand(provider.id.clone()))?;
            let mut cmd = Command::new(program);
            cmd.args(&provider.connect_args);
            if let Some(s) = secret.as_deref() {
                cmd.env("VYRO_VPN_SECRET", s);
            }
            cmd.spawn()
                .map(|child| Some(child.id()))
                .map_err(|e| VpnError::CommandFailed(e.to_string()))
        } else {
            let connect_cmd = provider
                .connect_command
                .clone()
                .ok_or_else(|| VpnError::MissingConnectCommand(provider.id.clone()))?;
            run_command(&connect_cmd, server_id, &country, secret.as_deref()).map(|_| None)
        };

        match launched {
            Ok(pid) => {
                self.selected_server = Some(server_id.to_string());
                self.status = VpnStatus::Connected(VpnSession {
                    provider_id: provider.id.clone(),
                    server_id: server_id.to_string(),
                    connected_at: now(),
                    pid,
                });
                Ok(())
            }
            Err(e) => {
                self.status = VpnStatus::Error(e.to_string());
                Err(e)
            }
        }
    }

    pub fn disconnect(&mut self) -> Result<(), VpnError> {
        let session = match &self.status {
            VpnStatus::Connected(s) => s.clone(),
            _ => return Err(VpnError::NotConnected),
        };
        let provider = self
            .provider(&session.provider_id)
            .ok_or_else(|| VpnError::ProviderNotFound(session.provider_id.clone()))?
            .clone();
        if let Some(pid) = session.pid {
            #[cfg(windows)]
            let kill = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .output();
            #[cfg(not(windows))]
            let kill = Command::new("kill").args(["-TERM", &pid.to_string()]).output();
            let _ = kill;
        }
        if let Some(cmd) = &provider.disconnect_command {
            let secret = self.secret_for(&provider)?;
            run_command(cmd, &session.server_id, "", secret.as_deref())?;
        }
        self.status = VpnStatus::Disconnected;
        self.selected_server = None;
        Ok(())
    }

    /// Honest diagnostics: reports configuration gaps instead of faking health.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let provider = self
            .selected_server
            .as_ref()
            .and_then(|_| self.providers.first())
            .or_else(|| self.providers.first());

        let mut out = vec![
            Diagnostic {
                name: "Provider configured".into(),
                ok: !self.providers.is_empty(),
                detail: if self.providers.is_empty() {
                    "No provider configured — connect() will refuse to simulate a tunnel.".into()
                } else {
                    format!("{} provider(s) configured.", self.providers.len())
                },
            },
            Diagnostic {
                name: "Connect command".into(),
                ok: provider.and_then(|p| p.connect_command.as_ref()).is_some(),
                detail: match provider.and_then(|p| p.connect_command.as_ref()) {
                    Some(c) => format!("Configured: {}", c.split_whitespace().next().unwrap_or("")),
                    None => "No tunneling command — real connections require one.".into(),
                },
            },
            Diagnostic {
                name: "Credential available".into(),
                ok: match provider.and_then(|p| p.credential_env.as_ref()) {
                    Some(name) => std::env::var(name).is_ok(),
                    None => true,
                },
                detail: match provider.and_then(|p| p.credential_env.as_ref()) {
                    Some(name) => format!("Environment variable '{}' {}", name,
                        if std::env::var(name).is_ok() { "is set" } else { "is NOT set" }),
                    None => "Provider uses no credential (e.g. certificate-based auth).".into(),
                },
            },
            Diagnostic {
                name: "Kill switch".into(),
                ok: self.kill_switch && !matches!(crate::platform::current_os(), crate::platform::Os::Other),
                detail: if self.kill_switch {
                    "Enabled — requires firewall rules applied by the provider backend.".into()
                } else {
                    "Disabled.".into()
                },
            },
            Diagnostic {
                name: "DNS leak protection".into(),
                ok: self.dns_leak_protection,
                detail: if self.dns_leak_protection {
                    "Enabled — provider should route DNS through the tunnel.".into()
                } else {
                    "Disabled; system DNS may leak queries.".into()
                },
            },
        ];
        if let VpnStatus::Connected(s) = &self.status {
            out.push(Diagnostic {
                name: "Tunnel".into(),
                ok: true,
                detail: format!("Connected to {} via {}.", s.server_id, s.provider_id),
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_to_simulate_without_provider() {
        let mut m = VpnManager::default();
        let err = m.connect("any", "any").unwrap_err();
        assert_eq!(err, VpnError::NoProviderConfigured);
        assert!(err.to_string().contains("will not simulate") || err.to_string().contains("No VPN provider configured"));
    }

    #[test]
    fn refuses_provider_without_command() {
        let mut m = VpnManager::default();
        m.add_provider(VpnProviderConfig {
            id: "p".into(),
            name: "P".into(),
            protocol: "wireguard".into(),
            servers: vec![VpnServer { id: "s1".into(), country: "JP".into(), city: "Tokyo".into() }],
            connect_command: None,
            disconnect_command: None,
            credential_env: None,
            ..Default::default()
        });
        assert_eq!(m.connect("p", "s1"), Err(VpnError::MissingConnectCommand("p".into())));
    }

    #[test]
    fn server_and_country_selection() {
        let mut m = VpnManager::default();
        m.add_provider(VpnProviderConfig {
            id: "p".into(), name: "P".into(), protocol: "wg".into(),
            servers: vec![
                VpnServer { id: "a".into(), country: "JP".into(), city: "Tokyo".into() },
                VpnServer { id: "b".into(), country: "DE".into(), city: "Berlin".into() },
            ],
            connect_command: None, disconnect_command: None, credential_env: None,
            ..Default::default()
        });
        assert_eq!(m.countries(), vec!["DE", "JP"]);
        assert_eq!(m.servers_for("p").len(), 2);
        assert_eq!(m.connect("p", "missing"), Err(VpnError::ServerNotFound("missing".into())));
    }

    #[test]
    fn connects_using_configured_command() {
        let mut m = VpnManager::default();
        let cmd = if cfg!(windows) { "exit 0" } else { "true" };
        m.add_provider(VpnProviderConfig {
            id: "p".into(), name: "P".into(), protocol: "wg".into(),
            servers: vec![VpnServer { id: "a".into(), country: "JP".into(), city: "Tokyo".into() }],
            connect_command: Some(cmd.into()),
            disconnect_command: Some(cmd.into()),
            credential_env: None,
            ..Default::default()
        });
        assert!(m.connect("p", "a").is_ok());
        assert!(matches!(m.status, VpnStatus::Connected(_)));
        assert!(m.disconnect().is_ok());
        assert_eq!(m.status, VpnStatus::Disconnected);
    }

    #[test]
    fn background_provider_spawns_and_kills_process() {
        let mut m = VpnManager::default();
        m.add_provider(VpnProviderConfig {
            id: "bg".into(),
            name: "Background".into(),
            protocol: "test".into(),
            servers: vec![VpnServer { id: "s".into(), country: "US".into(), city: "Test".into() }],
            background: true,
            connect_program: Some(if cfg!(windows) { "ping".into() } else { "sleep".into() }),
            connect_args: if cfg!(windows) {
                vec!["-n".into(), "20".into(), "127.0.0.1".into()]
            } else {
                vec!["20".into()]
            },
            ..Default::default()
        });
        m.connect("bg", "s").unwrap();
        match &m.status {
            VpnStatus::Connected(s) => assert!(s.pid.is_some()),
            _ => panic!("expected connected"),
        }
        assert!(m.disconnect().is_ok());
    }

    #[test]
    fn diagnostics_flag_missing_config() {
        let m = VpnManager::default();
        let d = m.diagnostics();
        assert_eq!(d[0].name, "Provider configured");
        assert!(!d[0].ok);
        assert!(!d[1].ok);
    }
}
