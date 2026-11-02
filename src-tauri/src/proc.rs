use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use tokio::sync::{RwLock, watch};

#[cfg(windows)]
use process_wrap::tokio::CreationFlags;
#[cfg(windows)]
use process_wrap::tokio::JobObject;
#[cfg(unix)]
use process_wrap::tokio::ProcessGroup;
#[cfg(windows)]
use windows::Win32::System::Threading::CREATE_NO_WINDOW;

/// Restart limit within one time window. Past this, the process counts as
/// a permanent failure: better to stop and report than to spin forever.
const MAX_RESTARTS: usize = 5;
const RESTART_WINDOW: Duration = Duration::from_secs(60);
const READY_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServiceState {
    Starting,
    Ready {
        pid: Option<u32>,
    },
    Backoff {
        attempt: u32,
        delay_ms: u64,
        reason: String,
    },
    Failed {
        reason: String,
    },
    Stopped,
}

impl ServiceState {
    /// Stable label for the UI; must not change when the message changes.
    pub fn tag(&self) -> &'static str {
        match self {
            ServiceState::Starting => "starting",
            ServiceState::Ready { .. } => "ready",
            ServiceState::Backoff { .. } => "backoff",
            ServiceState::Failed { .. } => "failed",
            ServiceState::Stopped => "stopped",
        }
    }

    pub fn detail(&self) -> Option<String> {
        match self {
            ServiceState::Ready { pid } => pid.map(|pid| format!("pid {pid}")),
            ServiceState::Backoff { reason, .. } | ServiceState::Failed { reason } => {
                Some(reason.clone())
            }
            _ => None,
        }
    }
}

