//! Process command handler

use std::collections::HashMap;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Process output event
#[derive(Debug)]
pub enum ProcessOutput {
    Stdout(String),
    Stderr(String),
    Exit(i32),
    Error(String),
}

/// Run a command and stream output
pub async fn run_command(
    command: &str,
    args: &[String],
    env: &HashMap<String, String>,
    cwd: Option<&str>,
    output_tx: mpsc::Sender<ProcessOutput>,
) -> anyhow::Result<()> {
    let mut cmd = Command::new(command);
    cmd.args(args)
        .envs(env.iter())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }

    let mut child = cmd.spawn()?;

    let stdout = child.stdout.take().expect("stdout not captured");
    let stderr = child.stderr.take().expect("stderr not captured");

    let tx1 = output_tx.clone();
    let tx2 = output_tx.clone();

    // Stream stdout
    let stdout_handle = tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = tx1.send(ProcessOutput::Stdout(line)).await;
        }
    });

    // Stream stderr
    let stderr_handle = tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = tx2.send(ProcessOutput::Stderr(line)).await;
        }
    });

    // Wait for completion
    let status = child.wait().await?;

    // Wait for output streams to finish
    let _ = stdout_handle.await;
    let _ = stderr_handle.await;

    let exit_code = status.code().unwrap_or(-1);
    let _ = output_tx.send(ProcessOutput::Exit(exit_code)).await;

    Ok(())
}

/// Kill a process by PID
pub fn kill_process(pid: u32, signal: i32) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        let signal = Signal::try_from(signal)?;
        kill(Pid::from_raw(pid as i32), signal)?;
    }

    #[cfg(not(unix))]
    {
        anyhow::bail!("Process killing not supported on this platform");
    }

    Ok(())
}
