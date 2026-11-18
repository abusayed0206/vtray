use crate::api::client::ApiKeyProvider;
use crate::core::errors::AppError;
use zeroize::Zeroizing;

const SERVICE: &str = "VT Tray";
const USER: &str = "api-key";

pub struct CredentialStore;

impl CredentialStore {
    pub fn new() -> Self {
        Self
    }

    pub fn set_key(&self, key: &str) -> Result<(), AppError> {
        let key = Zeroizing::new(key.trim().to_string());
        if key.is_empty() {
            return Err(AppError::Validation("API key cannot be empty".to_string()));
        }
        let entry = keyring::Entry::new(SERVICE, USER)
            .map_err(|error| AppError::Credential(error.to_string()))?;
        entry
            .set_password(&key)
            .map_err(|error| AppError::Credential(error.to_string()))
    }

    pub fn delete_key(&self) -> Result<(), AppError> {
        let entry = keyring::Entry::new(SERVICE, USER)
            .map_err(|error| AppError::Credential(error.to_string()))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(AppError::Credential(error.to_string())),
        }
    }

    pub fn has_key(&self) -> Result<bool, AppError> {
        Ok(self.get_key()?.is_some())
    }
}

impl Default for CredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiKeyProvider for CredentialStore {
    fn get_key(&self) -> Result<Option<Zeroizing<String>>, AppError> {
        let entry = keyring::Entry::new(SERVICE, USER)
            .map_err(|error| AppError::Credential(error.to_string()))?;
        match entry.get_password() {
            Ok(value) => Ok(Some(Zeroizing::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(AppError::Credential(error.to_string())),
        }
    }
}
