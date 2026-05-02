//! Persistent ichiran-cli worker pool.
//!
//! Each worker owns a long-lived `ichiran-cli -e <SERVER_LOOP>` process. Requests are
//! base64-line framed (one line per direction); each worker handles one request at a
//! time, so the protocol does not need request ids. The pool fans concurrent requests
//! across workers via a shared mpmc queue.

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Instant,
};

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::{mpsc, oneshot, Mutex},
};
use tracing::{Instrument, Level};

use crate::error::IchiranError;

/// Truncate a sexp for log output so we don't spam the trace with
/// multi-kilobyte JSON payloads.
fn sexp_head(s: &str) -> String {
    const LIMIT: usize = 80;
    let mut head: String = s.chars().take(LIMIT).collect();
    if s.chars().count() > LIMIT {
        head.push_str("...");
    }
    head
}

// Capture *standard-output* during eval and return it as the response payload.
// All callers must write their result via `(princ ...)` / `(format t ...)`;
// the value returned by the form is discarded.
const SERVER_LOOP: &str = r#"(progn
  (write-line "READY")
  (force-output)
  (loop for line = (read-line *standard-input* nil nil)
        while line
        do (let ((form-str (sb-ext:octets-to-string
                             (cl-base64:base64-string-to-usb8-array line)
                             :external-format :utf-8)))
             (handler-case
                 (let* ((buf (make-string-output-stream))
                        (out (progn
                               (let ((*standard-output* buf))
                                 (eval (read-from-string form-str)))
                               (get-output-stream-string buf)))
                        (out64 (cl-base64:usb8-array-to-base64-string
                                 (sb-ext:string-to-octets out :external-format :utf-8))))
                   (format t "ok ~A~%" out64))
               (error (e)
                 (let ((msg64 (cl-base64:usb8-array-to-base64-string
                                (sb-ext:string-to-octets (prin1-to-string e) :external-format :utf-8))))
                   (format t "err ~A~%" msg64))))
             (force-output))))"#;

type Reply = oneshot::Sender<Result<String, IchiranError>>;

struct Request {
    sexp: String,
    reply: Reply,
    /// Span representing the caller's `evaluate` invocation. The worker enters
    /// it while servicing the request so `lisp.write`, `lisp.read` etc. show
    /// up nested under the original call site.
    span: tracing::Span,
    /// When the request was enqueued, used to log queue wait time.
    enqueued_at: Instant,
}

type WorkQueue = Arc<Mutex<mpsc::Receiver<Request>>>;

pub struct IchiranPool {
    tx: mpsc::Sender<Request>,
}

impl IchiranPool {
    #[tracing::instrument(level = Level::INFO, skip_all, fields(?path, size), err)]
    pub async fn spawn(path: &Path, size: usize) -> Result<Self, IchiranError> {
        assert!(size >= 1, "pool size must be >= 1");
        path.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Could not find working directory of ichiran-cli",
            )
        })?;

        let (tx, rx) = mpsc::channel::<Request>(size * 4);
        let queue: WorkQueue = Arc::new(Mutex::new(rx));

        for worker_id in 0..size {
            spawn_worker(worker_id, path.to_owned(), queue.clone()).await?;
        }

        tracing::info!(size, "ichiran-cli pool ready");
        Ok(Self { tx })
    }

    #[tracing::instrument(level = Level::DEBUG, skip_all, fields(sexp = %sexp_head(&sexp)), err)]
    pub async fn evaluate(&self, sexp: String) -> Result<String, IchiranError> {
        tracing::trace!(%sexp, "evaluate");
        let (reply_tx, reply_rx) = oneshot::channel();
        let span = tracing::Span::current();
        let enqueued_at = Instant::now();
        self.tx
            .send(Request {
                sexp,
                reply: reply_tx,
                span,
                enqueued_at,
            })
            .await
            .map_err(|_| IchiranError::ServerGone)?;
        reply_rx.await.map_err(|_| IchiranError::ServerGone)?
    }
}

