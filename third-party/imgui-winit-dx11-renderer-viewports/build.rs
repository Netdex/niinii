//! Compiles the HLSL shaders to DXBC at build time so the crate does not need
//! d3dcompiler at runtime.

use std::{env, ffi::CStr, fs, ptr, slice};

use winapi::{
    shared::winerror::S_OK,
    um::{d3dcommon::ID3DBlob, d3dcompiler::D3DCompile},
};

const SHADERS: &[(&str, &str, &CStr)] = &[
    ("src/shader/vertex.hlsl", "vertex.dxbc", c"vs_4_0"),
    ("src/shader/pixel.hlsl", "pixel.dxbc", c"ps_4_0"),
];

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    for (src_path, out_name, target) in SHADERS {
        println!("cargo:rerun-if-changed={src_path}");
        let source = fs::read_to_string(src_path).unwrap();
        let bytecode = unsafe { compile(&source, target) }
            .unwrap_or_else(|err| panic!("failed to compile {src_path}: {err}"));
        fs::write(format!("{out_dir}/{out_name}"), bytecode).unwrap();
    }
}

unsafe fn compile(source: &str, target: &CStr) -> Result<Vec<u8>, String> {
    let mut blob: *mut ID3DBlob = ptr::null_mut();
    let mut err: *mut ID3DBlob = ptr::null_mut();
    let hr = D3DCompile(
        source.as_ptr().cast(),
        source.len(),
        ptr::null(),
        ptr::null(),
        ptr::null_mut(),
        c"main".as_ptr(),
        target.as_ptr(),
        0,
        0,
        &mut blob,
        &mut err,
    );
    let result = if hr == S_OK && !blob.is_null() {
        Ok(blob_bytes(&*blob).to_vec())
    } else if !err.is_null() {
        Err(String::from_utf8_lossy(blob_bytes(&*err)).into_owned())
    } else {
        Err(format!("HRESULT {hr:#010x}"))
    };
    if !blob.is_null() {
        (*blob).Release();
    }
    if !err.is_null() {
        (*err).Release();
    }
    result
}

unsafe fn blob_bytes(blob: &ID3DBlob) -> &[u8] {
    slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize())
}
