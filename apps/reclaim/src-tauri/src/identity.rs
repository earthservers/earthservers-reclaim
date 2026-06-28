// Anonymous User Identity System for Rating System
// Generates hardware-based fingerprints for deduplication without requiring account signup

use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use sysinfo::System;

// ==================== Data Structures ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub cpu_brand: String,
    pub cpu_cores: usize,
    pub machine_id: String,
    pub os_name: String,
    pub os_version: String,
    pub hostname_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceFingerprint {
    pub fingerprint: String,
    pub hardware_info: HardwareInfo,
}

// ==================== Identity Functions ====================

/// Get hardware information for fingerprinting
pub fn get_hardware_info() -> HardwareInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    // Get CPU info
    let cpu_brand = sys.cpus()
        .first()
        .map(|cpu| cpu.brand().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let cpu_cores = sys.cpus().len();

    // Get machine ID (unique per installation)
    let machine_id = machine_uid::get()
        .unwrap_or_else(|_| "unknown".to_string());

    // Get OS info
    let os_name = System::name().unwrap_or_else(|| "unknown".to_string());
    let os_version = System::os_version().unwrap_or_else(|| "unknown".to_string());

    // Hash the hostname for privacy
    let hostname = System::host_name().unwrap_or_else(|| "unknown".to_string());
    let hostname_hash = hash_string(&hostname);

    HardwareInfo {
        cpu_brand,
        cpu_cores,
        machine_id,
        os_name,
        os_version,
        hostname_hash,
    }
}

/// Generate a unique device fingerprint from hardware info
/// This hash is used for server-side deduplication
pub fn generate_device_fingerprint() -> DeviceFingerprint {
    let info = get_hardware_info();

    // Combine hardware identifiers into a unique fingerprint
    let fingerprint_data = format!(
        "{}:{}:{}:{}:{}",
        info.machine_id,
        info.cpu_brand,
        info.cpu_cores,
        info.os_name,
        info.hostname_hash
    );

    let fingerprint = hash_string(&fingerprint_data);

    DeviceFingerprint {
        fingerprint,
        hardware_info: info,
    }
}

/// Hash a string using SHA-256
fn hash_string(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hardware_info() {
        let info = get_hardware_info();
        assert!(!info.machine_id.is_empty());
    }

    #[test]
    fn test_fingerprint_generation() {
        let fp = generate_device_fingerprint();
        assert_eq!(fp.fingerprint.len(), 64); // SHA-256 hex = 64 chars
    }

    #[test]
    fn test_fingerprint_consistency() {
        let fp1 = generate_device_fingerprint();
        let fp2 = generate_device_fingerprint();
        assert_eq!(fp1.fingerprint, fp2.fingerprint);
    }
}
