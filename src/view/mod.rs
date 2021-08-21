mod raw;
mod rikai;
mod settings;

pub use raw::RawView;
pub use rikai::RikaiView;
pub use settings::SettingsView;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn id<T: Hash>(x: T) -> i32 {
    let mut hasher = DefaultHasher::new();
    x.hash(&mut hasher);
    hasher.finish() as i32
}
