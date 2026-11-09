//! Persistent PTY sessions with bounded output replay.
//!
//! A browser WebSocket is only a *view* of a session.  Detaching the socket
//! leaves the PTY running during a short grace period so a refresh, bfcache
//! restore, or transient network failure can reattach to the same shell.  The
//! session is killed when the grace period expires or when the server stops.

use crate::pty::PtySession;
use anyhow::{Context, Result};
use rand::rngs::SysRng;
use rand::TryRng;
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc};

const OUTPUT_HISTORY_BYTES: usize = 512 * 1024;
const OUTPUT_CHANNEL_CAPACITY: usize = 512;
const INPUT_CHANNEL_CAPACITY: usize = 64;
const PTY_READ_BYTES: usize = 8192;
pub const SESSION_GRACE_PERIOD: Duration = Duration::from_secs(300);

// The server already answers ConPTY's initial DSR. Do not replay that query to
// browsers, which would answer it a second time and inject an unsolicited reply.
#[cfg(any(windows, test))]
fn filter_initial_cursor_query(data: Vec<u8>, pending: &mut Vec<u8>, done: &mut bool) -> Vec<u8> {
    if *done {
        return data;
    }
    pending.extend(data);
    if let Some(at) = pending.windows(4).position(|w| w == b"\x1b[6n") {
        pending.drain(at..at + 4);
        *done = true;
        std::mem::take(pending)
    } else {
        let emit = pending.len().saturating_sub(3);
        pending.drain(..emit).collect()
    }
}

#[derive(Debug)]
pub struct OutputChunk {
    pub seq: u64,
    pub data: Vec<u8>,
}

#[derive(Default)]
struct History {
    chunks: VecDeque<Arc<OutputChunk>>,
    bytes: usize,
    last_seq: u64,
}

impl History {
    fn push(&mut self, data: Vec<u8>) -> Arc<OutputChunk> {
        self.last_seq += 1;
        let chunk = Arc::new(OutputChunk {
            seq: self.last_seq,
            data,
        });
        self.bytes = self.bytes.saturating_add(chunk.data.len());
        self.chunks.push_back(chunk.clone());
        while self.bytes > OUTPUT_HISTORY_BYTES {
            let Some(old) = self.chunks.pop_front() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(old.data.len());
        }
        chunk
    }
}

#[derive(Default)]
struct AttachmentState {
    count: usize,
    detached_at: Option<Instant>,
}

pub struct SessionHandle {
    pub id: String,
    pub read_only: bool,
    pty: Mutex<PtySession>,
    input_tx: mpsc::Sender<Vec<u8>>,
    output_tx: broadcast::Sender<Arc<OutputChunk>>,
    history: Arc<Mutex<History>>,
    ended: Arc<AtomicBool>,
    attachment: Mutex<AttachmentState>,
}

