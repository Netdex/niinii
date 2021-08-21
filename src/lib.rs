use {
    std::thread,
    winapi::{
        shared::minwindef::{BOOL, DWORD, HINSTANCE, LPVOID, TRUE},
        um::{
            consoleapi::AllocConsole, libloaderapi::DisableThreadLibraryCalls,
            winnt::DLL_PROCESS_ATTACH,
        },
    },
};

unsafe fn main() {
    // let result = d3d9_util::get_d3d9_vtable();
    // match result {
    //     Ok(v) => {
    //         println!("d3d9Device[42]: {:p}", *v.get(42).unwrap());
    //         hook::hook_functions(v);
    //     }
    //     Err(s) => println!("Error finding vtable addresses: {}", s),
    // }
}

#[no_mangle]
pub extern "stdcall" fn DllMain(h_inst: HINSTANCE, fdw_reason: DWORD, _: LPVOID) -> BOOL {
    if fdw_reason == DLL_PROCESS_ATTACH {
        unsafe {
            DisableThreadLibraryCalls(h_inst);
            AllocConsole();
        };
        thread::spawn(|| unsafe { main() });
    }
    TRUE
}
