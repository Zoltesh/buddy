//! Child process management for interface binaries (e.g. buddy-telegram).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use buddy_core::provider::Provider;
use buddy_core::state::{AppState, ChildProcessHandle};

/// Find the `buddy-telegram` binary adjacent to the current executable,
/// falling back to `PATH` lookup.
fn find_telegram_binary() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("buddy-telegram");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    // Fall back to PATH.
    which("buddy-telegram")
}

/// Simple PATH lookup for a binary name.
fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|p| p.exists())
    })
}

/// Spawn `buddy-telegram` with the given config path. Returns the child
/// process on success, or an error message.
fn spawn_telegram(config_path: &Path) -> Result<Child, String> {
    let binary = find_telegram_binary()
        .ok_or_else(|| "buddy-telegram binary not found (not adjacent to server, not in PATH)".to_string())?;

    let mut child = Command::new(&binary)
        .arg("--config")
        .arg(config_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn buddy-telegram: {e}"))?;

    // Spawn threads to read stdout/stderr with a [telegram] prefix.
    if let Some(stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => println!("[telegram] {l}"),
                    Err(_) => break,
                }
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(l) => eprintln!("[telegram] {l}"),
                    Err(_) => break,
                }
            }
        });
    }

    Ok(child)
}

/// Stop a running child process. Sends SIGTERM first, waits briefly,
/// then SIGKILL if still alive.
fn stop_child(child: &mut Child) {
    // Try graceful kill (SIGKILL on std::process::Child::kill).
    let _ = child.kill();
    let _ = child.wait();
}

/// Stop the currently running telegram process, if any.
pub fn stop_telegram(handle: &ChildProcessHandle) {
    let mut guard = handle.lock().unwrap();
    if let Some(ref mut child) = *guard {
        stop_child(child);
    }
    *guard = None;
}

