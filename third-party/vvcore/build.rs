fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!(
        r"cargo:rustc-link-search=native={}\..\..\third-party\vvcore\lib",
        dir
    );
    println!("cargo:rustc-link-lib=voicevox_core");
}
