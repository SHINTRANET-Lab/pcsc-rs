//! GPU names on Windows XP, Vista and 7, which have neither the GPU performance counters nor
//! DXGI's memory figures: the display adapters the registry lists. Usage and memory are
//! unknown, so the usage is hidden (see `SystemStatus::hide`).

use std::{os::windows::process::CommandExt, process::Command, sync::OnceLock};

use crate::status::{GpuData, GpuMemory};

const DISPLAY_CLASS: &str =
    r"HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

/// Adapters that are not GPUs the owner would want listed
const IGNORED: [&str; 4] = ["mirror", "remote", "virtual desktop", "basic render"];

/// The GPUs do not change while the client runs; reg.exe is a process each time
pub fn get_info() -> Vec<GpuData> {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES
        .get_or_init(names)
        .iter()
        .map(|name| GpuData {
            name: name.clone(),
            usage: 0.0,
            memory: GpuMemory { free: 0, total: 0 },
            temperature: None,
            usage_known: false,
        })
        .collect()
}

fn names() -> Vec<String> {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // 「/s」だけで全部の値をたどると Vista では遅く、3 秒の制限に掛かる。必要な値だけを聞く
    let mut text = String::new();
    for value in ["DriverDesc", "MatchingDeviceId"] {
        let mut command = Command::new("reg");
        command
            .args(["query", DISPLAY_CLASS, "/s", "/v", value])
            .creation_flags(CREATE_NO_WINDOW);
        // 一部のサブキーが読めないと終了コードが 1 になるので、成否ではなく出力を読む
        if let Ok(output) = crate::cmd::run(&mut command) {
            text.push_str(&output.stdout);
            text.push('\n');
        }
    }
    parse(&text)
}

/// One adapter per registry key (`...\0000`): `DriverDesc` is the driver's name for it
/// (`Mobile Intel(R) 965 Express Chipset Family`), `MatchingDeviceId` has the PCI IDs
/// (`pci\ven_8086&dev_2a02&...`), which name Intel's GPUs better.
fn parse(output: &str) -> Vec<String> {
    // (キー, DriverDesc, MatchingDeviceId)
    let mut adapters: Vec<(String, String, String)> = Vec::new();
    let mut key = String::new();
    for line in output.lines() {
        let line = line.trim();
        if line.starts_with("HKEY_") {
            key = line.to_ascii_lowercase();
        } else if let Some((name, value)) = line.split_once("REG_SZ") {
            let value = value.trim();
            let index = adapters.iter().position(|a| a.0 == key).unwrap_or_else(|| {
                adapters.push((key.clone(), String::new(), String::new()));
                adapters.len() - 1
            });
            match name.trim() {
                "DriverDesc" => value.clone_into(&mut adapters[index].1),
                "MatchingDeviceId" => {
                    value
                        .to_ascii_lowercase()
                        .clone_into(&mut adapters[index].2);
                }
                _ => {}
            }
        }
    }
    let mut names: Vec<String> = Vec::new();
    for (_, desc, id) in &adapters {
        let Some(name) = adapter_name(desc.trim(), id) else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        if !name.is_empty() && !IGNORED.iter().any(|i| lower.contains(i)) && !names.contains(&name)
        {
            names.push(name);
        }
    }
    names
}

fn adapter_name(desc: &str, matching_id: &str) -> Option<String> {
    let field = |tag: &str| {
        let start = matching_id.find(tag)? + tag.len();
        matching_id.get(start..start + 4)
    };
    if field("ven_") == Some("8086")
        && let Some(device) = field("dev_")
    {
        if super::intel::is_secondary(device) {
            return None;
        }
        if let Some(name) = super::intel::name(device) {
            return Some(format!("Intel {name}"));
        }
    }
    Some(desc.to_owned())
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_from_reg() {
        let out = "\r\nHKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Control\\Class\\{4d36e968-e325-11ce-bfc1-08002be10318}\\0000\r\n    DriverDesc    REG_SZ    NVIDIA GeForce 9600 GT\r\n    MatchingDeviceId    REG_SZ    pci\\ven_10de&dev_0622\r\n\r\nHKEY_LOCAL_MACHINE\\...\\0001\r\n    DriverDesc    REG_SZ    Mobile Intel(R) 965 Express Chipset Family\r\n    MatchingDeviceId    REG_SZ    pci\\ven_8086&dev_2a02&subsys_1234&rev_0c\r\n\r\nHKEY_LOCAL_MACHINE\\...\\0002\r\n    DriverDesc    REG_SZ    Mobile Intel(R) 965 Express Chipset Family\r\n    MatchingDeviceId    REG_SZ    pci\\ven_8086&dev_2a03\r\n\r\nHKEY_LOCAL_MACHINE\\...\\0003\r\n    DriverDesc    REG_SZ    Mirage Mirror Driver\r\n";
        assert_eq!(
            super::parse(out),
            ["NVIDIA GeForce 9600 GT", "Intel GMA X3100"]
        );
    }
}
