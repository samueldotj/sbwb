//! Installed-font check (EXP-06): an unavailable font produces an explicit
//! substitution warning, never a claim of exact typography.

use std::path::PathBuf;

fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(win) = std::env::var_os("WINDIR") {
        dirs.push(PathBuf::from(win).join("Fonts"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local).join("Microsoft/Windows/Fonts"));
    }
    dirs.push(PathBuf::from("/usr/share/fonts"));
    dirs.push(PathBuf::from("/Library/Fonts"));
    dirs
}

fn key(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

/// Whether a font family appears to be installed (file-name match; Word's
/// own substitution table is not consulted).
pub fn is_installed(family: &str) -> bool {
    let want = key(family);
    if want.is_empty() {
        return false;
    }
    for dir in font_dirs() {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_lowercase();
                let is_font =
                    name.ends_with(".ttf") || name.ends_with(".otf") || name.ends_with(".ttc");
                if is_font && key(&name).starts_with(&want) {
                    return true;
                }
            }
        }
    }
    false
}

pub fn warning_for(family: &str) -> Option<String> {
    if is_installed(family) {
        None
    } else {
        Some(format!("“{family}” is not installed on this computer; Word will substitute a font when the document is opened here. The source typography is not reproduced."))
    }
}
