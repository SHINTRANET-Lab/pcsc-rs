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
    let mut command = Command::new("reg");
    command
        .args(["query", DISPLAY_CLASS, "/s"])
        .creation_flags(CREATE_NO_WINDOW);
    match crate::cmd::run(&mut command) {
        Ok(output) if output.success => parse(&output.stdout),
        _ => Vec::new(),
    }
}

/// One adapter per registry key (`...\0000`): `DriverDesc` is the driver's name for it
/// (`Mobile Intel(R) 965 Express Chipset Family`), `MatchingDeviceId` has the PCI IDs
/// (`pci\ven_8086&dev_2a02&...`), which name Intel's GPUs better.
fn parse(output: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut desc = String::new();
    let mut id = String::new();
    let mut finish = |desc: &mut String, id: &mut String| {
        if let Some(name) = adapter_name(desc.trim(), id) {
            let lower = name.to_ascii_lowercase();
            if !name.is_empty()
                && !IGNORED.iter().any(|i| lower.contains(i))
                && !names.contains(&name)
            {
                names.push(name);
            }
        }
        desc.clear();
        id.clear();
    };
    for line in output.lines() {
        let line = line.trim();
        if line.starts_with("HKEY_") {
            finish(&mut desc, &mut id);
        } else if let Some((key, value)) = line.split_once("REG_SZ") {
            match key.trim() {
                "DriverDesc" => value.trim().clone_into(&mut desc),
                "MatchingDeviceId" => value.trim().to_ascii_lowercase().clone_into(&mut id),
                _ => {}
            }
        }
    }
    finish(&mut desc, &mut id);
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
