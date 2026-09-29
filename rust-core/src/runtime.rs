use std::path::{Path, PathBuf};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct LocalAiRuntime {
    child: Arc<Mutex<Option<Child>>>,
}

impl LocalAiRuntime {
    pub fn model_dir() -> PathBuf {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            .join("KronikiRPG")
            .join("ai")
    }

    pub fn model_path() -> PathBuf {
        Self::model_dir().join("Qwen3-8B-Q5_K_M.gguf")
    }

    pub fn server_path() -> PathBuf {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("kroniki_core.exe"));
        exe.parent().unwrap_or(Path::new("."))
            .join("ai-runtime")
            .join(if cfg!(windows) { "llama-server.exe" } else { "llama-server" })
    }

    pub fn installed() -> bool {
        Self::model_path().is_file() && Self::server_path().is_file()
    }

    pub async fn ensure_started(&self) -> Result<bool, String> {
        if !Self::installed() {
            return Ok(false);
        }

        {
            let mut guard = self.child.lock().await;
            if let Some(child) = guard.as_mut() {
                match child.try_wait() {
                    Ok(None) => return Ok(true),
                    Ok(Some(_)) | Err(_) => *guard = None,
                }
            }
        }

        std::fs::create_dir_all(Self::model_dir()).map_err(|e|e.to_string())?;
        let mut cmd = Command::new(Self::server_path());
        cmd.arg("-m").arg(Self::model_path())
            .arg("--host").arg("127.0.0.1")
            .arg("--port").arg("8080")
            .arg("-c").arg("14000")
            .arg("-ngl").arg("999")
            .arg("--jinja")
            .kill_on_drop(true);

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let child = cmd.spawn().map_err(|e| format!("Nie można uruchomić lokalnego MGAI: {e}"))?;
        *self.child.lock().await = Some(child);
        Ok(true)
    }

    pub async fn stop(&self) {
        let mut guard = self.child.lock().await;
        if let Some(child) = guard.as_mut() {
            let _ = child.kill().await;
        }
        *guard = None;
    }

    pub fn status_json() -> serde_json::Value {
        serde_json::json!({
            "runtime_present": Self::server_path().is_file(),
            "model_present": Self::model_path().is_file(),
            "model_path": Self::model_path(),
            "runtime_path": Self::server_path(),
            "profile": "Qwen3-8B-Q5_K_M"
        })
    }
}
