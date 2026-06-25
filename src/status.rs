use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StatusSnapshot {
    pub release: String,
    pub recovery_version: String,
    pub variant: String,
    pub build_type: String,
    pub branch: String,
    pub codebase_sdk: String,
    pub codebase: String,
    pub build_date: String,
    pub device_model: String,
    pub device_code: String,
    pub storage_encrypted: bool,
    pub storage_decrypted: bool,
    pub dynamic_partitions: bool,
    pub boot_slot: String,
    pub virtual_ab: bool,
    pub kernel: String,
    pub rom_name: String,
    pub rom_incremental: String,
    pub rom_sdk: String,
    pub android_release: String,
    pub rom_miui: bool,
    pub fingerprint: String,
}

impl StatusSnapshot {
    /// Build from the engine's `status` data-event payload (a JSON object with
    /// the fields above). Missing/empty payloads yield a default snapshot.
    pub fn from_data(value: Option<&Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }
}
