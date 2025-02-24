use std::fs;

use serde::{Deserialize, Serialize};

use crate::net::config::NetworkConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DeviceConfig {
    pub(crate) network: NetworkConfig,
}

impl DeviceConfig {
    pub(crate) fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }
}
