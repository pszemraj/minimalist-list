//! Embeds the application icon in Windows executables and their shell shortcuts.

fn main() {
    println!("cargo:rerun-if-changed=assets/minimalist-list.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/minimalist-list.ico")
            .set("ProductName", "Minimalist List")
            .set("FileDescription", "Minimalist List")
            .compile()
            .expect("could not embed the Windows application icon");
    }
}
