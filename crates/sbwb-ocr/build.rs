// vcpkg's Tesseract port links libarchive and libcurl statically; those need
// Windows system libraries that tesseract-sys does not declare.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        for lib in [
            "xmllite", "iphlpapi", "secur32", "bcrypt", "crypt32", "wldap32", "normaliz", "ws2_32",
        ] {
            println!("cargo:rustc-link-lib={lib}");
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
}
