use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

#[derive(Clone, Debug)]
pub struct AgentRecoveryConfig {
    pub pane_id: String,
    pub agent_dir: PathBuf,
    pub command_file: PathBuf,
    pub agent_log_file: PathBuf,
    pub idle_grace_seconds: u64,
    pub stall_grace_seconds: u64,
    pub restart_backoff_seconds: u64,
    pub poll_seconds: u64,
}

/// the command file as written by clawblox when the agent
/// started, kept in memory and in a private copy outside the agent-visible runtime
/// dir. Respawns run the private copy, never `<agent>/runtime/start.sh`.
#[derive(Clone, Debug)]
struct TrustedCommand {
    path: PathBuf,
    contents: Vec<u8>,
}

fn recovery_dir(config: &AgentRecoveryConfig) -> PathBuf {
    config.agent_dir.join("recovery")
}

fn snapshot_command_file(config: &AgentRecoveryConfig) -> Result<TrustedCommand, String> {
    let contents = std::fs::read(&config.command_file)
        .map_err(|err| format!("failed to read {}: {err}", config.command_file.display()))?;
    let trusted = TrustedCommand {
        path: recovery_dir(config).join("start.sh"),
        contents,
    };
    ensure_trusted_copy(&trusted)?;
    Ok(trusted)
}

/// Rewrite the private copy from memory if it is missing or differs.
fn ensure_trusted_copy(trusted: &TrustedCommand) -> Result<(), String> {
    if std::fs::read(&trusted.path).ok().as_deref() == Some(trusted.contents.as_slice()) {
        return Ok(());
    }
    if let Some(parent) = trusted.path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    let _ = std::fs::remove_file(&trusted.path);
    std::fs::write(&trusted.path, &trusted.contents)
        .map_err(|err| format!("failed to write {}: {err}", trusted.path.display()))
}

fn command_file_changed(config: &AgentRecoveryConfig, trusted: &TrustedCommand) -> bool {
    std::fs::read(&config.command_file).ok().as_deref() != Some(trusted.contents.as_slice())
}

fn respawn_command(trusted: &TrustedCommand) -> String {
    format!("bash {}", shell_quote(&trusted.path.display().to_string()))
}

#[derive(Clone, Debug, Serialize)]
struct AgentRecoveryMetadata {
    pid: u32,
    pane_id: String,
    command_file: PathBuf,
    agent_log_file: PathBuf,
    idle_grace_seconds: u64,
    stall_grace_seconds: u64,
    restart_backoff_seconds: u64,
    poll_seconds: u64,
}

pub fn start_agent_recovery_watchdog(config: AgentRecoveryConfig) -> Result<(), String> {
    let runtime_dir = recovery_dir(&config);
    std::fs::create_dir_all(&runtime_dir)
        .map_err(|err| format!("failed to create recovery dir: {err}"))?;
    let trusted = snapshot_command_file(&config)?;
    let metadata = AgentRecoveryMetadata {
        pid: std::process::id(),
        pane_id: config.pane_id.clone(),
        command_file: config.command_file.clone(),
        agent_log_file: config.agent_log_file.clone(),
        idle_grace_seconds: config.idle_grace_seconds,
        stall_grace_seconds: config.stall_grace_seconds,
        restart_backoff_seconds: config.restart_backoff_seconds,
        poll_seconds: config.poll_seconds,
    };
    let metadata_path = runtime_dir.join("recovery_watchdog.json");
    let metadata_json = serde_json::to_string_pretty(&metadata).map_err(|err| err.to_string())?;
    std::fs::write(&metadata_path, format!("{metadata_json}\n"))
        .map_err(|err| format!("failed to write {}: {err}", metadata_path.display()))?;

    thread::Builder::new()
        .name(format!("clawblox-recovery-{}", config.pane_id))
        .spawn(move || watch_recovery(config, trusted))
        .map_err(|err| format!("failed to start recovery watchdog: {err}"))?;
    Ok(())
}