impl SessionHandle {
    fn open(command: &[String], cols: u16, rows: u16, read_only: bool) -> Result<Arc<Self>> {
        let mut pty = PtySession::open(command, cols, rows).context("open PTY")?;
        let mut reader = pty.take_reader();
        let mut writer = pty.take_writer();

        let (input_tx, mut input_rx) = mpsc::channel::<Vec<u8>>(INPUT_CHANNEL_CAPACITY);
        let (output_tx, _) = broadcast::channel::<Arc<OutputChunk>>(OUTPUT_CHANNEL_CAPACITY);
        let history = Arc::new(Mutex::new(History::default()));
        let ended = Arc::new(AtomicBool::new(false));

        let session = Arc::new(Self {
            id: random_session_id(),
            read_only,
            pty: Mutex::new(pty),
            input_tx,
            output_tx,
            history: history.clone(),
            ended: ended.clone(),
            attachment: Mutex::new(AttachmentState {
                count: 0,
                detached_at: Some(Instant::now()),
            }),
        });

        let reader_history = history.clone();
        let reader_output = session.output_tx.clone();
        let reader_ended = ended.clone();
        tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; PTY_READ_BYTES];
            #[cfg(windows)]
            let (mut cursor_pending, mut cursor_done) = (Vec::new(), false);
            loop {
                if reader_ended.load(Ordering::Acquire) {
                    break;
                }
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        #[cfg(windows)]
                        let data = filter_initial_cursor_query(
                            data,
                            &mut cursor_pending,
                            &mut cursor_done,
                        );
                        if data.is_empty() {
                            continue;
                        }
                        let chunk = reader_history
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .push(data);
                        // No receivers is normal while the session is detached.
                        let _ = reader_output.send(chunk);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
            reader_ended.store(true, Ordering::Release);
        });

        let writer_ended = ended.clone();
        tokio::task::spawn_blocking(move || {
            'input: while let Some(data) = input_rx.blocking_recv() {
                let mut remaining = data.as_slice();
                while !remaining.is_empty() {
                    if writer_ended.load(Ordering::Acquire) {
                        break 'input;
                    }
                    match writer.write(remaining) {
                        Ok(0) => {
                            writer_ended.store(true, Ordering::Release);
                            break 'input;
                        }
                        Ok(n) => remaining = &remaining[n..],
                        Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(10))
                        }
                        Err(_) => {
                            writer_ended.store(true, Ordering::Release);
                            break 'input;
                        }
                    }
                }
            }
        });

        Ok(session)
    }

    pub fn subscribe_output(&self) -> broadcast::Receiver<Arc<OutputChunk>> {
        self.output_tx.subscribe()
    }

    pub fn snapshot_history(&self) -> (u64, Vec<Arc<OutputChunk>>) {
        let history = self
            .history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (history.last_seq, history.chunks.iter().cloned().collect())
    }

    pub fn attach(&self) {
        let mut state = self.attachment.lock().unwrap_or_else(|p| p.into_inner());
        state.count += 1;
        state.detached_at = None;
    }

    pub fn detach(&self) {
        let mut state = self.attachment.lock().unwrap_or_else(|p| p.into_inner());
        if state.count > 0 {
            state.count -= 1;
            if state.count == 0 {
                state.detached_at = Some(Instant::now());
            }
        }
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        if let Ok(pty) = self.pty.lock() {
            let _ = pty.resize(cols, rows);
        }
    }

    pub fn write_input(&self, data: Vec<u8>) -> bool {
        data.is_empty() || (!self.is_ended() && self.input_tx.try_send(data).is_ok())
    }

    pub fn is_ended(&self) -> bool {
        self.ended.load(Ordering::Acquire)
            || self.pty.lock().map(|mut p| p.has_exited()).unwrap_or(true)
    }

    pub fn detached_for(&self) -> Option<Duration> {
        let state = self.attachment.lock().unwrap_or_else(|p| p.into_inner());
        if state.count == 0 {
            state.detached_at.map(|at| at.elapsed())
        } else {
            None
        }
    }

    pub fn shutdown(&self) {
        self.ended.store(true, Ordering::Release);
        if let Ok(mut pty) = self.pty.lock() {
            pty.kill();
        }
    }
}

pub struct SessionManager {
    command: Arc<Vec<String>>,
    read_only: bool,
    sessions: Mutex<HashMap<String, Arc<SessionHandle>>>,
    max_sessions: usize,
    grace: Duration,
    opening: AtomicUsize,
    stopping: AtomicBool,
}

impl SessionManager {
    pub fn new(command: Arc<Vec<String>>, read_only: bool, max_sessions: usize) -> Self {
        Self {
            command,
            read_only,
            sessions: Mutex::new(HashMap::new()),
            max_sessions: max_sessions.max(2),
            grace: SESSION_GRACE_PERIOD,
            opening: AtomicUsize::new(0),
            stopping: AtomicBool::new(false),
        }
    }

    /// Attach to an existing session or create a fresh one.  A stale/invalid
    /// session id is deliberately treated as a request for a new session so a
    /// server restart cannot leave the browser in a permanent reconnect loop.
    pub fn attach_or_create(
        &self,
        requested_id: Option<&str>,
        cols: u16,
        rows: u16,
    ) -> Result<(Arc<SessionHandle>, bool)> {
        self.cleanup();
        let sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        anyhow::ensure!(!self.stopping.load(Ordering::Acquire), "server stopping");

        if let Some(id) = requested_id.filter(|id| valid_session_id(id)) {
            if let Some(session) = sessions.get(id) {
                if !session.is_ended() {
                    let session = session.clone();
                    session.attach();
                    drop(sessions);
                    session.resize(cols, rows);
                    return Ok((session, true));
                }
            }
        }

        anyhow::ensure!(
            sessions.len() + self.opening.load(Ordering::Relaxed) < self.max_sessions,
            "session capacity reached"
        );
        self.opening.fetch_add(1, Ordering::Relaxed);
        drop(sessions);
        let opened = SessionHandle::open(&self.command, cols, rows, self.read_only);
        let mut sessions = self.sessions.lock().unwrap_or_else(|p| p.into_inner());
        self.opening.fetch_sub(1, Ordering::Relaxed);
        let session = opened?;
        if self.stopping.load(Ordering::Acquire) {
            drop(sessions);
            session.shutdown();
            anyhow::bail!("server stopping");
        }
        sessions.insert(session.id.clone(), session.clone());
        session.attach();
        Ok((session, false))
    }