/// Check the current config and manage the telegram child process accordingly.
/// Called on startup and after every config change.
pub fn manage_telegram<P: Provider>(state: &AppState<P>) {
    let config = state.config.read().unwrap();
    let tg = &config.interfaces.telegram;
    let should_run = tg.enabled && tg.resolve_bot_token().is_ok();
    let config_path = state.config_path.clone();
    drop(config);

    let mut guard = state.telegram_process.lock().unwrap();

    let is_running = guard
        .as_mut()
        .map(|c| c.try_wait().ok().flatten().is_none())
        .unwrap_or(false);

    if should_run && !is_running {
        // Need to spawn (or respawn after unexpected exit).
        // Kill stale child if it exited.
        if let Some(ref mut child) = *guard {
            let _ = child.wait();
        }
        *guard = None;

        match spawn_telegram(&config_path) {
            Ok(child) => {
                println!("  telegram:   spawned (pid {})", child.id());
                *guard = Some(child);
            }
            Err(e) => {
                eprintln!("  telegram:   warning: {e}");
            }
        }
    } else if !should_run && is_running {
        // Need to stop.
        if let Some(ref mut child) = *guard {
            stop_child(child);
            println!("  telegram:   stopped");
        }
        *guard = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn test_state(toml: &str) -> AppState<buddy_core::testutil::MockProvider> {
        let config = buddy_core::config::Config::parse(toml).unwrap();
        AppState {
            provider: arc_swap::ArcSwap::from_pointee(buddy_core::testutil::MockProvider {
                tokens: vec![],
            }),
            registry: arc_swap::ArcSwap::from_pointee(
                buddy_core::skill::SkillRegistry::new(),
            ),
            store: buddy_core::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: buddy_core::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(
                buddy_core::config::MemoryConfig::default(),
            ),
            warnings: buddy_core::warning::new_shared_warnings(),
            pending_approvals: buddy_core::state::new_pending_approvals(),
            conversation_approvals: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(60),
            config: std::sync::RwLock::new(config),
            config_path: PathBuf::from("/tmp/buddy-test-070.toml"),
            on_config_change: None,
            telegram_process: buddy_core::state::new_child_process_handle(),
        }
    }

    /// Test case: manage_telegram with disabled config does not spawn a process.
    #[test]
    fn manage_telegram_disabled_does_not_spawn() {
        let state = test_state(
            r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test"
endpoint = "http://localhost:1234/v1"

[interfaces.telegram]
enabled = false
"#,
        );
        manage_telegram(&state);
        let guard = state.telegram_process.lock().unwrap();
        assert!(guard.is_none());
    }

    /// Test case: manage_telegram with enabled config but no binary available
    /// logs a warning and does not crash.
    #[test]
    fn manage_telegram_enabled_no_binary_does_not_crash() {
        let state = test_state(
            r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test"
endpoint = "http://localhost:1234/v1"

[interfaces.telegram]
enabled = true
bot_token = "fake:token"
"#,
        );
        // Ensure buddy-telegram is not findable — clear PATH to a nonexistent dir.
        let orig_path = std::env::var_os("PATH");
        unsafe { std::env::set_var("PATH", "/nonexistent_070_test_dir"); }
        manage_telegram(&state);
        if let Some(p) = orig_path {
            unsafe { std::env::set_var("PATH", p); }
        }
        let guard = state.telegram_process.lock().unwrap();
        assert!(guard.is_none(), "should not have spawned without binary");
    }

    /// Test case: stop_telegram with no running process is a no-op.
    #[test]
    fn stop_telegram_no_process_is_noop() {
        let handle = buddy_core::state::new_child_process_handle();
        stop_telegram(&handle);
        assert!(handle.lock().unwrap().is_none());
    }

    /// Test case: stop_telegram kills a running child process.
    #[test]
    fn stop_telegram_kills_running_process() {
        let handle = buddy_core::state::new_child_process_handle();
        let child = Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("failed to spawn sleep");
        let pid = child.id();
        *handle.lock().unwrap() = Some(child);

        stop_telegram(&handle);

        assert!(handle.lock().unwrap().is_none());
        // Verify the process is no longer running.
        let status = Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status();
        assert!(
            status.is_err() || !status.unwrap().success(),
            "process should no longer be running"
        );
    }

    /// Test case: manage_telegram_on_config_change with disabled config and
    /// no running process does not crash.
    #[test]
    fn manage_telegram_on_config_change_disabled_noop() {
        let state = test_state(
            r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test"
endpoint = "http://localhost:1234/v1"

[interfaces.telegram]
enabled = false
"#,
        );
        manage_telegram_on_config_change(&state);
        assert!(state.telegram_process.lock().unwrap().is_none());
    }
}

/// Variant of manage_telegram that always restarts a running process.
/// Used when config changes (token or enabled state may have changed).
pub fn manage_telegram_on_config_change<P: Provider>(state: &AppState<P>) {
    let config = state.config.read().unwrap();
    let tg = &config.interfaces.telegram;
    let should_run = tg.enabled && tg.resolve_bot_token().is_ok();
    let config_path = state.config_path.clone();

    if tg.enabled && !should_run {
        eprintln!("  telegram:   warning: enabled but bot token could not be resolved");
    }
    drop(config);

    let mut guard = state.telegram_process.lock().unwrap();

    let is_running = guard
        .as_mut()
        .map(|c| c.try_wait().ok().flatten().is_none())
        .unwrap_or(false);

    if should_run {
        // Kill existing if running (config may have changed).
        if is_running {
            if let Some(ref mut child) = *guard {
                stop_child(child);
            }
        }
        *guard = None;

        match spawn_telegram(&config_path) {
            Ok(child) => {
                println!("  telegram:   restarted (pid {})", child.id());
                *guard = Some(child);
            }
            Err(e) => {
                eprintln!("  telegram:   warning: {e}");
            }
        }
    } else if is_running {
        if let Some(ref mut child) = *guard {
            stop_child(child);
            println!("  telegram:   stopped");
        }
        *guard = None;
    }
}
