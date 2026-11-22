fn main() {
    // Só o .exe do Windows: ícone e o nome "FeatherDB" (sem o VERSIONINFO o Windows mostra
    // "featherdb.exe" no menu Iniciar, na barra de tarefas e no Gerenciador de Tarefas) e o
    // manifesto (Windows 10/11 e escala por monitor).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let raiz = std::env::var("CARGO_MANIFEST_DIR").unwrap().replace('\\', "/");
    let versao = std::env::var("CARGO_PKG_VERSION").unwrap();
    let numeros: Vec<&str> = versao.split(['.', '-']).take(3).collect();
    let rc = format!(
        r#"1 ICON "{raiz}/../../pacote/icones/featherdb.ico"
1 24 "{raiz}/../../pacote/featherdb.manifest"
1 VERSIONINFO
FILEVERSION {v},0
PRODUCTVERSION {v},0
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "041604B0"
    BEGIN
      VALUE "CompanyName", "FeatherWorks"
      VALUE "FileDescription", "FeatherDB"
      VALUE "FileVersion", "{versao}"
      VALUE "InternalName", "featherdb"
      VALUE "OriginalFilename", "featherdb.exe"
      VALUE "ProductName", "FeatherDB"
      VALUE "ProductVersion", "{versao}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0416, 1200
  END
END
"#,
        v = numeros.join(",")
    );
    let arquivo = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("featherdb.rc");
    std::fs::write(&arquivo, rc).unwrap();
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../pacote/icones/featherdb.ico");
    println!("cargo:rerun-if-changed=../../pacote/featherdb.manifest");
    embed_resource::compile(&arquivo, embed_resource::NONE).manifest_optional().expect("ícone e nome do .exe");
}
