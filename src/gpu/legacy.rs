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
        .args(["query", DISPLAY_CLASS, "/s", "/v", "DriverDesc"])
        .creation_flags(CREATE_NO_WINDOW);
    match crate::cmd::run(&mut command) {
        Ok(output) if output.success => parse(&output.stdout),
        _ => Vec::new(),
    }
}

/// `    DriverDesc    REG_SZ    NVIDIA GeForce 9600 GT` for each adapter
fn parse(output: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for line in output.lines() {
        let Some((key, value)) = line.trim().split_once("REG_SZ") else {
            continue;
        };
        let name = value.trim();
        let lower = name.to_ascii_lowercase();
        if key.trim() != "DriverDesc"
            || name.is_empty()
            || IGNORED.iter().any(|i| lower.contains(i))
            || names.iter().any(|n| n == name)
        {
            continue;
        }
        names.push(name.to_owned());
    }
    names
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_from_reg() {
        let out = "\r\nHKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Control\\Class\\{4d36e968-e325-11ce-bfc1-08002be10318}\\0000\r\n    DriverDesc    REG_SZ    NVIDIA GeForce 9600 GT\r\n\r\nHKEY_LOCAL_MACHINE\\...\\0001\r\n    DriverDesc    REG_SZ    Intel(R) HD Graphics\r\n\r\nHKEY_LOCAL_MACHINE\\...\\0002\r\n    DriverDesc    REG_SZ    RDPDD Chained DD\r\n    DriverDesc    REG_SZ    Mirage Mirror Driver\r\n";
        assert_eq!(
            super::parse(out),
            [
                "NVIDIA GeForce 9600 GT",
                "Intel(R) HD Graphics",
                "RDPDD Chained DD"
            ]
        );
    }
}
