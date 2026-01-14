//! Sandbox domain model

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Sandbox state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxState {
    /// Sandbox is being created
    Starting,
    /// Sandbox is running and ready
    Running,
    /// Sandbox is being stopped
    Stopping,
    /// Sandbox is stopped
    Stopped,
    /// Sandbox encountered an error
    Error,
}

impl SandboxState {
    pub fn as_str(&self) -> &'static str {
        match self {
            SandboxState::Starting => "starting",
            SandboxState::Running => "running",
            SandboxState::Stopping => "stopping",
            SandboxState::Stopped => "stopped",
            SandboxState::Error => "error",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "starting" => Some(SandboxState::Starting),
            "running" => Some(SandboxState::Running),
            "stopping" => Some(SandboxState::Stopping),
            "stopped" => Some(SandboxState::Stopped),
            "error" => Some(SandboxState::Error),
            _ => None,
        }
    }
}

/// Sandbox entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sandbox {
    /// Unique identifier
    pub id: String,

    /// Optional human-readable name
    pub name: Option<String>,

    /// Template used to create this sandbox
    pub template: String,

    /// Current state
    pub state: SandboxState,

    /// Docker container ID
    pub container_id: Option<String>,

    /// Environment variables
    pub env: HashMap<String, String>,

    /// Custom metadata
    pub metadata: HashMap<String, String>,

    /// NFS mount URL (if available)
    pub nfs_url: Option<String>,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    pub updated_at: DateTime<Utc>,

    /// Timeout in seconds (0 = no timeout)
    pub timeout: u64,

    /// Error message (if state is Error)
    pub error_message: Option<String>,
}

impl Sandbox {
    /// Create a new sandbox
    pub fn new(id: String, template: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            name: None,
            template,
            state: SandboxState::Starting,
            container_id: None,
            env: HashMap::new(),
            metadata: HashMap::new(),
            nfs_url: None,
            created_at: now,
            updated_at: now,
            timeout: 0,
            error_message: None,
        }
    }

    /// Check if the sandbox is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self.state, SandboxState::Stopped | SandboxState::Error)
    }

    /// Check if the sandbox can accept commands
    pub fn is_ready(&self) -> bool {
        self.state == SandboxState::Running
    }

    /// Transition to a new state
    pub fn transition_to(&mut self, new_state: SandboxState) -> bool {
        let valid_transition = match (self.state, new_state) {
            // From Starting
            (SandboxState::Starting, SandboxState::Running) => true,
            (SandboxState::Starting, SandboxState::Error) => true,
            // From Running
            (SandboxState::Running, SandboxState::Stopping) => true,
            (SandboxState::Running, SandboxState::Error) => true,
            // From Stopping
            (SandboxState::Stopping, SandboxState::Stopped) => true,
            (SandboxState::Stopping, SandboxState::Error) => true,
            // No other transitions allowed
            _ => false,
        };

        if valid_transition {
            self.state = new_state;
            self.updated_at = Utc::now();
        }

        valid_transition
    }
}

/// Parameters for creating a sandbox
#[derive(Debug, Clone, Deserialize)]
pub struct CreateSandboxParams {
    /// Template to use
    pub template: Option<String>,

    /// Optional name
    pub name: Option<String>,

    /// Environment variables
    pub env: Option<HashMap<String, String>>,

    /// Custom metadata
    pub metadata: Option<HashMap<String, String>>,

    /// Timeout in seconds
    pub timeout: Option<u64>,
}