impl std::fmt::Display for ServiceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceState::Starting => write!(f, "starting"),
            ServiceState::Ready { pid } => match pid {
                Some(pid) => write!(f, "ready (pid {pid})"),
                None => write!(f, "ready"),
            },
            ServiceState::Backoff {
                attempt,
                delay_ms,
                reason,
            } => write!(f, "backoff #{attempt} {delay_ms}ms: {reason}"),
            ServiceState::Failed { reason } => write!(f, "failed: {reason}"),
            ServiceState::Stopped => write!(f, "stopped"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ServiceSpec {
    pub name: String,
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: Option<PathBuf>,
    /// When set, the service counts as ready once this port accepts a connection.
    pub port: Option<u16>,
    pub log_dir: PathBuf,
}

type StateHook = Arc<dyn Fn(&str, &ServiceState) + Send + Sync>;

pub struct Supervisor {
    pub name: String,
    state: Arc<RwLock<ServiceState>>,
    stop_tx: watch::Sender<bool>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl Supervisor {
    pub fn start(spec: ServiceSpec, hook: Option<StateHook>) -> Result<Self> {
        let name = spec.name.clone();
        let state = Arc::new(RwLock::new(ServiceState::Starting));
        let (stop_tx, stop_rx) = watch::channel(false);
        let task_state = state.clone();
        let task_name = name.clone();

        let task = tokio::spawn(async move {
            run(spec, task_state, stop_rx, task_name, hook).await;
        });

        Ok(Self {
            name,
            state,
            stop_tx,
            task: Some(task),
        })
    }

    pub async fn state(&self) -> ServiceState {
        self.state.read().await.clone()
    }

    /// Waits for the service to become ready. Returns the last reason on failure, so
    /// the caller does not just see "starting" without context.
    pub async fn wait_ready(&self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        let mut last = String::from("starting");

        loop {
            match self.state.read().await.clone() {
                ServiceState::Ready { .. } => return Ok(()),
                ServiceState::Failed { reason } => return Err(reason),
                ServiceState::Stopped => return Err("berhenti sebelum siap".to_string()),
                ServiceState::Backoff { reason, .. } => last = reason,
                ServiceState::Starting => {}
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "belum siap setelah {}s ({last})",
                    timeout.as_secs()
                ));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    pub async fn shutdown(mut self) {
        let _ = self.stop_tx.send(true);
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        // Child processes are killed by KillOnDrop too; this signal only speeds up
        // the loop so it cannot respawn during shutdown.
        let _ = self.stop_tx.send(true);
    }
}

async fn run(
    spec: ServiceSpec,
    state: Arc<RwLock<ServiceState>>,
    mut stop: watch::Receiver<bool>,
    name: String,
    hook: Option<StateHook>,
) {
    let mut restarts: VecDeque<Instant> = VecDeque::new();

    loop {
        if *stop.borrow() {
            break;
        }

        set_state(&state, &name, ServiceState::Starting, &hook).await;

        let mut child = match spawn_once(&spec) {
            Ok(child) => child,
            Err(err) => {
                set_state(
                    &state,
                    &name,
                    ServiceState::Failed {
                        reason: format!("gagal start {}: {err}", spec.program.display()),
                    },
                    &hook,
                )
                .await;
                return;
            }
        };

        let pid = child.id();
        let ready = match spec.port {
            Some(port) => wait_port(port, READY_TIMEOUT, &mut stop).await,
            None => true,
        };

        let exit_reason = if !ready {
            stop_child(&mut child).await;
            let _ = child.wait().await;
            "tidak siap sebelum timeout".to_string()
        } else {
            set_state(&state, &name, ServiceState::Ready { pid }, &hook).await;

            tokio::select! {
                status = child.wait() => match status {
                    Ok(status) => format!("exit {}", describe_status(&status)),
                    Err(err) => format!("wait error: {err}"),
                },
                _ = stop.changed() => {
                    stop_child(&mut child).await;
                    let _ = child.wait().await;
                    set_state(&state, &name, ServiceState::Stopped, &hook).await;
                    return;
                }
            }
        };

        if *stop.borrow() {
            set_state(&state, &name, ServiceState::Stopped, &hook).await;
            return;
        }

        let now = Instant::now();
        restarts.push_back(now);
        while let Some(front) = restarts.front() {
            if now.duration_since(*front) > RESTART_WINDOW {
                restarts.pop_front();
            } else {
                break;
            }
        }

        if restarts.len() > MAX_RESTARTS {
            set_state(
                &state,
                &name,
                ServiceState::Failed {
                    reason: format!(
                        "{exit_reason}; {MAX_RESTARTS}+ restart dalam {}s",
                        RESTART_WINDOW.as_secs()
                    ),
                },
                &hook,
            )
            .await;
            return;
        }

        let attempt = restarts.len() as u32;
        let delay = backoff_delay(attempt);
        set_state(
            &state,
            &name,
            ServiceState::Backoff {
                attempt,
                delay_ms: delay.as_millis() as u64,
                reason: exit_reason,
            },
            &hook,
        )
        .await;

        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = stop.changed() => {
                set_state(&state, &name, ServiceState::Stopped, &hook).await;
                return;
            }
        }
    }

    set_state(&state, &name, ServiceState::Stopped, &hook).await;
}

fn spawn_once(spec: &ServiceSpec) -> Result<Box<dyn ChildWrapper>> {
    std::fs::create_dir_all(&spec.log_dir).ok();
    let log_path = spec.log_dir.join(format!("{}.log", spec.name));

    let out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("gagal membuka log {}", log_path.display()))?;
    let err = out.try_clone()?;

    let args = spec.args.clone();
    let env = spec.env.clone();
    let cwd = spec.cwd.clone();

    let mut wrap = CommandWrap::with_new(&spec.program, move |cmd| {
        cmd.args(&args);
        for (key, value) in &env {
            cmd.env(key, value);
        }
        if let Some(dir) = &cwd {
            cmd.current_dir(dir);
        }
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::from(out));
        cmd.stderr(std::process::Stdio::from(err));
    });

    // Windows: a Job Object with kill-on-close stops php-cgi from being orphaned
    // when devku itself dies. Unix: a process group so the signal reaches children.
    #[cfg(windows)]
    {
        wrap.wrap(CreationFlags(CREATE_NO_WINDOW));
        wrap.wrap(JobObject);
    }
    #[cfg(unix)]
    {
        wrap.wrap(ProcessGroup::leader());
    }
    wrap.wrap(KillOnDrop);

    let child = wrap
        .spawn()
        .with_context(|| format!("spawn {}", spec.program.display()))?;
    Ok(child)
}

async fn stop_child(child: &mut Box<dyn ChildWrapper>) {
    // kill() returns a Box<dyn Future> that is not Unpin; Box::into_pin
    // makes it awaitable without adding a dependency.
    let _ = Box::into_pin(child.kill()).await;
}

async fn wait_port(port: u16, timeout: Duration, stop: &mut watch::Receiver<bool>) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(stream) = tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
            drop(stream);
            return true;
        }
        if Instant::now() >= deadline || *stop.borrow() {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn backoff_delay(attempt: u32) -> Duration {
    let millis = 200u64.saturating_mul(1u64 << attempt.min(5));
    Duration::from_millis(millis.min(5_000))
}

fn describe_status(status: &std::process::ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("kode {code}"),
        None => "terminated".to_string(),
    }
}

async fn set_state(
    state: &Arc<RwLock<ServiceState>>,
    name: &str,
    next: ServiceState,
    hook: &Option<StateHook>,
) {
    {
        let mut guard = state.write().await;
        *guard = next.clone();
    }
    if let Some(hook) = hook {
        hook(name, &next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_then_caps() {
        assert_eq!(backoff_delay(0), Duration::from_millis(200));
        assert_eq!(backoff_delay(1), Duration::from_millis(400));
        assert_eq!(backoff_delay(4), Duration::from_millis(3200));
        assert_eq!(backoff_delay(9), Duration::from_millis(5000));
    }
}