fn watch_recovery(config: AgentRecoveryConfig, trusted: TrustedCommand) {
    let watch_started = now_epoch();
    log_event(
        &config,
        "watch_start",
        &format!(
            "agent_dir={} command_file={} idle_grace_seconds={} stall_grace_seconds={} restart_backoff_seconds={} poll_seconds={}",
            config.agent_dir.display(),
            config.command_file.display(),
            config.idle_grace_seconds,
            config.stall_grace_seconds,
            config.restart_backoff_seconds,
            config.poll_seconds,
        ),
    );

    let mut last_handled_key = String::new();
    while pane_exists(&config.pane_id) {
        let Some(session_id) = current_session_id(&config) else {
            sleep_poll(&config);
            continue;
        };
        let stop_failure_file = config
            .agent_dir
            .join("logs")
            .join(format!("stop_failure.{session_id}.json"));
        if !stop_failure_file.is_file() {
            sleep_poll(&config);
            continue;
        }
        let stop_failure_epoch = file_mtime_epoch(&stop_failure_file);
        let session_start_epoch = file_mtime_epoch(
            &config
                .agent_dir
                .join("logs")
                .join(format!("session_start.{session_id}.json")),
        );
        if stop_failure_epoch < watch_started || stop_failure_epoch < session_start_epoch {
            sleep_poll(&config);
            continue;
        }
        let current_key = format!("{session_id}:{stop_failure_epoch}");
        if current_key == last_handled_key {
            sleep_poll(&config);
            continue;
        }

        let (failure_error, mut transcript_path) = stop_failure_fields(&stop_failure_file);
        if transcript_path.is_none() {
            transcript_path = session_transcript_path(&config, &session_id);
        }
        let idle_prompt_epoch = file_mtime_epoch(
            &config
                .agent_dir
                .join("logs")
                .join(format!("notification.idle_prompt.{session_id}.json")),
        );
        let session_end_epoch = file_mtime_epoch(
            &config
                .agent_dir
                .join("logs")
                .join(format!("session_end.{session_id}.json")),
        );
        let mut progress_epoch = transcript_path
            .as_ref()
            .map(|path| file_mtime_epoch(path))
            .unwrap_or(0);
        if progress_epoch == 0 {
            progress_epoch = file_mtime_epoch(&config.agent_log_file);
        }
        let now = now_epoch();

        if session_end_epoch >= stop_failure_epoch {
            last_handled_key = current_key;
            log_event(
                &config,
                "recover_session_end",
                &format!(
                    "session_id={session_id} error={failure_error} stop_failure_epoch={stop_failure_epoch} session_end_epoch={session_end_epoch}"
                ),
            );
            respawn_agent(
                &config,
                &trusted,
                "stop_failure_session_end",
                &session_id,
                stop_failure_epoch,
            );
            continue;
        }
        if idle_prompt_epoch >= stop_failure_epoch
            && now.saturating_sub(idle_prompt_epoch) >= config.idle_grace_seconds
        {
            last_handled_key = current_key;
            log_event(
                &config,
                "recover_idle_prompt",
                &format!(
                    "session_id={session_id} error={failure_error} stop_failure_epoch={stop_failure_epoch} idle_prompt_epoch={idle_prompt_epoch}"
                ),
            );
            respawn_agent(
                &config,
                &trusted,
                "stop_failure_idle_prompt",
                &session_id,
                stop_failure_epoch,
            );
            continue;
        }
        if now.saturating_sub(stop_failure_epoch) >= config.stall_grace_seconds
            && progress_epoch <= stop_failure_epoch
        {
            last_handled_key = current_key;
            log_event(
                &config,
                "recover_stalled",
                &format!(
                    "session_id={session_id} error={failure_error} stop_failure_epoch={stop_failure_epoch} progress_epoch={progress_epoch}"
                ),
            );
            respawn_agent(
                &config,
                &trusted,
                "stop_failure_stalled",
                &session_id,
                stop_failure_epoch,
            );
            continue;
        }

        sleep_poll(&config);
    }
    log_event(&config, "watch_exit", "reason=pane_missing");
}

