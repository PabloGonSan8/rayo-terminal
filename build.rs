#[path = "src/icon.rs"]
mod icon;

fn main() {
    println!("cargo:rerun-if-changed=src/icon.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // Embed the icon so Explorer, the Start menu and a pinned taskbar entry show it.
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico = out.join("rayo.ico");
    std::fs::write(&ico, icon::ico()).unwrap();
    let rc = out.join("rayo.rc");
    // Version resource: Explorer's Properties → Details, WinGet and antivirus heuristics read it.
    let v = env!("CARGO_PKG_VERSION");
    let n: Vec<u32> = v.split(['.', '-']).take(3).map(|p| p.parse().unwrap_or(0)).collect();
    let (a, b, c) = (n[0], n.get(1).copied().unwrap_or(0), n.get(2).copied().unwrap_or(0));
    std::fs::write(
        &rc,
        format!(
            r#"1 ICON "{ico}"
1 VERSIONINFO
FILEVERSION {a},{b},{c},0
PRODUCTVERSION {a},{b},{c},0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "Rayo contributors"
      VALUE "FileDescription", "Rayo terminal"
      VALUE "FileVersion", "{v}"
      VALUE "InternalName", "rayo"
      VALUE "LegalCopyright", "MIT License"
      VALUE "OriginalFilename", "rayo.exe"
      VALUE "ProductName", "Rayo"
      VALUE "ProductVersion", "{v}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
            ico = ico.display().to_string().replace('\\', "\\\\")
        ),
    )
    .unwrap();
    embed_resource::compile(&rc, embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
