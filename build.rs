fn main() {
    println!("cargo:rerun-if-changed=resources/vt-tray.ico");
    println!("cargo:rerun-if-changed=resources/vt-tray.rc");
    println!("cargo:rerun-if-changed=resources/app.manifest");
    if std::env::var_os("VTRAY_SKIP_RESOURCE").is_none() {
        let _ = embed_resource::compile("resources/vt-tray.rc", embed_resource::NONE);
    }
}
