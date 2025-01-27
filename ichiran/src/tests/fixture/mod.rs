use std::path::PathBuf;

use crate::Ichiran;

pub async fn ichiran() -> Ichiran {
    let ichiran_path =
        PathBuf::from("../data/ichiran-cli").with_extension(std::env::consts::EXE_EXTENSION);

    Ichiran::new(ichiran_path)
}
