use std::{mem, ptr};

use detour::{Function, GenericDetour};

use {
    winapi::shared::{d3d9, d3d9types, minwindef::FALSE},
    winapi::{shared::d3d9::LPDIRECT3DDEVICE9, um::winnt::HRESULT},
};

const DIRECTX_VTABLE_SIZE: usize = 119;

pub unsafe fn get_d3d9_vtable() -> Vec<*const usize> {
    let p_d3d = d3d9::Direct3DCreate9(d3d9::D3D_SDK_VERSION);
    if p_d3d.is_null() {
        panic!("Direct3DCreate9 returned null");
    }

    let proc_hwnd = match super::get_arbitrary_hwnd() {
        Some(hwnd) => hwnd,
        _ => panic!("failed to find hwnd"),
    };

    let p_dummy_device: *mut d3d9::IDirect3DDevice9 = ptr::null_mut();
    let mut d3dpp = d3d9types::D3DPRESENT_PARAMETERS {
        BackBufferWidth: 0,
        BackBufferHeight: 0,
        BackBufferFormat: 0,
        BackBufferCount: 0,
        MultiSampleType: 0,
        MultiSampleQuality: 0,
        SwapEffect: d3d9types::D3DSWAPEFFECT_DISCARD,
        hDeviceWindow: proc_hwnd,
        Windowed: FALSE,
        EnableAutoDepthStencil: 0,
        AutoDepthStencilFormat: 0,
        Flags: 0,
        FullScreen_RefreshRateInHz: 0,
        PresentationInterval: 0,
    };

    let mut dummy_device_created = (*p_d3d).CreateDevice(
        d3d9::D3DADAPTER_DEFAULT,
        d3d9types::D3DDEVTYPE_HAL,
        d3dpp.hDeviceWindow,
        d3d9::D3DCREATE_SOFTWARE_VERTEXPROCESSING,
        mem::transmute(&d3dpp),
        mem::transmute(&p_dummy_device),
    );

    if dummy_device_created != 0 {
        d3dpp.Windowed = !d3dpp.Windowed;
        dummy_device_created = (*p_d3d).CreateDevice(
            d3d9::D3DADAPTER_DEFAULT,
            d3d9types::D3DDEVTYPE_HAL,
            d3dpp.hDeviceWindow,
            d3d9::D3DCREATE_SOFTWARE_VERTEXPROCESSING,
            mem::transmute(&d3dpp),
            mem::transmute(&p_dummy_device),
        );
        if dummy_device_created != 0 {
            panic!("failed to create dummy_device");
        }
    }

    let v = std::slice::from_raw_parts(
        (p_dummy_device as *const *const *const usize).read(),
        DIRECTX_VTABLE_SIZE,
    )
    .to_vec();
    if v.is_empty() {
        panic!("failed to dump d3d9 device addresses");
    }
    v
}

pub type EndScene = unsafe extern "stdcall" fn(LPDIRECT3DDEVICE9) -> HRESULT;

pub fn hook<T>(func: T) -> detour::Result<GenericDetour<T>>
where
    T: Function<Arguments = (LPDIRECT3DDEVICE9,), Output = HRESULT>,
{
    unsafe {
        let vtable = get_d3d9_vtable();
        GenericDetour::new(
            Function::from_ptr(std::mem::transmute(*vtable.get(42).unwrap())),
            func,
        )
    }
}
