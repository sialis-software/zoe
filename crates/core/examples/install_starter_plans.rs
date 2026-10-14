//! Writes the starter reading plans into a vault's `library/` folder, as a new vault gets them.
//! Files that are already there are left as they are (a plan that was started keeps its start date).
//! Usage: `cargo run -q -p study-core --example install_starter_plans -- <vault folder>`

use std::path::Path;
use study_core::starter_plans::starter_plans;

fn main() {
    let Some(vault) = std::env::args().nth(1) else {
        eprintln!("Usage: install_starter_plans <vault folder>");
        std::process::exit(2);
    };
    let vault = Path::new(&vault);
    if !vault.is_dir() {
        eprintln!("{} is not a folder", vault.display());
        std::process::exit(1);
    }
    for plan in starter_plans() {
        let (mut written, mut kept) = (0, 0);
        for (path, text) in plan.to_files() {
            let target = vault.join(&path);
            if target.exists() {
                kept += 1;
                continue;
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).expect("make the plan's folder");
            }
            std::fs::write(&target, text).expect("write a plan note");
            written += 1;
        }
        println!("{}: {written} notes written, {kept} already there", plan.name);
    }
}
