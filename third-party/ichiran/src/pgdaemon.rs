use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use tokio::process::{Child, Command};

use crate::ConnParams;

pub struct PostgresDaemon {
    pg_bin_dir: PathBuf,
    data_path: PathBuf,
    pg_proc: Result<Child, std::io::Error>,
    silent: bool,
}
impl PostgresDaemon {
    pub fn new(
        pg_bin_dir: impl Into<PathBuf>,
        data_path: impl Into<PathBuf>,
        conn_params: ConnParams,
        silent: bool,
    ) -> Self {
        let pg_bin_dir = pg_bin_dir.into();
        let data_path = data_path.into();

        log::info!(
            "starting pg daemon in {:?} at {:?}",
            &pg_bin_dir,
            &data_path
        );
        let postgres_bin_path = Self::pg_bin_path(&pg_bin_dir, "postgres");

        let mut proc = Command::new(postgres_bin_path);
        let mut proc = proc
            .kill_on_drop(true)
            .args(["-p", &format!("{}", conn_params.port)])
            .arg("-D")
            .arg(&data_path);
        if silent {
            proc = proc.stdout(Stdio::null()).stderr(Stdio::null());
        }
        let proc = proc.spawn();

        match &proc {
            Ok(proc) => {
                log::info!("started pg daemon w/ pid {:?}", proc.id());
            }
            Err(err) => {
                log::warn!("failed to start pg daemon: {}", err)
            }
        }

        PostgresDaemon {
            pg_bin_dir,
            data_path,
            pg_proc: proc,
            silent,
        }
    }
    fn pg_bin_path(pg_bin_dir: impl AsRef<Path>, name: impl Into<PathBuf>) -> PathBuf {
        let mut bin = name.into();
        bin.set_extension(std::env::consts::EXE_EXTENSION);
        pg_bin_dir.as_ref().join(bin)
    }
}
impl Drop for PostgresDaemon {
    fn drop(&mut self) {
        match &mut self.pg_proc {
            Ok(pg_proc) => match pg_proc.try_wait() {
                Ok(Some(status)) => log::warn!("pg daemon already exited with: {}", status),
                Ok(None) => {
                    log::info!("stopping pg daemon w/ pid {:?}", pg_proc.id());
                    let pgctl_bin_path = Self::pg_bin_path(&self.pg_bin_dir, "pg_ctl");

                    let mut pgctl_proc = Command::new(pgctl_bin_path);
                    let mut pgctl_proc = pgctl_proc
                        .arg("--wait")
                        .arg("-D")
                        .arg(&self.data_path)
                        .arg("stop");
                    if self.silent {
                        pgctl_proc = pgctl_proc.stdout(Stdio::null()).stderr(Stdio::null());
                    }
                    let pgctl_proc = pgctl_proc.spawn();

                    match pgctl_proc {
                        Ok(mut pgctl_proc) => {
                            let fut = pgctl_proc.wait();
                            // there might not be a live executor at this point
                            let _ = match tokio::runtime::Handle::try_current() {
                                Ok(handle) => handle.block_on(fut),
                                Err(_) => futures::executor::block_on(fut),
                            };
                            log::info!("stopped pg daemon");
                        }
                        Err(err) => {
                            log::warn!("failed to stop pg daemon: {}", err)
                        }
                    }
                }
                Err(e) => log::error!("failed to wait pg daemon: {}", e),
            },
            Err(_) => (),
        }
    }
}