async fn spawn_worker(
    worker_id: usize,
    path: PathBuf,
    queue: WorkQueue,
) -> Result<(), IchiranError> {
    let working_dir = path.parent().unwrap();
    let mut child = Command::new(&path)
        .current_dir(working_dir)
        .arg("-e")
        .arg(SERVER_LOOP)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    // Wait for READY handshake. Skip any preamble lines (banner, NIL, etc).
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Err(IchiranError::Server(format!(
                "ichiran-cli worker {worker_id} exited before READY"
            )));
        }
        if line.trim_end() == "READY" {
            break;
        }
        tracing::debug!(worker_id, line = %line.trim_end(), "ichiran-cli preamble");
    }

    tracing::info!(worker_id, pid = ?child.id(), "ichiran-cli worker ready");

    tokio::spawn(worker_loop(worker_id, child, stdin, reader, queue));
    Ok(())
}

async fn worker_loop(
    worker_id: usize,
    mut child: tokio::process::Child,
    mut stdin: tokio::process::ChildStdin,
    mut reader: BufReader<tokio::process::ChildStdout>,
    queue: WorkQueue,
) {
    loop {
        // Hold the lock only long enough to pop one request, then release before
        // processing so other workers can grab the next one.
        let req = {
            let mut rx = queue.lock().await;
            match rx.recv().await {
                Some(r) => r,
                None => break,
            }
        };

        // Service the request inside the caller's span so events nest under
        // the original `evaluate` call.
        let parent_span = req.span.clone();
        let queue_wait_ms = req.enqueued_at.elapsed().as_millis() as u64;
        let result = handle_request(&mut stdin, &mut reader, &req.sexp, worker_id, queue_wait_ms)
            .instrument(parent_span)
            .await;

        // Decide whether to bail before consuming the result via send().
        let fatal = matches!(result, Err(IchiranError::ServerGone));
        if req.reply.send(result).is_err() {
            tracing::debug!(worker_id, "request dropped before reply");
        }
        if fatal {
            break;
        }
    }

    tracing::warn!(worker_id, "ichiran-cli worker loop exiting");
    let _ = child.start_kill();
}

async fn handle_request(
    stdin: &mut tokio::process::ChildStdin,
    reader: &mut BufReader<tokio::process::ChildStdout>,
    sexp: &str,
    worker_id: usize,
    queue_wait_ms: u64,
) -> Result<String, IchiranError> {
    let b64 = B64.encode(sexp.as_bytes());
    let mut frame = b64.into_bytes();
    frame.push(b'\n');

    let lisp_start = Instant::now();
    stdin.write_all(&frame).await?;
    stdin.flush().await?;

    let mut line = String::new();
    let n = reader.read_line(&mut line).await?;
    if n == 0 {
        return Err(IchiranError::ServerGone);
    }
    let lisp_ms = lisp_start.elapsed().as_millis() as u64;
    tracing::debug!(
        worker_id,
        lisp_ms,
        queue_wait_ms,
        bytes_in = frame.len(),
        bytes_out = n,
        "ichiran.call"
    );

    parse_response(line.trim_end())
}

fn parse_response(line: &str) -> Result<String, IchiranError> {
    let (tag, b64) = line
        .split_once(' ')
        .ok_or_else(|| IchiranError::Server(format!("malformed frame: {line}")))?;
    let bytes = B64
        .decode(b64)
        .map_err(|e| IchiranError::Server(format!("invalid base64: {e}")))?;
    let payload = String::from_utf8(bytes)
        .map_err(|e| IchiranError::Server(format!("invalid utf-8: {e}")))?;
    match tag {
        "ok" => Ok(payload),
        "err" => Err(IchiranError::Server(payload)),
        other => Err(IchiranError::Server(format!("unknown tag: {other}"))),
    }
}