fn current_session_id(config: &AgentRecoveryConfig) -> Option<String> {
    let hook = config.agent_dir.join("runtime/hook_session.env");
    if let Some(value) = read_hook_value(&hook, "SESSION_ID") {
        return Some(value);
    }
    read_trimmed(&config.agent_dir.join("claude_session_id.txt"))
}

fn session_transcript_path(config: &AgentRecoveryConfig, session_id: &str) -> Option<PathBuf> {
    let session_start = config
        .agent_dir
        .join("logs")
        .join(format!("session_start.{session_id}.json"));
    if let Ok(text) = std::fs::read_to_string(&session_start) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(path) = value
                .get("transcript_path")
                .and_then(|value| value.as_str())
            {
                return map_runtime_path(config, path);
            }
        }
    }
    let hook = config.agent_dir.join("runtime/hook_session.env");
    let hook_session = read_hook_value(&hook, "SESSION_ID");
    if hook_session.as_deref() == Some(session_id) {
        return read_hook_value(&hook, "TRANSCRIPT_PATH")
            .as_deref()
            .and_then(|path| map_runtime_path(config, path));
    }
    None
}

fn stop_failure_fields(path: &Path) -> (String, Option<PathBuf>) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return ("unknown".to_string(), None);
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return ("unknown".to_string(), None);
    };
    let error = value
        .get("error")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown")
        .to_string();
    let transcript = value
        .get("transcript_path")
        .and_then(|value| value.as_str())
        .map(PathBuf::from);
    (error, transcript)
}

fn map_runtime_path(config: &AgentRecoveryConfig, path: &str) -> Option<PathBuf> {
    if path.is_empty() {
        return None;
    }
    if let Some(rest) = path.strip_prefix("/home/agent/") {
        return Some(config.agent_dir.join("sandbox-home").join(rest));
    }
    Some(PathBuf::from(path))
}

fn respawn_agent(
    config: &AgentRecoveryConfig,
    trusted: &TrustedCommand,
    reason: &str,
    session_id: &str,
    stop_failure_epoch: u64,
) {
    log_event(
        config,
        "respawn_start",
        &format!("reason={reason} session_id={session_id} stop_failure_epoch={stop_failure_epoch}"),
    );
    if command_file_changed(config, trusted) {
        log_event(
            config,
            "command_file_changed",
            &format!("command_file={} respawning from the private copy", config.command_file.display()),
        );
    }
    if let Err(err) = ensure_trusted_copy(trusted) {
        log_event(config, "respawn_refused", &err);
        return;
    }
    if let Err(err) = crate::agent_runner::prepare_agent_log(&config.agent_log_file) {
        log_event(config, "respawn_refused", &format!("agent log: {err}"));
        return;
    }
    let respawn_cmd = respawn_command(trusted);
    let _ = Command::new("tmux")
        .args(["respawn-pane", "-k", "-t", &config.pane_id, &respawn_cmd])
        .status();
    let pipe_command = format!(
        "cat >> {}",
        shell_quote(&config.agent_log_file.display().to_string())
    );
    let _ = Command::new("tmux")
        .args(["pipe-pane", "-o", "-t", &config.pane_id, &pipe_command])
        .status();
    thread::sleep(Duration::from_secs(config.restart_backoff_seconds));
}

