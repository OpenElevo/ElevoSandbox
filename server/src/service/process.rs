//! Process service

use std::collections::HashMap;
use std::sync::Arc;

use futures::Stream;
use tracing::info;

use crate::domain::sandbox::SandboxState;
use crate::domain::types::{CommandResult, ProcessEvent};
use crate::error::{Error, Result};
use crate::infra::agent_pool::{AgentCommandResponse, AgentConnPool};
use crate::infra::sqlite::SandboxRepository;

/// Options for running a command
#[derive(Debug, Clone, Default)]
pub struct RunCommandOptions {
    /// Command to execute
    pub command: String,
    /// Command arguments
    pub args: Vec<String>,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Working directory
    pub cwd: Option<String>,
    /// Timeout in milliseconds (0 = no timeout)
    pub timeout_ms: u64,
}

/// Process service for executing commands
pub struct ProcessService {
    agent_pool: Arc<AgentConnPool>,
    repository: Arc<SandboxRepository>,
}

impl ProcessService {
    /// Create a new process service
    pub fn new(agent_pool: Arc<AgentConnPool>, repository: Arc<SandboxRepository>) -> Self {
        Self {
            agent_pool,
            repository,
        }
    }

    /// Run a command and wait for completion
    pub async fn run(&self, sandbox_id: &str, opts: RunCommandOptions) -> Result<CommandResult> {
        // Validate sandbox exists and is running
        let sandbox = self.repository.get(sandbox_id).await?;
        if sandbox.state != SandboxState::Running {
            return Err(Error::InvalidSandboxState {
                expected: "running".to_string(),
                actual: sandbox.state.as_str().to_string(),
            });
        }

        // Check agent connection
        if !self.agent_pool.is_connected(sandbox_id) {
            return Err(Error::AgentNotConnected(sandbox_id.to_string()));
        }

        info!(
            "Running command in sandbox {}: {} {:?}",
            sandbox_id, opts.command, opts.args
        );

        // Execute command via agent
        let response = self
            .agent_pool
            .run_command(
                sandbox_id,
                opts.command,
                opts.args,
                opts.env,
                opts.cwd,
                opts.timeout_ms,
                false, // Not streaming
            )
            .await?;

        // Convert response to CommandResult
        self.response_to_result(response)
    }

    /// Run a command with streaming output
    pub async fn run_stream(
        &self,
        sandbox_id: &str,
        opts: RunCommandOptions,
    ) -> Result<impl Stream<Item = ProcessEvent>> {
        // Validate sandbox exists and is running
        let sandbox = self.repository.get(sandbox_id).await?;
        if sandbox.state != SandboxState::Running {
            return Err(Error::InvalidSandboxState {
                expected: "running".to_string(),
                actual: sandbox.state.as_str().to_string(),
            });
        }

        // Check agent connection
        if !self.agent_pool.is_connected(sandbox_id) {
            return Err(Error::AgentNotConnected(sandbox_id.to_string()));
        }

        info!(
            "Running streamed command in sandbox {}: {} {:?}",
            sandbox_id, opts.command, opts.args
        );

        // For now, return a simple stream that executes and returns result
        // TODO: Implement true streaming via gRPC bidirectional stream
        let response = self
            .agent_pool
            .run_command(
                sandbox_id,
                opts.command,
                opts.args,
                opts.env,
                opts.cwd,
                opts.timeout_ms,
                true, // Streaming requested
            )
            .await?;

        // Convert to stream of events
        let events = self.response_to_events(response);
        Ok(futures::stream::iter(events))
    }

    /// Kill a running process
    pub async fn kill(&self, sandbox_id: &str, pid: u32, signal: Option<i32>) -> Result<()> {
        // Validate sandbox exists and is running
        let sandbox = self.repository.get(sandbox_id).await?;
        if sandbox.state != SandboxState::Running {
            return Err(Error::InvalidSandboxState {
                expected: "running".to_string(),
                actual: sandbox.state.as_str().to_string(),
            });
        }

        // Check agent connection
        if !self.agent_pool.is_connected(sandbox_id) {
            return Err(Error::AgentNotConnected(sandbox_id.to_string()));
        }

        let sig = signal.unwrap_or(15); // SIGTERM
        info!(
            "Killing process {} in sandbox {} with signal {}",
            pid, sandbox_id, sig
        );

        self.agent_pool.kill_process(sandbox_id, pid, sig).await
    }

    /// Convert agent response to CommandResult
    fn response_to_result(&self, response: AgentCommandResponse) -> Result<CommandResult> {
        if response.success {
            Ok(CommandResult {
                exit_code: response.exit_code.unwrap_or(0),
                stdout: response.stdout.unwrap_or_default(),
                stderr: response.stderr.unwrap_or_default(),
            })
        } else {
            Err(Error::ProcessExecutionFailed(
                response
                    .error_message
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ))
        }
    }

    /// Convert agent response to process events
    fn response_to_events(&self, response: AgentCommandResponse) -> Vec<ProcessEvent> {
        let mut events = Vec::new();

        if let Some(stdout) = response.stdout {
            if !stdout.is_empty() {
                events.push(ProcessEvent::Stdout { data: stdout });
            }
        }

        if let Some(stderr) = response.stderr {
            if !stderr.is_empty() {
                events.push(ProcessEvent::Stderr { data: stderr });
            }
        }

        if response.success {
            events.push(ProcessEvent::Exit {
                code: response.exit_code.unwrap_or(0),
            });
        } else {
            events.push(ProcessEvent::Error {
                message: response
                    .error_message
                    .unwrap_or_else(|| "Unknown error".to_string()),
            });
        }

        events
    }
}
