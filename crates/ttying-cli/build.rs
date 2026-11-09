fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/ttying.ico");
    res.set("FileDescription", "ttying web terminal");
    res.set("ProductName", "ttying");
    res.set("LegalCopyright", "MIT");
    res.compile().expect("compile Windows icon resources");
}
