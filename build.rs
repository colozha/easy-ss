use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=icons");
    let target = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("icons.gresource");
    let status = Command::new("glib-compile-resources")
        .args(["--sourcedir=icons", "icons/resources.xml", "--target"])
        .arg(target)
        .status()
        .expect("glib-compile-resources");
    assert!(status.success(), "Could not compile toolbar icons");
}
