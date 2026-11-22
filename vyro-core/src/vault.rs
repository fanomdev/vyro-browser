//! Encrypted password vault. Master password derives an AES-256-GCM key via
//! Argon2; only ciphertext is persisted. The key lives in memory while unlocked.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use argon2::Argon2;
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};

const VERIFIER: &str = "VYRO-VAULT-V1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultError {
    NotInitialized,
    AlreadyInitialized,
    Locked,
    WrongPassword,
    NotFound(u64),
    Crypto(String),
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VaultError::NotInitialized => write!(f, "Vault has no master password yet."),
            VaultError::AlreadyInitialized => write!(f, "Vault is already initialized."),
            VaultError::Locked => write!(f, "Vault is locked. Unlock it first."),
            VaultError::WrongPassword => write!(f, "Incorrect master password."),
            VaultError::NotFound(id) => write!(f, "Credential {} not found.", id),
            VaultError::Crypto(e) => write!(f, "Encryption error: {}", e),
        }
    }
}
impl std::error::Error for VaultError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultEntry {
    pub id: u64,
    pub service: String,
    pub username: String,
    nonce: String,
    cipher: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultFile {
    salt: String,
    verifier_nonce: String,
    verifier_cipher: String,
    entries: Vec<VaultEntry>,
    next_id: u64,
}

impl Default for VaultFile {
    fn default() -> Self {
        Self {
            salt: String::new(),
            verifier_nonce: String::new(),
            verifier_cipher: String::new(),
            entries: Vec::new(),
            next_id: 1,
        }
    }
}

#[derive(Debug)]
pub struct Vault {
    file: VaultFile,
    key: Option<Key<Aes256Gcm>>,
}

impl Default for Vault {
    fn default() -> Self {
        Self::new()
    }
}

fn derive(master: &str, salt: &[u8]) -> Key<Aes256Gcm> {
    let mut key = Key::<Aes256Gcm>::default();
    Argon2::default()
        .hash_password_into(master.as_bytes(), salt, &mut key)
        .expect("Argon2 key derivation is infallible for valid parameters");
    key
}

fn encrypt(key: &Key<Aes256Gcm>, plain: &str) -> Result<(String, String), VaultError> {
    let cipher = Aes256Gcm::new(key);
    let mut bytes = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let nonce = Nonce::from(bytes);
    let ct = cipher
        .encrypt(&nonce, plain.as_bytes())
        .map_err(|e| VaultError::Crypto(e.to_string()))?;
    Ok((
        base64::engine::general_purpose::STANDARD.encode(bytes),
        base64::engine::general_purpose::STANDARD.encode(ct),
    ))
}

fn decrypt(key: &Key<Aes256Gcm>, nonce_b64: &str, cipher_b64: &str) -> Result<String, VaultError> {
    let cipher = Aes256Gcm::new(key);
    let nonce_bytes = base64::engine::general_purpose::STANDARD
        .decode(nonce_b64)
        .map_err(|e| VaultError::Crypto(e.to_string()))?;
    let ct = base64::engine::general_purpose::STANDARD
        .decode(cipher_b64)
        .map_err(|e| VaultError::Crypto(e.to_string()))?;
    let nonce_array: [u8; 12] = nonce_bytes
        .as_slice()
        .try_into()
        .map_err(|_| VaultError::Crypto("invalid nonce length".into()))?;
    let plain = cipher
        .decrypt(&Nonce::from(nonce_array), ct.as_slice())
        .map_err(|_| VaultError::WrongPassword)?;
    String::from_utf8(plain).map_err(|e| VaultError::Crypto(e.to_string()))
}

impl Vault {
    pub fn new() -> Self {
        Self {
            file: VaultFile::default(),
            key: None,
        }
    }

    pub fn from_json(json: &str) -> Result<Self, VaultError> {
        let file: VaultFile =
            serde_json::from_str(json).map_err(|e| VaultError::Crypto(e.to_string()))?;
        Ok(Self { file, key: None })
    }

