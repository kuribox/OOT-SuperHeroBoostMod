//! The identity of the executable: its icon, its name and its version, inside the file's properties.
//!
//! Windows reads them from a resource compiled into the `.exe`: without it the file shows the generic icon and no
//! version, and "SHB-OcarinaTweaks" in Explorer is only the file name. The icon is the Python app's own
//! (`assets/icon.ico`, the same picture the window shows), so both apps look alike in the taskbar and in a shortcut.
//!
//! The resource is compiled with `llvm-rc` — the LLVM toolchain this workspace already builds with — and `windres`
//! is the fallback. Neither being there must not break the build: it only costs the icon.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let icon = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest"))
        .join("assets")
        .join("icon.ico");
    if !icon.is_file() {
        return;
    }
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let parts: Vec<u32> = version
        .split('.')
        .map(|piece| piece.parse().unwrap_or(0))
        .collect();
    let quad = format!(
        "{},{},{},{}",
        parts.first().copied().unwrap_or(0),
        parts.get(1).copied().unwrap_or(0),
        parts.get(2).copied().unwrap_or(0),
        parts.get(3).copied().unwrap_or(0)
    );
    let rc = out.join("shb.rc");
    let script = format!(
        r#"
1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {quad}
PRODUCTVERSION {quad}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "Kuribox"
      VALUE "FileDescription", "Super Hero Boost - configurador del mod"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "SHB-OcarinaTweaks"
      VALUE "OriginalFilename", "SHB-OcarinaTweaks.exe"
      VALUE "ProductName", "SHB-OcarinaTweaks"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        icon = icon.display().to_string().replace('\\', "/"),
    );
    if std::fs::write(&rc, script).is_err() {
        println!("cargo:warning=the icon resource could not be written");
        return;
    }
    let res = out.join("shb.res");
    let Some(compiler) = find_rc() else {
        println!("cargo:warning=no resource compiler (llvm-rc): the exe has no icon");
        return;
    };
    let status = Command::new(&compiler)
        .arg(format!("/fo{}", res.display()))
        .arg(&rc)
        .status();
    match status {
        Ok(status) if status.success() && res.is_file() => {
            println!("cargo:rustc-link-arg={}", res.display());
        }
        _ => println!("cargo:warning=the icon could not be compiled into the exe"),
    }
}

/// `llvm-rc` in the LLVM install of this machine, or on PATH (the same one `SHB_LLVM` names).
fn find_rc() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    for var in ["SHB_LLVM", "LLVM_HOME"] {
        if let Ok(base) = std::env::var(var) {
            let base = PathBuf::from(base);
            candidates.push(base.join("llvm-rc.exe"));
            candidates.push(base.join("bin").join("llvm-rc.exe"));
        }
    }
    for var in ["ProgramFiles", "ProgramW6432"] {
        if let Ok(base) = std::env::var(var) {
            candidates.push(
                PathBuf::from(base)
                    .join("LLVM")
                    .join("bin")
                    .join("llvm-rc.exe"),
            );
        }
    }
    candidates.push(PathBuf::from(r"C:\Program Files\LLVM\bin\llvm-rc.exe"));
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            candidates.push(dir.join("llvm-rc.exe"));
        }
    }
    candidates.into_iter().find(|candidate| candidate.is_file())
}
