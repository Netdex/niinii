use std::path::PathBuf;

use crate::pgdaemon::PostgresDaemon;

use super::*;

pub fn ichiran() -> (Ichiran, PostgresDaemon) {
    let ichiran_path =
        PathBuf::from("../../compat/ichiran-cli").with_extension(std::env::consts::EXE_EXTENSION);
    let ichiran = Ichiran::new(ichiran_path);
    let conn_params = ichiran.conn_params().unwrap();
    let pgdaemon = PostgresDaemon::new(
        "../../compat/pgsql/bin",
        "../../compat/pgsql/data",
        conn_params,
        true,
    );
    (ichiran, pgdaemon)
}
