fn main() {
    println!("cargo:rerun-if-changed=icon/icon.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("icon/icon.ico")
            .set("ProductName", "Easy Media Compress")
            .compile()
            .expect("embed Windows icon");
    }
}
