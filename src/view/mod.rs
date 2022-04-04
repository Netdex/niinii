use std::ffi::c_void;

pub mod deepl;
pub mod kanji;
pub mod mixins;
pub mod raw;
pub mod rikai;
pub mod settings;
pub mod term;

fn id<T>(x: &T) -> *const c_void {
    x as *const _ as *const _
}
