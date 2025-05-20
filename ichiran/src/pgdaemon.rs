use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use tokio::process::{Child, Command};
use win32job::{Job, JobError};

use crate::ConnParams;

pub struct PostgresDaemon {
    pg_bin_dir: PathBuf,
    data_path: PathBuf,
    pg_proc: Result<Child, std::io::Error>,
    silent: bool,
    _job_obj: Job,
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

        let job = Self::create_job_object().expect("failed to create job object");

        tracing::info!(?pg_bin_dir, ?data_path, "starting");
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
                tracing::info!(pid = ?proc.id(), "started");
            }
            Err(err) => {
                tracing::warn!(%err, "start failed");
            }
        }

        PostgresDaemon {
            pg_bin_dir,
            data_path,
            pg_proc: proc,
            silent,
            _job_obj: job,
        }
    }
    fn pg_bin_path(pg_bin_dir: impl AsRef<Path>, name: impl Into<PathBuf>) -> PathBuf {
        let mut bin = name.into();
        bin.set_extension(std::env::consts::EXE_EXTENSION);
        pg_bin_dir.as_ref().join(bin)
    }
    fn create_job_object() -> Result<Job, JobError> {
        let job = Job::create()?;
        let mut info = job.query_extended_limit_info()?;
        info.limit_kill_on_job_close();
        job.set_extended_limit_info(&info)?;
        job.assign_current_process()?;
        Ok(job)
    }
}
impl Drop for PostgresDaemon {
    fn drop(&mut self) {
        if let Ok(pg_proc) = &mut self.pg_proc {
            match pg_proc.try_wait() {
                Ok(Some(status)) => tracing::warn!(?status, "exited"),
                Ok(None) => {
                    tracing::info!(pid = ?pg_proc.id(), "stopping");
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
                            futures::executor::block_on(fut).unwrap();
                            tracing::info!("stopped");
                        }
                        Err(err) => {
                            tracing::warn!(%err, "stop failed")
                        }
                    }
                }
                Err(err) => tracing::error!(%err, "wait failed"),
            }
        }
    }
}
