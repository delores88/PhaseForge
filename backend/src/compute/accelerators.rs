//! OS device inventory is deliberately separate from initialized numerical engines.
use serde_json::{json, Value};

pub async fn neural_inventory() -> Value {
    #[cfg(windows)] {
        let script = r#"$ErrorActionPreference='Stop'; $items=@(Get-CimInstance Win32_PnPEntity | Where-Object { $_.Name -match '(?i)(\bNPU\b|neural processing|Intel\(R\) AI Boost|AMD IPU|AMD Ryzen AI|Hexagon)' } | Select-Object Name,Manufacturer,Status,PNPDeviceID); ConvertTo-Json -InputObject $items -Compress"#;
        if let Some(output) = super::telemetry::command_output(std::path::Path::new("powershell.exe"), &["-NoProfile", "-NonInteractive", "-Command", script]).await {
            if let Ok(Value::Array(devices)) = serde_json::from_str::<Value>(&output) {
                return json!({"probe_status":"complete","devices":devices,"numerical_execution_available":false,
                    "scope":"Windows PnP inventory. No NPU solver backend is installed; these devices are not counted as simulation capacity."});
            }
        }
    }
    #[cfg(target_os = "linux")] {
        if let Ok(entries) = std::fs::read_dir("/sys/class/accel") {
            let devices = entries.filter_map(Result::ok).map(|entry| {
                let driver = std::fs::read_link(entry.path().join("device/driver")).ok()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()));
                json!({"name":entry.file_name().to_string_lossy(),"driver":driver,"source":"Linux accel class"})
            }).collect::<Vec<_>>();
            return json!({"probe_status":"complete","devices":devices,"numerical_execution_available":false,
                "scope":"Linux accelerator class inventory. Devices are not assumed to be NPUs, and no NPU numerical backend is installed."});
        }
    }
    json!({"probe_status":"unavailable","devices":[],"numerical_execution_available":false,
        "scope":"The OS did not provide a supported accelerator inventory. NPU presence and capacity are unknown; no NPU numerical backend is installed."})
}
