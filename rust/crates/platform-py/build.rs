//! Linker configuration for a loadable Python extension.

fn main() {
    if std::env::var_os("CARGO_FEATURE_EXTENSION_MODULE").is_some() {
        pyo3_build_config::add_extension_module_link_args();
    } else if cfg!(target_os = "macos") {
        if let Some(library_dir) = pyo3_build_config::get().lib_dir() {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{library_dir}");
        }
    }
}
