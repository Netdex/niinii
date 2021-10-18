use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::{
    job::{self, JobObject},
    ConnParams,
};

pub struct PostgresDaemon {
    pg_bin_dir: PathBuf,
    data_path: PathBuf,
    pg_proc: Result<std::process::Child, std::io::Error>,
    _job_obj: Option<JobObject>,
    silent: bool,
}
impl PostgresDaemon {
    pub fn new<N: Into<PathBuf>, M: Into<PathBuf>>(
        pg_bin_dir: N,
        data_path: M,
        conn_params: ConnParams,
        silent: bool,
    ) -> Self {
        let pg_bin_dir = pg_bin_dir.into();
        let data_path = data_path.into();

        let job_obj = job::setup();
        log::info!(
            "starting pg daemon in {:?} at {:?}",
            &pg_bin_dir,
            &data_path
        );
        let postgres_bin_path = Self::pg_bin_path(&pg_bin_dir, "postgres");

        let mut proc = Command::new(postgres_bin_path);
        let mut proc = proc
            .args(["-p", &format!("{}", conn_params.port)])
            .arg("-D")
            .arg(&data_path);
        if silent {
            proc = proc.stdout(Stdio::null()).stderr(Stdio::null());
        }
        let proc = proc.spawn();

        match &proc {
            Ok(proc) => {
                log::info!("started pg daemon w/ pid {}", proc.id());
            }
            Err(err) => {
                log::warn!("failed to start pg daemon: {}", err)
            }
        }

        PostgresDaemon {
            pg_bin_dir,
            data_path,
            pg_proc: proc,
            _job_obj: job_obj,
            silent,
        }
    }
    fn pg_bin_path<N: AsRef<Path>, M: Into<PathBuf>>(pg_bin_dir: N, name: M) -> PathBuf {
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
                    log::info!("stopping pg daemon w/ pid {}", pg_proc.id());
                    let pgctl_bin_path = Self::pg_bin_path(&self.pg_bin_dir, "pg_ctl");

                    let mut pgctl_proc = Command::new(pgctl_bin_path);
                    let mut pgctl_proc = pgctl_proc.arg("-D").arg(&self.data_path).arg("stop");
                    if self.silent {
                        pgctl_proc = pgctl_proc.stdout(Stdio::null()).stderr(Stdio::null());
                    }
                    let pgctl_proc = pgctl_proc.spawn();

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