fn pane_exists(pane_id: &str) -> bool {
    Command::new("tmux")
        .args(["display-message", "-p", "-t", pane_id, "#{pane_id}"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn file_mtime_epoch(path: &Path) -> u64 {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(system_time_epoch)
        .unwrap_or(0)
}

fn now_epoch() -> u64 {
    system_time_epoch(SystemTime::now()).unwrap_or(0)
}

fn system_time_epoch(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn read_hook_value(path: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() == key {
            return Some(
                value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string(),
            );
        }
    }
    None
}

fn log_event(config: &AgentRecoveryConfig, event: &str, details: &str) {
    let path = recovery_dir(config).join("recovery.log");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let _ = writeln!(
        file,
        "{} event={} pid={} pane_id={} details={}",
        utc_now(),
        event,
        std::process::id(),
        shell_quote(&config.pane_id),
        shell_quote(details),
    );
}

fn sleep_poll(config: &AgentRecoveryConfig) {
    thread::sleep(Duration::from_secs(config.poll_seconds.max(1)));
}

fn utc_now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '@' | '+'))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_agent_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "clawblox-recovery-{tag}-{}-{}",
            std::process::id(),
            now_epoch()
        ));
        std::fs::create_dir_all(dir.join("runtime")).unwrap();
        dir
    }

    fn config_for(agent_dir: &Path) -> AgentRecoveryConfig {
        AgentRecoveryConfig {
            pane_id: "%1".to_string(),
            agent_dir: agent_dir.to_path_buf(),
            command_file: agent_dir.join("runtime/start.sh"),
            agent_log_file: agent_dir.join("logs/agent.log"),
            idle_grace_seconds: 1,
            stall_grace_seconds: 1,
            restart_backoff_seconds: 1,
            poll_seconds: 1,
        }
    }

    #[test]
    fn respawn_runs_a_private_copy_not_the_agent_visible_command_file() {
        // the watchdog re-executes outside the sandbox, so it
        // must never run a file the agent can reach.
        let dir = temp_agent_dir("private");
        let config = config_for(&dir);
        std::fs::write(&config.command_file, "#!/usr/bin/env bash
echo trusted\n").unwrap();

        let trusted = snapshot_command_file(&config).unwrap();
        std::fs::write(&config.command_file, "#!/usr/bin/env bash
echo tampered\n").unwrap();

        assert!(trusted.path.starts_with(dir.join("recovery")));
        assert!(!trusted.path.starts_with(dir.join("runtime")));
        assert!(respawn_command(&trusted).contains(&trusted.path.display().to_string()));
        assert!(!respawn_command(&trusted).contains("runtime/start.sh"));
        assert_eq!(
            std::fs::read_to_string(&trusted.path).unwrap(),
            "#!/usr/bin/env bash
echo trusted\n"
        );
        assert!(command_file_changed(&config, &trusted));
    }

    #[test]
    fn verify_restores_a_deleted_private_copy_and_ignores_an_unchanged_file() {
        let dir = temp_agent_dir("restore");
        let config = config_for(&dir);
        std::fs::write(&config.command_file, "echo ok\n").unwrap();
        let trusted = snapshot_command_file(&config).unwrap();
        assert!(!command_file_changed(&config, &trusted));

        std::fs::remove_file(&trusted.path).unwrap();
        ensure_trusted_copy(&trusted).unwrap();
        assert_eq!(std::fs::read_to_string(&trusted.path).unwrap(), "echo ok\n");
    }

    #[test]
    fn watchdog_state_lives_outside_the_agent_visible_runtime_dir() {
        let dir = temp_agent_dir("state");
        let config = config_for(&dir);
        log_event(&config, "watch_start", "x");
        assert!(dir.join("recovery/recovery.log").is_file());
        assert!(!dir.join("runtime/recovery.log").exists());
    }

    #[test]
    fn maps_sandbox_home_paths_to_host() {
        let config = AgentRecoveryConfig {
            pane_id: "%1".to_string(),
            agent_dir: PathBuf::from("/tmp/agent"),
            command_file: PathBuf::from("/tmp/agent/runtime/start.sh"),
            agent_log_file: PathBuf::from("/tmp/agent/logs/agent.log"),
            idle_grace_seconds: 1,
            stall_grace_seconds: 1,
            restart_backoff_seconds: 1,
            poll_seconds: 1,
        };

        assert_eq!(
            map_runtime_path(&config, "/home/agent/.claude/transcript.jsonl"),
            Some(PathBuf::from(
                "/tmp/agent/sandbox-home/.claude/transcript.jsonl"
            ))
        );
    }
}
