//! PTY handler

use std::collections::HashMap;
use std::sync::Arc;

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use tokio::sync::{Mutex, RwLock};

/// PTY instance
pub struct PtyInstance {
    pub master: Mutex<Box<dyn MasterPty + Send>>,
}

// Safety: We use Mutex to ensure exclusive access to the MasterPty
unsafe impl Sync for PtyInstance {}

/// PTY manager
pub struct PtyManager {
    ptys: RwLock<HashMap<String, Arc<PtyInstance>>>,
    max_ptys: usize,
}

impl PtyManager {
    /// Create a new PTY manager
    pub fn new(max_ptys: usize) -> Self {
        Self {
            ptys: RwLock::new(HashMap::new()),
            max_ptys,
        }
    }

    /// Create a new PTY
    pub async fn create(
        &self,
        id: String,
        cols: u16,
        rows: u16,
        shell: Option<&str>,
        env: &HashMap<String, String>,
    ) -> anyhow::Result<()> {
        let mut ptys = self.ptys.write().await;

        if ptys.len() >= self.max_ptys {
            anyhow::bail!("Maximum number of PTYs reached");
        }

        let pty_system = native_pty_system();

        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let shell = shell.unwrap_or("/bin/bash");
        let mut cmd = CommandBuilder::new(shell);
        for (key, value) in env {
            cmd.env(key, value);
        }

        let _child = pair.slave.spawn_command(cmd)?;

        let instance = Arc::new(PtyInstance {
            master: Mutex::new(pair.master),
        });

        ptys.insert(id, instance);

        Ok(())
    }

    /// Resize a PTY
    pub async fn resize(&self, id: &str, cols: u16, rows: u16) -> anyhow::Result<()> {
        let ptys = self.ptys.read().await;

        let pty = ptys
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("PTY not found"))?;

        let master = pty.master.lock().await;
        master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        Ok(())
    }

    /// Kill a PTY
    pub async fn kill(&self, id: &str) -> anyhow::Result<()> {
        let mut ptys = self.ptys.write().await;

        if ptys.remove(id).is_some() {
            Ok(())
        } else {
            anyhow::bail!("PTY not found")
        }
    }

    /// Write data to a PTY
    pub async fn write(&self, id: &str, data: &[u8]) -> anyhow::Result<()> {
        let ptys = self.ptys.read().await;

        let pty = ptys
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("PTY not found"))?;

        let master = pty.master.lock().await;
        use std::io::Write;
        let mut writer = master.take_writer()?;
        writer.write_all(data)?;

        Ok(())
    }
}
