//! On Windows, put the app's icon and name into the .exe, so the Explorer and the taskbar show them.
fn main() {
    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("../../packaging/icons/zoe.ico");
        resource.set("ProductName", "Zoe");
        resource.set("FileDescription", "Zoe");
        if let Err(e) = resource.compile() {
            println!("cargo:warning=could not add the icon to the program: {e}");
        }
    }
    println!("cargo:rerun-if-changed=../../packaging/icons/zoe.ico");
}
