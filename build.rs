fn main() {
    println!("cargo:rerun-if-changed=packaging/windows/rhythia-is-trash.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("packaging/windows/rhythia-is-trash.ico");
        res.set("ProductName", "rhythia is trash");
        res.set("FileDescription", "rhythia is trash");
        res.compile().expect("embedding the Windows icon failed");
    }
}
