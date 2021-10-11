use std::{
    path::{Path, PathBuf},
    process::Command,
};

use ichiran::ConnParams;

use crate::job::{self, JobObject};

pub struct PostgresDaemon {
    pg_bin_dir: String,
    data_path: String,
    pg_proc: Result<std::process::Child, std::io::Error>,
    _job_obj: Option<JobObject>,
}
impl PostgresDaemon {
    pub fn new(pg_bin_dir: &str, data_path: &str, conn_params: ConnParams) -> Self {
        let job_obj = job::setup();
        log::info!("starting pg daemon in {} at {}", pg_bin_dir, data_path);
        let postgres_bin_path = Self::pg_bin_path(pg_bin_dir, "postgres");
        let proc = Command::new(postgres_bin_path)
            .args(["-D", data_path, "-p", &format!("{}", conn_params.port)])
            .spawn();

        match &proc {
            Ok(proc) => {
                log::info!("started pg daemon w/ pid {}", proc.id());
            }
            Err(err) => {
                log::warn!("failed to start pg daemon: {}", err)
            }
        }

        PostgresDaemon {
            pg_bin_dir: pg_bin_dir.to_owned(),
            data_path: data_path.to_owned(),
            pg_proc: proc,
            _job_obj: job_obj,
        }
    }
    fn pg_bin_path(pg_bin_dir: &str, name: &str) -> PathBuf {
        let mut bin = PathBuf::from(name);
        bin.set_extension(std::env::consts::EXE_EXTENSION);
        Path::new(pg_bin_dir).join(bin)
    }
}
impl Drop for PostgresDaemon {
    fn drop(&mut self) {
        match &mut self.pg_proc {
            Ok(pg_proc) => match pg_proc.try_wait() {
                Ok(Some(status)) => log::warn!("pg daemon already exited with: {}", status),
                Ok(None) => {
                    log::info!("stopping pg daemon w/ pid {}", pg_proc.id());
                    let pgctl_bin_path = Self::pg_bin_path(&self.pg_bin_dir, "pg_ctl");
                    let pgctl_proc = Command::new(pgctl_bin_path)
                        .args(["-D", &self.data_path, "stop"])
                        .spawn();
                    match pgctl_proc {
                        Ok(mut pgctl_proc) => {
                            // TODO: there's a case where we crash after pg is
                            // running but before it's fully up, so pg_ctl will
                            // not stop it.
                            let _ = pgctl_proc.wait();
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
