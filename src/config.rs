use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const PROFILE_NAMES: [&str; 3] = [
    "TLS · Multisplit",
    "TLS · Multidisorder",
    "TLS · Fake + split",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub schema: u32,
    pub profile: usize,
    pub domains: String,
    pub exclusions: String,
    pub probe: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            schema: 1,
            profile: 0,
            domains: "youtube.com\ngooglevideo.com\nytimg.com".into(),
            exclusions: String::new(),
            probe: "www.youtube.com".into(),
        }
    }
}
pub fn normalize_domains(input: &str, allow_empty: bool) -> Result<String, String> {
    if input.len() > 8192 {
        return Err("Danh sách quá dài (tối đa 8 KB).".into());
    }
    let mut output = Vec::new();
    for line in input.lines() {
        let value = line.trim().trim_end_matches('.').to_ascii_lowercase();
        if value.is_empty() {
            continue;
        }
        if value.len() > 253
            || !value.contains('.')
            || value.parse::<std::net::IpAddr>().is_ok()
            || value.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || label.starts_with('-')
                    || label.ends_with('-')
                    || !label
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            })
        {
            return Err(format!(
                "Tên miền không hợp lệ: {value}. Nhập domain, không nhập URL hoặc ký tự đại diện."
            ));
        }
        if !output.contains(&value) {
            output.push(value);
        }
    }
    if !allow_empty && output.is_empty() {
        return Err("Cần ít nhất một tên miền để giới hạn phạm vi xử lý.".into());
    }
    Ok(output.join("\n"))
}
impl Settings {
    pub fn validated(&self) -> Result<Self, String> {
        if self.schema != 1 {
            return Err("Phiên bản cấu hình chưa được hỗ trợ.".into());
        }
        if self.profile >= PROFILE_NAMES.len() {
            return Err("Preset không tồn tại.".into());
        }
        let domains = normalize_domains(&self.domains, false)?;
        let exclusions = normalize_domains(&self.exclusions, true)?;
        let probe = normalize_domains(&self.probe, false)?;
        if probe.lines().count() != 1 {
            return Err("Chỉ nhập một tên miền kiểm tra.".into());
        }
        Ok(Self {
            domains,
            exclusions,
            probe,
            ..self.clone()
        })
    }
}
pub fn data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Umbra")
}
pub fn load(path: &Path) -> Result<Settings, String> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let settings: Settings =
        toml::from_str(&text).map_err(|e| format!("Không đọc được cấu hình: {e}"))?;
    settings.validated()
}
pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    let settings = settings.validated()?;
    fs::create_dir_all(path.parent().ok_or("Đường dẫn cấu hình không hợp lệ")?)
        .map_err(|e| e.to_string())?;
    let temp = path.with_extension("tmp");
    fs::write(
        &temp,
        toml::to_string_pretty(&settings).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    // MoveFileEx replaces atomically on Windows; never truncate the active configuration.
    crate::platform::replace_file(&temp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_and_deduplicates() {
        assert_eq!(
            normalize_domains(" YouTube.COM.\nyoutube.com\ncdn.example.com ", false).unwrap(),
            "youtube.com\ncdn.example.com"
        );
    }
    #[test]
    fn rejects_injection_and_urls() {
        for domain in [
            "--lua-init=x",
            "https://example.com",
            "a.com & calc",
            "*.com",
            "a..com",
            "a.-b.com",
            "127.0.0.1",
            "a.com\r--new",
            "",
        ] {
            assert!(normalize_domains(domain, false).is_err(), "{domain}");
        }
    }
    #[test]
    fn refuses_unknown_schema_or_preset() {
        let mut s = Settings {
            schema: 2,
            ..Settings::default()
        };
        assert!(s.validated().is_err());
        s.schema = 1;
        s.profile = 99;
        assert!(s.validated().is_err());
    }
    #[test]
    fn roundtrip() {
        let s = Settings::default();
        let decoded: Settings = toml::from_str(&toml::to_string(&s).unwrap()).unwrap();
        assert_eq!(s, decoded.validated().unwrap());
    }
    #[test]
    fn atomic_save_preserves_previous_on_invalid_input() {
        let mut nonce = [0u8; 8];
        getrandom::fill(&mut nonce).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "umbra-settings-test-{:x}",
            u64::from_le_bytes(nonce)
        ));
        let file = dir.join("settings.toml");
        let original = Settings::default();
        save(&file, &original).unwrap();
        let invalid = Settings {
            domains: "--new".into(),
            ..original.clone()
        };
        assert!(save(&file, &invalid).is_err());
        assert_eq!(load(&file).unwrap(), original);
        let next = Settings {
            profile: 1,
            ..original
        };
        save(&file, &next).unwrap();
        assert_eq!(load(&file).unwrap(), next);
        fs::remove_file(&file).unwrap();
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn bounds_command_line_input() {
        assert!(normalize_domains(&"a.example.com\n".repeat(1000), false).is_err());
        let settings = Settings {
            probe: "example.com\nexample.org".into(),
            ..Settings::default()
        };
        assert!(settings.validated().is_err());
    }
}
