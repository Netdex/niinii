use std::ffi::c_void;

pub mod index;
pub mod inject;
pub mod kanji;
pub mod mixins;
pub mod raw;
pub mod gloss;
pub mod settings;
pub mod term;
pub mod translator;
pub mod tts;

fn id<T>(x: &T) -> *const c_void {
    x as *const _ as *const _
}
