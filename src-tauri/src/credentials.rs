const SERVICE: &str = "DelegateControl";
const ACCOUNT: &str = "RuntimeApiKey";

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|error| format!("无法访问 Windows 安全存储：{error}"))
}

pub fn credential_exists() -> bool {
    read_runtime_key().is_ok()
}

pub fn write_runtime_key(key: &str) -> Result<(), String> {
    let trimmed = key.trim();
    if trimmed.len() < 8 {
        return Err("Runtime API Key 为空或过短。".to_string());
    }
    entry()?.set_password(trimmed).map_err(|error| format!("无法写入 Windows 安全存储：{error}"))
}

pub fn read_runtime_key() -> Result<String, String> {
    entry()?.get_password().map_err(|_| "尚未配置 Runtime API Key。".to_string())
}
