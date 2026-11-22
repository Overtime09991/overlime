// Linkt den chiaki-Kern (C) und seine Bausteine statisch in overlime.exe.
fn main() {
    let build = std::env::var("OVERLIME_NATIVE_BUILD").unwrap_or_else(|_| "C:/dev/build".into());
    let vcpkg = std::env::var("OVERLIME_VCPKG_LIB")
        .unwrap_or_else(|_| "C:/vcpkg/installed/x64-windows-static-md/lib".into());

    for dir in [
        build.clone(),
        format!("{build}/lib"),
        format!("{build}/third-party"),
        format!("{build}/third-party/nanopb"),
        vcpkg,
    ] {
        println!("cargo:rustc-link-search=native={dir}");
    }

    for lib in [
        "overlime_core", "chiaki", "protobuf-nanopb", "jerasure", "gf_complete",
        "avformat", "avcodec", "swscale", "swresample", "avutil", "opus",
        "libcurl", "libssl", "libcrypto", "libzs", "json-c", "event", "event_core", "event_extra", "libminiupnpc",
    ] {
        println!("cargo:rustc-link-lib=static={lib}");
    }
    for sys in [
        "ws2_32", "crypt32", "bcrypt", "user32", "advapi32", "secur32", "ncrypt", "ole32", "oleaut32",
        "mfplat", "mfuuid", "strmiids", "d3d11", "dxgi", "iphlpapi", "shell32", "gdi32", "wldap32",
        "normaliz", "psapi", "shlwapi", "uuid", "comdlg32", "winmm", "mf", "mfreadwrite", "vfw32",
    ] {
        println!("cargo:rustc-link-lib={sys}");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
