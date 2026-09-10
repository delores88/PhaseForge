use anyhow::{Context, bail};
use keyring::{Entry, Error as KeyringError};

use crate::domain::ProviderKind;

#[derive(Debug, Clone, Default)]
pub struct SecretStore;

impl SecretStore {
    const SERVICE: &'static str = "PhaseForge AI providers";

    fn entry(provider: ProviderKind) -> anyhow::Result<Entry> {
        Entry::new(Self::SERVICE, provider.account_name())
            .context("the operating-system credential store is unavailable")
    }

    pub fn set_api_key(&self, provider: ProviderKind, api_key: &str) -> anyhow::Result<()> {
        let value = api_key.trim();
        if value.len() < 12 {
            bail!("API key is too short");
        }
        Self::entry(provider)?
            .set_password(value)
            .context("unable to save API key in the operating-system credential store")
    }

    pub fn get_api_key(&self, provider: ProviderKind) -> anyhow::Result<Option<String>> {
        match Self::entry(provider)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(error) => Err(error).context("unable to read API key from credential store"),
        }
    }

    pub fn has_api_key(&self, provider: ProviderKind) -> bool {
        matches!(self.get_api_key(provider), Ok(Some(_)))
    }

    pub fn delete_api_key(&self, provider: ProviderKind) -> anyhow::Result<()> {
        match Self::entry(provider)?.delete_password() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(error) => Err(error).context("unable to delete API key from credential store"),
        }
    }
}