    pub fn cleanup(&self) {
        let mut dead = Vec::new();
        {
            let mut sessions = self
                .sessions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            sessions.retain(|_, session| {
                let expired = session.is_ended()
                    || session.detached_for().is_some_and(|age| age >= self.grace);
                if expired {
                    dead.push(session.clone());
                    false
                } else {
                    true
                }
            });
        }
        for session in dead {
            session.shutdown();
        }
    }

    pub fn shutdown_all(&self) {
        let sessions: Vec<_> = {
            let mut sessions = self
                .sessions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            self.stopping.store(true, Ordering::Release);
            sessions.drain().map(|(_, session)| session).collect()
        };
        for session in sessions {
            session.shutdown();
        }
    }
}

impl Drop for SessionManager {
    fn drop(&mut self) {
        self.shutdown_all();
    }
}

pub fn random_session_id() -> String {
    use base64::Engine;
    let mut bytes = [0u8; 32];
    let mut rng = SysRng;
    rng.try_fill_bytes(&mut bytes).expect("OS RNG failed");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn valid_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn capacity_preserves_detached_sessions_and_shutdown_prevents_creation() {
        let manager = SessionManager::new(Arc::new(crate::default_shell()), false, 2);
        let (a, _) = manager.attach_or_create(None, 80, 24).unwrap();
        let (b, _) = manager.attach_or_create(None, 80, 24).unwrap();
        a.detach();
        b.detach();
        assert!(manager.attach_or_create(None, 80, 24).is_err());
        assert!(manager.attach_or_create(Some(&a.id), 80, 24).unwrap().1);
        a.detach();
        manager.shutdown_all();
        assert!(manager.attach_or_create(None, 80, 24).is_err());
    }

    #[test]
    fn initial_cursor_query_is_filtered_even_across_reads() {
        let (mut pending, mut done) = (Vec::new(), false);
        let mut out = filter_initial_cursor_query(b"hello\x1b[".to_vec(), &mut pending, &mut done);
        out.extend(filter_initial_cursor_query(
            b"6nworld".to_vec(),
            &mut pending,
            &mut done,
        ));
        assert_eq!(out, b"helloworld");
        assert!(done);
        assert_eq!(
            filter_initial_cursor_query(b"\x1b[6n".to_vec(), &mut pending, &mut done),
            b"\x1b[6n"
        );
    }

    #[test]
    fn history_snapshot_has_no_uncommitted_sequence() {
        let history = Arc::new(Mutex::new(History::default()));
        let writer = history.clone();
        let thread = std::thread::spawn(move || {
            for _ in 0..10000 {
                writer.lock().unwrap().push(vec![0; 128]);
            }
        });
        for _ in 0..10000 {
            let h = history.lock().unwrap();
            assert_eq!(h.last_seq, h.chunks.back().map_or(0, |c| c.seq));
            assert!(h.bytes <= OUTPUT_HISTORY_BYTES);
        }
        thread.join().unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn child_exit_is_observed_and_not_reattached() {
        #[cfg(windows)]
        let command = vec!["cmd.exe".into(), "/c".into(), "exit".into(), "0".into()];
        #[cfg(unix)]
        let command = vec!["/bin/sh".into(), "-c".into(), "exit 0".into()];
        let manager = SessionManager::new(Arc::new(command), false, 2);
        let (session, _) = manager.attach_or_create(None, 80, 24).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !session.is_ended() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let (replacement, reattached) =
            manager.attach_or_create(Some(&session.id), 80, 24).unwrap();
        assert!(!reattached);
        assert_ne!(session.id, replacement.id);
        session.detach();
        replacement.detach();
        manager.shutdown_all();
    }
}
