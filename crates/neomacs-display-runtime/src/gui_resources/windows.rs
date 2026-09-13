use neovm_core::emacs_core::display_host::GuiResourceQuery;
use winreg::{HKCU, HKLM, RegKey, enums::REG_SZ, types::FromRegValue};

fn string(key: &RegKey, name: &str) -> Option<String> {
    let value = key.get_raw_value(name).ok()?;
    // The general crate conversion also accepts expanded and multi strings;
    // GNU resources accept only REG_SZ, including a valid empty string.
    if value.vtype != REG_SZ || value.bytes.len() % 2 != 0 {
        return None;
    }
    let mut text = String::from_reg_value(&value).ok()?;
    if let Some(end) = text.find('\0') {
        text.truncate(end);
    }
    Some(text)
}

pub(super) fn query(query: &GuiResourceQuery) -> Option<String> {
    for hive in [&HKCU, &HKLM] {
        if let Ok(key) = hive.open_subkey(r"SOFTWARE\GNU\Emacs") {
            if let Some(value) = string(&key, &query.name).or_else(|| string(&key, &query.class)) {
                return Some(value);
            }
        }
    }
    let defaults = [
        ("emacs.foreground", "SystemWindowText"),
        ("emacs.background", "SystemWindow"),
        ("emacs.tooltip.attributeForeground", "SystemInfoText"),
        ("emacs.tooltip.attributeBackground", "SystemInfoWindow"),
        ("emacs.tool-bar.attributeForeground", "SystemButtonText"),
        ("emacs.tool-bar.attributeBackground", "SystemButtonFace"),
        ("emacs.tab-bar.attributeForeground", "SystemButtonText"),
        ("emacs.tab-bar.attributeBackground", "SystemButtonFace"),
        ("emacs.menu.attributeForeground", "SystemMenuText"),
        ("emacs.menu.attributeBackground", "SystemMenu"),
        ("emacs.scroll-bar.attributeForeground", "SystemScrollbar"),
    ];
    defaults
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(&query.name))
        .map(|(_, value)| (*value).to_owned())
}
