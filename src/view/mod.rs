use std::ffi::c_void;

mod raw;
mod rikai;
mod settings;

pub use raw::RawView;
pub use rikai::RikaiView;
pub use settings::SettingsView;

fn id<T>(x: &T) -> *const c_void {
    x as *const _ as *const _
}
