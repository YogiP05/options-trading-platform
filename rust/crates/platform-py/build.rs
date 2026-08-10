//! Linker configuration for a loadable Python extension.

fn main() {
    if std::env::var_os("CARGO_FEATURE_EXTENSION_MODULE").is_some() {
        pyo3_build_config::add_extension_module_link_args();
        return;
    }

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("Cargo provides target OS");
    let target_family =
        std::env::var("CARGO_CFG_TARGET_FAMILY").expect("Cargo provides target family");
    assert!(!target_os.is_empty(), "Cargo target OS must not be empty");
    if target_family.split(',').any(|family| family == "unix") {
        if let Some(library_dir) = pyo3_build_config::get().lib_dir() {
            println!("cargo:rustc-link-arg=-Wl,-rpath");
            println!("cargo:rustc-link-arg=-Wl,{library_dir}");
        }
    }
}
