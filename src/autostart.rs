use std::path::Path;
use winreg::enums::*;
use winreg::RegKey;

#[allow(dead_code)]
pub(crate) fn register_autostart() {
    // let key = r#"HKEY_CURRENT_USER\SOFTWARE\Microsoft\Windows\CurrentVersion\Run"#;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = Path::new("Software")
        .join("Microsoft")
        .join("Windows")
        .join("CurrentVersion")
        .join("Run");
    let (key, disp) = hkcu.create_subkey(path).unwrap();
    dbg!(&disp);
    key.set_value("Path", &"written by Rust").unwrap();
}
