use serde::Serialize;

#[derive(Default, Serialize)]
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
    pub fn parse(text: &str) -> Self {
        let mut out = StatusSnapshot::default();
        let mut in_block = false;

        for line in text.lines() {
            if line == "FOX_STATUS_BEGIN" {
                in_block = true;
                continue;
            }
            if line == "FOX_STATUS_END" {
                break;
            }
            if !in_block {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            out.set(key, value);
        }

        out
    }

    fn set(&mut self, key: &str, value: &str) {
        match key {
            "release" => self.release = value.to_string(),
            "recovery_version" => self.recovery_version = value.to_string(),
            "variant" => self.variant = value.to_string(),
            "build_type" => self.build_type = value.to_string(),
            "branch" => self.branch = value.to_string(),
            "codebase_sdk" => self.codebase_sdk = value.to_string(),
            "codebase" => self.codebase = value.to_string(),
            "build_date" => self.build_date = value.to_string(),
            "device_model" => self.device_model = value.to_string(),
            "device_code" => self.device_code = value.to_string(),
            "storage_encrypted" => self.storage_encrypted = value == "1",
            "storage_decrypted" => self.storage_decrypted = value == "1",
            "dynamic_partitions" => self.dynamic_partitions = value == "1",
            "boot_slot" => self.boot_slot = value.to_string(),
            "virtual_ab" => self.virtual_ab = value == "1",
            "kernel" => self.kernel = value.to_string(),
            "rom_name" => self.rom_name = value.to_string(),
            "rom_incremental" => self.rom_incremental = value.to_string(),
            "rom_sdk" => self.rom_sdk = value.to_string(),
            "android_release" => self.android_release = value.to_string(),
            "rom_miui" => self.rom_miui = value == "1",
            "fingerprint" => self.fingerprint = value.to_string(),
            _ => {}
        }
    }
}
