//! API keys (AI-01): session-only by default; optionally remembered in the
//! OS credential store. Never written to projects, exports, or logs.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::Provider;

const SERVICE: &str = "io.github.samueldotj.sbwb";

#[derive(Default)]
pub struct KeyStore {
    session: Mutex<HashMap<Provider, String>>,
}

impl KeyStore {
    fn entry(provider: Provider) -> Option<keyring::Entry> {
        keyring::Entry::new(SERVICE, &format!("ai:{}", provider.id())).ok()
    }

    /// Keep for this session; with `remember`, also in the OS store.
    pub fn set(&self, provider: Provider, key: &str, remember: bool) -> Result<bool, String> {
        let key = key.trim().to_string();
        if key.is_empty() {
            return Err("the key is empty".into());
        }
        self.session
            .lock()
            .map_err(|_| "key store poisoned")?
            .insert(provider, key.clone());
        if remember {
            match Self::entry(provider) {
                Some(e) => e
                    .set_password(&key)
                    .map_err(|e| format!("the OS credential store refused the key: {e}"))?,
                None => return Err(
                    "no OS credential store is available; the key is kept for this session only"
                        .into(),
                ),
            }
            return Ok(true);
        }
        Ok(false)
    }

    /// Session key, else the remembered one.
    pub fn get(&self, provider: Provider) -> Option<String> {
        if let Some(k) = self
            .session
            .lock()
            .ok()
            .and_then(|m| m.get(&provider).cloned())
        {
            return Some(k);
        }
        Self::entry(provider)
            .and_then(|e| e.get_password().ok())
            .filter(|k| !k.is_empty())
    }

    pub fn remembered(&self, provider: Provider) -> bool {
        Self::entry(provider)
            .and_then(|e| e.get_password().ok())
            .map(|k| !k.is_empty())
            .unwrap_or(false)
    }

    pub fn has_session(&self, provider: Provider) -> bool {
        self.session
            .lock()
            .ok()
            .map(|m| m.contains_key(&provider))
            .unwrap_or(false)
    }

    pub fn forget(&self, provider: Provider) {
        if let Ok(mut m) = self.session.lock() {
            m.remove(&provider);
        }
        if let Some(e) = Self::entry(provider) {
            let _ = e.delete_credential();
        }
    }
}
