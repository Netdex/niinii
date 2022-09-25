use std::{env, fs, ptr, slice, str};

use winapi::{shared::winerror::S_OK, um::d3dcommon::ID3DBlob, um::d3dcompiler::D3DCompile};

fn main() {
    static VERTEX_SHADER: &str = include_str!("src/vertex_shader.vs_4_0");
    static PIXEL_SHADER: &str = include_str!("src/pixel_shader.ps_4_0");

    let mut err = ptr::null_mut();

    unsafe {
        let mut vs_blob = ptr::null_mut();
        if D3DCompile(
            VERTEX_SHADER.as_ptr().cast(),
            VERTEX_SHADER.len(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            "main\0".as_ptr().cast(),
            "vs_4_0\0".as_ptr().cast(),
            0,
            0,
            &mut vs_blob,
            &mut err,
        ) != S_OK
        {
            report_err(err)
        }
        if let Some(vs_blob) = vs_blob.as_ref() {
            write_blob("vertex_shader.vs_4_0", vs_blob);
        }
    }
    unsafe {
        let mut ps_blob = ptr::null_mut();
        if D3DCompile(
            PIXEL_SHADER.as_ptr().cast(),
            PIXEL_SHADER.len(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            "main\0".as_ptr().cast(),
            "ps_4_0\0".as_ptr().cast(),
            0,
            0,
            &mut ps_blob,
            &mut err,
        ) != S_OK
        {
            report_err(err)
        }
        if let Some(ps_blob) = ps_blob.as_ref() {
            write_blob("pixel_shader.ps_4_0", ps_blob);
        }
    }
}

unsafe fn write_blob(shader_name: &str, blob: &ID3DBlob) {
    let out_dir = env::var("OUT_DIR").unwrap();
    let data = slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize());
    let _ = fs::write(&format!("{}/{}", out_dir, shader_name), data)
        .map_err(|e| panic!("Unable to write {} shader to out dir: {:?}", shader_name, e));
    blob.Release();
}

unsafe fn report_err(err: *const ID3DBlob) -> ! {
    let err_msg = err
        .as_ref()
        .and_then(|err| {
            str::from_utf8(slice::from_raw_parts(
                err.GetBufferPointer().cast::<u8>(),
                err.GetBufferSize(),
            ))
            .ok()
        })
        .map(ToOwned::to_owned);
    err.as_ref().map(|err| err.Release());
    panic!("Failed to compile shader: {}", err_msg.unwrap_or_else(|| String::from("Unknown error")))
}
