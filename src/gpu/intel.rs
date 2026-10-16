pub fn name(device: &str) -> Option<&'static str> {
    Some(match device {
        // ...drm.rs の intel_name にあった match の中身を、そのまま...
        "2582" | "2592" => "GMA 900",
        "2772" | "27a2" | "27ae" => "GMA 950",
        "2972" => "GMA 3000",
        "29a2" => "GMA X3000",
        "2a02" => "GMA X3100",
        "2a42" => "GMA 4500MHD",
        "2e12" | "2e22" | "2e32" | "2e42" => "GMA X4500",
        // Ironlake
        "0042" | "0046" => "HD Graphics",
        // Sandy Bridge
        "0102" => "HD Graphics 2000",
        "0112" | "0122" | "0116" | "0126" => "HD Graphics 3000",
        "0106" => "HD Graphics",
        "010a" => "HD Graphics P3000",
        // Ivy Bridge
        "0152" | "0156" => "HD Graphics 2500",
        "0162" | "0166" => "HD Graphics 4000",
        "016a" => "HD Graphics P4000",
        // Haswell
        "0412" | "0416" => "HD Graphics 4600",
        "0a16" => "HD Graphics 4400",
        "0a1e" => "HD Graphics 4200",
        "0a26" => "HD Graphics 5000",
        "0a2e" => "Iris Graphics 5100",
        "0d22" | "0d26" => "Iris Pro Graphics 5200",
        // Broadwell
        "1612" => "HD Graphics 5600",
        "1616" => "HD Graphics 5500",
        "161e" => "HD Graphics 5300",
        "1622" => "Iris Pro Graphics 6200",
        "1626" => "HD Graphics 6000",
        "162b" => "Iris Graphics 6100",
        // Skylake
        "1906" => "HD Graphics 510",
        "1912" | "191b" => "HD Graphics 530",
        "1916" => "HD Graphics 520",
        "191e" => "HD Graphics 515",
        "1926" => "Iris Graphics 540",
        "1927" => "Iris Graphics 550",
        "193b" => "Iris Pro Graphics 580",
        // Kaby Lake
        "5902" | "5906" => "HD Graphics 610",
        "5912" | "591b" => "HD Graphics 630",
        "5916" => "HD Graphics 620",
        "591e" => "HD Graphics 615",
        "5926" => "Iris Plus Graphics 640",
        "5927" => "Iris Plus Graphics 650",
        // Apollo Lake, Gemini Lake
        "5a84" => "HD Graphics 505",
        "5a85" => "HD Graphics 500",
        "3184" => "UHD Graphics 605",
        "3185" => "UHD Graphics 600",
        // Coffee Lake, Whiskey Lake, Amber Lake, Comet Lake
        "3e90" | "3e93" => "UHD Graphics 610",
        "3e91" | "3e92" | "3e98" | "3e9b" => "UHD Graphics 630",
        "3ea0" => "UHD Graphics 620",
        "3ea5" => "Iris Plus Graphics 655",
        "3ea6" => "Iris Plus Graphics 645",
        "87c0" => "UHD Graphics 617",
        "9b41" => "UHD Graphics",
        // Ice Lake, Tiger Lake
        "8a52" | "8a51" => "Iris Plus Graphics G7",
        "8a5a" | "8a5c" => "Iris Plus Graphics G4",
        "8a56" => "UHD Graphics G1",
        "9a49" | "9a40" => "Iris Xe Graphics",
        "9a60" | "9a68" => "UHD Graphics",
        // Alder Lake, Raptor Lake
        "4680" | "4690" | "a780" => "UHD Graphics 770",
        "4682" | "4692" => "UHD Graphics 730",
        "46a6" => "Iris Xe Graphics",
        // Arc
        "56a0" => "Arc A770",
        "56a1" => "Arc A750",
        "56a5" => "Arc A380",
        "56a6" => "Arc A310",
        "5690" => "Arc A770M",
        "5691" => "Arc A730M",
        "5693" => "Arc A370M",
        _ => return None,
    })
}
#[cfg_attr(not(windows), allow(dead_code))]
pub fn is_secondary(device: &str) -> bool {
    matches!(device, "27a6" | "29a3" | "2a03" | "2a43")
}

#[cfg(test)]
mod tests {
    use super::name;

    #[test]
    fn intel_names() {
        assert_eq!(name("3ea0"), Some("UHD Graphics 620"));
        assert_eq!(name("2a42"), Some("GMA 4500MHD"));
        assert_eq!(name("0116"), Some("HD Graphics 3000"));
        assert_eq!(name("ffff"), None);
    }
}