    pub fn to_json(&self) -> Result<String, VaultError> {
        serde_json::to_string_pretty(&self.file).map_err(|e| VaultError::Crypto(e.to_string()))
    }

    pub fn is_initialized(&self) -> bool {
        !self.file.salt.is_empty()
    }
    pub fn is_unlocked(&self) -> bool {
        self.key.is_some()
    }
    pub fn count(&self) -> usize {
        self.file.entries.len()
    }

    pub fn initialize(&mut self, master: &str) -> Result<(), VaultError> {
        if self.is_initialized() {
            return Err(VaultError::AlreadyInitialized);
        }
        let mut salt = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut salt);
        let key = derive(master, &salt);
        let (nonce, cipher) = encrypt(&key, VERIFIER)?;
        self.file.salt = base64::engine::general_purpose::STANDARD.encode(salt);
        self.file.verifier_nonce = nonce;
        self.file.verifier_cipher = cipher;
        self.key = Some(key);
        Ok(())
    }

    pub fn unlock(&mut self, master: &str) -> Result<(), VaultError> {
        if !self.is_initialized() {
            return Err(VaultError::NotInitialized);
        }
        let salt = base64::engine::general_purpose::STANDARD
            .decode(&self.file.salt)
            .map_err(|e| VaultError::Crypto(e.to_string()))?;
        let key = derive(master, &salt);
        decrypt(&key, &self.file.verifier_nonce, &self.file.verifier_cipher)?;
        self.key = Some(key);
        Ok(())
    }

    pub fn lock(&mut self) {
        self.key = None;
    }

    pub fn add(&mut self, service: &str, username: &str, password: &str) -> Result<u64, VaultError> {
        let key = self.key.as_ref().ok_or(VaultError::Locked)?;
        let (nonce, cipher) = encrypt(key, password)?;
        let id = self.file.next_id;
        self.file.next_id += 1;
        self.file.entries.push(VaultEntry {
            id,
            service: service.to_string(),
            username: username.to_string(),
            nonce,
            cipher,
        });
        Ok(id)
    }

    /// Metadata only — never returns a plaintext password.
    pub fn list(&self) -> Vec<(u64, String, String)> {
        self.file
            .entries
            .iter()
            .map(|e| (e.id, e.service.clone(), e.username.clone()))
            .collect()
    }

    pub fn reveal(&self, id: u64) -> Result<String, VaultError> {
        let key = self.key.as_ref().ok_or(VaultError::Locked)?;
        let entry = self
            .file
            .entries
            .iter()
            .find(|e| e.id == id)
            .ok_or(VaultError::NotFound(id))?;
        decrypt(key, &entry.nonce, &entry.cipher)
    }

    pub fn remove(&mut self, id: u64) -> Result<(), VaultError> {
        let before = self.file.entries.len();
        self.file.entries.retain(|e| e.id != id);
        if self.file.entries.len() == before {
            Err(VaultError::NotFound(id))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_unlock_add_reveal() {
        let mut v = Vault::new();
        assert!(!v.is_initialized());
        v.initialize("correct horse battery staple").unwrap();
        assert!(v.is_unlocked());
        let id = v.add("example.com", "user", "s3cret").unwrap();
        assert_eq!(v.reveal(id).unwrap(), "s3cret");
        assert_eq!(v.list()[0].1, "example.com");

        let json = v.to_json().unwrap();
        assert!(!json.contains("s3cret"));

        let mut v2 = Vault::from_json(&json).unwrap();
        assert!(v2.unlock("wrong").is_err());
        v2.unlock("correct horse battery staple").unwrap();
        assert_eq!(v2.reveal(id).unwrap(), "s3cret");
    }

    #[test]
    fn locked_vault_cannot_reveal() {
        let mut v = Vault::new();
        v.initialize("master").unwrap();
        let id = v.add("svc", "u", "p").unwrap();
        v.lock();
        assert_eq!(v.reveal(id), Err(VaultError::Locked));
        assert!(v.add("svc", "u", "p").is_err());
    }
}
