//! On Windows, put the app's icon and name into the .exe, so the Explorer and the taskbar show them. And on every
//! platform, note which commit the program was built from, for the About page (`ZOE_BUILD`).
use std::process::Command;

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

    // The commit, and a mark if there is work in the tree that it does not have (a release is made from a clean one).
    let git = |args: &[&str]| Command::new("git").args(args).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let build = match git(&["rev-parse", "--short", "HEAD"]) {
        Some(sha) if git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty()) => format!("{sha}+"),
        Some(sha) => sha,
        None => "unknown".into(),
    };
    println!("cargo:rustc-env=ZOE_BUILD={build}");
    // Made again when the checkout moves on (a commit, a switch of branch).
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/index");
}
