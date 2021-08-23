use std::{env, path::PathBuf};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let bindings = bindgen::Builder::default()
        .header("src/wrapper.h")
        .header("imgui/backends/imgui_impl_win32.h")
        .clang_args(["-I", "imgui"])
        .clang_args(["-x", "c++"])
        .whitelist_function("ImGui_ImplWin32_.*")
        .blacklist_type("LPARAM") // ugh
        .blacklist_type("LRESULT")
        .blacklist_type("UINT")
        .blacklist_type("WPARAM")
        .blacklist_type("HWND")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("unable to generate bindings");
    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("could not write bindings");

    cc::Build::new()
        .include("imgui")
        .file("imgui/backends/imgui_impl_win32.cpp")
        .compile("imgui-win32-sys");
}
