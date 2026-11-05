use std::{
    collections::HashMap,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::agent::{build_agent, Agent, AgentContext, AgentPaths, AgentSettings, CommandSpec};
use crate::agent_recovery::{start_agent_recovery_watchdog, AgentRecoveryConfig};
use crate::agent_sandbox::{detect_sandbox_deps_root, wrap_command, SandboxLayout};
use crate::config::{SandboxConfig, WorldConfig};
use crate::manifest::record_agent_workspace_recording;
use crate::workspace_recording::{
    start_workspace_recorder, WorkspaceRecorderConfig, WorkspaceRecorderHandle,
};

static WORKSPACE_RECORDERS: OnceLock<Mutex<HashMap<PathBuf, WorkspaceRecorderHandle>>> =
    OnceLock::new();

pub const DEFAULT_SYSTEM_PROMPT: &str = r#"You are a being living in your world.
Your name is ${WORLD_AGENT_NAME}.
Your workspace is ${WORKSPACE_DIR}. Only work inside this directory.
Make sure to keep your workspace clean and well organized.
The files in your workspace are yours to evolve.
These files are your memory that persists across sessions. Anything not in your workspace will be forgotten.
The source for your world is available under ${WORKSPACE_DIR}/world.

Among your workspace, you have two memory files:
- EPISODIC_MEMORY.md describes how to record what happened. Actual entries are appended to EPISODIC_MEMORY.jsonl - never rewrite or delete old ones.
- SEMANTIC_MEMORY.md is for techniques and knowledge you have learned.

Embody SOUL.md's persona and tone.

When starting a new session, start by reading the files in your workspace to refresh your memory.

Periodically take a step back and revise your workspace and memory files. You don't want to forget what you have experienced and learned.

When using the bash tool with timeout, always keep the timeout duration under 30s.

Prefer observing and acting directly over using scripts.

Note: There is an automated stop hook. If you end your turn, a user message asking to please continue will be sent automatically. Just continue doing whatever you want to do. Avoid wasting time in an endless response loop with the stop hook.

World base URL: ${WORLD_BASE_URL}
${SESSION_LINE}

Fetch capabilities with: ${SKILL_CURL} ${SKILL_URL}
"#;

pub fn default_system_prompt() -> &'static str {
    DEFAULT_SYSTEM_PROMPT
}

#[derive(Clone, Debug)]
pub struct AgentRunnerOptions {
    pub kind: String,
    pub name: String,
    pub model: Option<String>,
    pub permission_mode: String,
    pub tmux_session: Option<String>,
    pub agent_dir: Option<PathBuf>,
    pub system_prompt: Option<PathBuf>,
    pub sandbox: bool,
    pub sandbox_config: SandboxConfig,
}

#[derive(Clone, Debug)]
pub struct AgentStartOptions {
    pub kind: String,
    pub name: String,
    pub binary: Option<PathBuf>,
    pub agent_dir: PathBuf,
    pub model: String,
    pub permission_mode: String,
    pub initial_prompt: String,
    pub system_prompt: String,
    pub auto_continue_prompt: String,
    pub tmux_session: Option<String>,
    pub tmux_window: Option<String>,
    pub env: Vec<(String, String)>,
    pub extra_args: Vec<String>,
    pub sandbox: bool,
    pub sandbox_config: SandboxConfig,
    pub bare: bool,
    pub version: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AgentSendOptions {
    pub kind: String,
    pub name: String,
    pub binary: Option<PathBuf>,
    pub agent_dir: PathBuf,
    pub model: String,
    pub permission_mode: String,
    pub prompt: String,
    pub auto_continue_prompt: String,
    pub tmux_session: Option<String>,
    pub tmux_window: Option<String>,
    pub env: Vec<(String, String)>,
    pub extra_args: Vec<String>,
    pub sandbox: bool,
    pub sandbox_config: SandboxConfig,
    pub bare: bool,
    pub version: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AgentStopOptions {
    pub kind: String,
    pub name: String,
    pub binary: Option<PathBuf>,
    pub agent_dir: PathBuf,
    pub model: String,
    pub permission_mode: String,
    pub auto_continue_prompt: String,
    pub tmux_session: Option<String>,
    pub tmux_window: Option<String>,
    pub env: Vec<(String, String)>,
    pub extra_args: Vec<String>,
    pub sandbox: bool,
    pub sandbox_config: SandboxConfig,
    pub bare: bool,
    pub grace_seconds: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartedAgent {
    pub name: String,
    pub kind: &'static str,
    pub tmux_session: String,
    pub tmux_window: Option<String>,
    pub tmux_pane_id: Option<String>,
    pub agent_dir: PathBuf,
    pub command_file: PathBuf,
    pub events_file: PathBuf,
    pub attach_command: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoppedAgent {
    pub name: String,
    pub kind: &'static str,
    pub tmux_session: Option<String>,
    pub agent_dir: PathBuf,
    pub events_file: PathBuf,
    pub log_file: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ActiveAgent {
    name: String,
    kind: String,
    operation: String,
    tmux_session: String,
    tmux_window: Option<String>,
    command_file: PathBuf,
    started_at: String,
}

#[derive(Clone, Debug)]
struct ProcessEventContext {
    agent_name: String,
    agent_kind: String,
    agent_dir: PathBuf,
    events_file: PathBuf,
    log_file: PathBuf,
    tmux_session: Option<String>,
    command_file: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct OperationReport {
    pub time: String,
    pub operation: String,
    pub status: String,
    pub agent_name: String,
    pub agent_kind: String,
    pub agent_dir: PathBuf,
    pub tmux_session: Option<String>,
    pub attach_command: Option<String>,
    pub command_file: Option<PathBuf>,
    pub log_file: Option<PathBuf>,
    pub events_file: PathBuf,
    pub error: Option<String>,
}

pub fn start_agent_tmux_for_local_world(
    world_dir: &Path,
    port: u16,
    base_path: &str,
    options: AgentRunnerOptions,
    bundled_prompt: Option<String>,
) -> Result<StartedAgent, String> {
    let safe_name = sanitize_agent_name(&options.name);
    let agent_dir = options
        .agent_dir
        .clone()
        .unwrap_or_else(|| world_dir.join(".clawblox").join("agents").join(&safe_name));
    let agent = build_agent(&options.kind, None, false)?;
    let sandbox_config = WorldConfig::from_game_dir(world_dir)
        .map(|config| config.sandbox)
        .unwrap_or_else(|_| options.sandbox_config.clone());
    start_agent(AgentStartOptions {
        kind: options.kind,
        name: options.name,
        binary: None,
        version: None,
        agent_dir,
        model: options
            .model
            .unwrap_or_else(|| agent.default_model().to_string()),
        permission_mode: options.permission_mode,
        initial_prompt: "Begin".to_string(),
        system_prompt: match options.system_prompt.as_ref() {
            Some(path) => std::fs::read_to_string(path)
                .map_err(|err| format!("failed to read system prompt {}: {err}", path.display()))?,
            None => bundled_prompt.unwrap_or_else(|| DEFAULT_SYSTEM_PROMPT.to_string()),
        },
        auto_continue_prompt: "Please don't stop. Just continue doing whatever you want to do."
            .to_string(),
        tmux_session: options.tmux_session,
        tmux_window: None,
        env: vec![
            (
                "WORLD_BASE_URL".to_string(),
                local_world_url(port, base_path),
            ),
            (
                "WORLD_INTERNAL_BASE_URL".to_string(),
                local_world_url(port, base_path),
            ),
            ("WORLD_AGENT_NAME".to_string(), safe_name),
        ],
        extra_args: Vec::new(),
        sandbox: options.sandbox,
        sandbox_config,
        bare: false,
    })
}

pub fn start_agent(options: AgentStartOptions) -> Result<StartedAgent, String> {
    let safe_name = sanitize_agent_name(&options.name);
    let agent_dir = absolutize_path(&options.agent_dir);
    let agent = build_agent(&options.kind, options.binary.clone(), options.bare)?;
    let kind_name = agent.name();
    let runtime_dir = agent_dir.join("runtime");
    let log_dir = agent_dir.join("logs");
    let events_file = agent_dir.join("events.jsonl");
    let mut ctx = AgentContext {
        paths: AgentPaths {
            workspace_dir: agent_dir.join("workspace"),
            prompt_file: runtime_dir.join("system_prompt.md"),
            session_id_file: agent_dir.join(format!("{kind_name}_session_id.txt")),
            transcript_dir: agent_dir.join("session"),
            runtime_dir,
            log_file: log_dir.join("agent.log"),
            agent_home_dir: agent_dir.join("home"),
        },
        settings: AgentSettings {
            model: options.model.clone(),
            permission_mode: options.permission_mode.clone(),
            initial_prompt: options.initial_prompt.clone(),
            lifecycle_prompt:
                "You will be reset in 5 minutes. Update your workspace memory files now."
                    .to_string(),
            auto_continue_prompt: options.auto_continue_prompt.clone(),
            extra_args: options.extra_args.clone(),
            env: options.env.clone(),
            sandbox_enabled: options.sandbox,
            version: options.version.clone(),
        },
        sandbox: None,
    };

    if let Err(err) = ctx.prepare_paths() {
        let message = format!("failed to prepare agent paths: {err}");
        let _ = append_operation_report(&events_file, failure_report(&ctx, &options, &message));
        return Err(message);
    }
    if let Err(err) = std::fs::write(&ctx.paths.prompt_file, &options.system_prompt) {
        let message = format!(
            "failed to write agent prompt {}: {err}",
            ctx.paths.prompt_file.display()
        );
        let _ = append_operation_report(&events_file, failure_report(&ctx, &options, &message));
        return Err(message);
    }

    if options.sandbox {
        ctx.sandbox = Some(build_sandbox_layout(
            &ctx,
            agent.as_ref(),
            options.binary.clone(),
            &options.sandbox_config,
        )?);
    }

    if let Err(err) = agent.check_setup(&ctx) {
        let message = format!("{} setup failed: {err}", agent.name());
        let _ = append_operation_report(&events_file, failure_report(&ctx, &options, &message));
        return Err(message);
    }
    if let Err(err) = agent.configure_auto_continue(&ctx) {
        let message = format!("failed to configure {} hooks: {err}", agent.name());
        let _ = append_operation_report(&events_file, failure_report(&ctx, &options, &message));
        return Err(message);
    }
    let command = match agent.start(&ctx) {
        Ok(command) => command,
        Err(err) => {
            let message = format!("failed to build {} command: {err}", agent.name());
            let _ = append_operation_report(&events_file, failure_report(&ctx, &options, &message));
            return Err(message);
        }
    };

    let tmux_session = options
        .tmux_session
        .clone()
        .unwrap_or_else(|| format!("clawblox-{}-{}", agent.name(), safe_name));
    let tmux_window = options.tmux_window.clone();
    if let Err(message) = ensure_no_active_agent(&ctx) {
        let _ = append_operation_report(&events_file, failure_report(&ctx, &options, &message));
        return Err(message);
    }
    if tmux_window.is_none() && tmux_session_exists(&tmux_session) {
        let message = format!(
            "tmux session '{}' already exists. Use --tmux-session or close the old session.",
            tmux_session
        );
        let _ = append_operation_report(&events_file, failure_report(&ctx, &options, &message));
        return Err(message);
    }

    let command = if let Some(layout) = &ctx.sandbox {
        wrap_command(&ctx, layout, command, agent.needs_socat_sandbox()).map_err(|err| {
            let message = format!("failed to build {} sandbox command: {err}", agent.name());
            let _ = append_operation_report(&events_file, failure_report(&ctx, &options, &message));
            message
        })?
    } else {
        command
    };
    let command_file = ctx.paths.runtime_dir.join("start.sh");
    let shell_command = command_spec_to_shell(&command, false);
    if let Err(err) = write_agent_command_file(
        &command_file,
        &shell_command,
        &ProcessEventContext {
            agent_name: options.name.clone(),
            agent_kind: agent.name().to_string(),
            agent_dir: agent_dir.clone(),
            events_file: events_file.clone(),
            log_file: ctx.paths.log_file.clone(),
            tmux_session: Some(tmux_session.clone()),
            command_file: command_file.clone(),
        },
    ) {
        let message = format!(
            "failed to write command file {}: {err}",
            command_file.display()
        );
        let _ = append_operation_report(&events_file, failure_report(&ctx, &options, &message));
        return Err(message);
    }

    let attach_command = format!("tmux attach -t {}", tmux_session);
    append_operation_report(
        &events_file,
        OperationReport {
            time: utc_now(),
            operation: "agent.start".to_string(),
            status: "starting".to_string(),
            agent_name: options.name.clone(),
            agent_kind: agent.name().to_string(),
            agent_dir: agent_dir.clone(),
            tmux_session: Some(tmux_session.clone()),
            attach_command: Some(attach_command.clone()),
            command_file: Some(command_file.clone()),
            log_file: Some(ctx.paths.log_file.clone()),
            events_file: events_file.clone(),
            error: None,
        },
    )
    .map_err(|err| {
        format!(
            "failed to write operation event {}: {err}",
            events_file.display()
        )
    })?;

    let agent_name = agent_display_name_from_env(&ctx.settings.env).unwrap_or(&options.name);
    if let Err(err) =
        start_workspace_recording_for_agent(&ctx, &options.name, agent_name, &options.name)
    {
        let message = format!("failed to start workspace recorder: {err}");
        let _ = append_operation_report(&events_file, failure_report(&ctx, &options, &message));
        return Err(message);
    }

    let pane_id = match start_tmux_target(
        &tmux_session,
        tmux_window.as_deref(),
        &command_file,
        &ctx.paths.log_file,
        &command.env,
    ) {
        Ok(pane_id) => Some(pane_id),
        Err(err) => {
            let message = format!(
                "{err}\nagent dir: {}\ncommand file: {}\nevents file: {}\nattach: {}",
                agent_dir.display(),
                command_file.display(),
                events_file.display(),
                attach_command
            );
            let _ = append_operation_report(&events_file, failure_report(&ctx, &options, &message));
            stop_workspace_recording_for_agent(&agent_dir);
            return Err(message);
        }
    };
    if let Some(input) = agent.initial_tmux_input(&ctx) {
        let session = tmux_session.clone();
        let window = tmux_window.clone();
        std::thread::spawn(move || {
            // Fixed delay waiting for the TUI to be ready. If the agent's TUI
            // takes longer than this to start, the keys will be lost. A more
            // robust solution would poll the pane content for a ready signal.
            std::thread::sleep(std::time::Duration::from_secs(5));
            tmux_send_literal(&session, window.as_deref(), &input);
        });
    }
    if agent.needs_autoloop_watcher() && kimi_autoloop_enabled() {
        start_kimi_autoloop_watcher(KimiAutoloopConfig {
            tmux_session: tmux_session.clone(),
            tmux_window: tmux_window.clone(),
            marker_file: ctx
                .paths
                .log_file
                .parent()
                .unwrap_or(Path::new(""))
                .join("autoloop_fires"),
            message: ctx.settings.auto_continue_prompt.clone(),
            poll: Duration::from_millis(env_u64("CLAWBLOX_KIMI_AUTOLOOP_POLL_MS", 500)),
        });
    }
    if agent.needs_recovery_watchdog() && recovery_watchdog_enabled() {
        if let Some(pane_id) = &pane_id {
            if let Err(err) = start_agent_recovery_watchdog(AgentRecoveryConfig {
                pane_id: pane_id.clone(),
                agent_dir: agent_dir.clone(),
                command_file: command_file.clone(),
                agent_log_file: ctx.paths.log_file.clone(),
                idle_grace_seconds: env_u64("CLAUDE_FAILURE_IDLE_GRACE_SECONDS", 180),
                stall_grace_seconds: env_u64("CLAUDE_FAILURE_STALL_GRACE_SECONDS", 300),
                restart_backoff_seconds: env_u64("CLAUDE_FAILURE_RESTART_BACKOFF_SECONDS", 5),
                poll_seconds: env_u64("CLAUDE_FAILURE_POLL_SECONDS", 2),
            }) {
                let _ = append_operation_report(
                    &events_file,
                    OperationReport {
                        time: utc_now(),
                        operation: "agent.recovery".to_string(),
                        status: "failed".to_string(),
                        agent_name: options.name.clone(),
                        agent_kind: agent.name().to_string(),
                        agent_dir: agent_dir.clone(),
                        tmux_session: Some(tmux_session.clone()),
                        attach_command: Some(attach_command.clone()),
                        command_file: Some(command_file.clone()),
                        log_file: Some(ctx.paths.log_file.clone()),
                        events_file: events_file.clone(),
                        error: Some(err),
                    },
                );
            } else {
                let _ = append_operation_report(
                    &events_file,
                    OperationReport {
                        time: utc_now(),
                        operation: "agent.recovery".to_string(),
                        status: "started".to_string(),
                        agent_name: options.name.clone(),
                        agent_kind: agent.name().to_string(),
                        agent_dir: agent_dir.clone(),
                        tmux_session: Some(tmux_session.clone()),
                        attach_command: Some(attach_command.clone()),
                        command_file: Some(command_file.clone()),
                        log_file: Some(ctx.paths.log_file.clone()),
                        events_file: events_file.clone(),
                        error: None,
                    },
                );
            }
        }
    }
    if let Err(err) = write_active_agent(
        &ctx,
        ActiveAgent {
            name: options.name.clone(),
            kind: agent.name().to_string(),
            operation: "agent.start".to_string(),
            tmux_session: tmux_session.clone(),
            tmux_window: tmux_window.clone(),
            command_file: command_file.clone(),
            started_at: utc_now(),
        },
    ) {
        let message = format!(
            "failed to write active agent state {}: {err}",
            active_agent_file(&ctx).display()
        );
        let _ = append_operation_report(&events_file, failure_report(&ctx, &options, &message));
        return Err(message);
    }

    append_operation_report(
        &events_file,
        OperationReport {
            time: utc_now(),
            operation: "agent.start".to_string(),
            status: "started".to_string(),
            agent_name: options.name.clone(),
            agent_kind: agent.name().to_string(),
            agent_dir: agent_dir.clone(),
            tmux_session: Some(tmux_session.clone()),
            attach_command: Some(attach_command.clone()),
            command_file: Some(command_file.clone()),
            log_file: Some(ctx.paths.log_file.clone()),
            events_file: events_file.clone(),
            error: None,
        },
    )
    .map_err(|err| {
        format!(
            "failed to write operation event {}: {err}",
            events_file.display()
        )
    })?;

    Ok(StartedAgent {
        name: options.name,
        kind: agent.name(),
        tmux_session,
        tmux_window,
        tmux_pane_id: pane_id,
        agent_dir,
        command_file,
        events_file,
        attach_command,
    })
}

pub fn send_agent(options: AgentSendOptions) -> Result<StartedAgent, String> {
    let safe_name = sanitize_agent_name(&options.name);
    let agent_dir = absolutize_path(&options.agent_dir);
    let agent = build_agent(&options.kind, options.binary.clone(), options.bare)?;
    let kind_name = agent.name();
    let runtime_dir = agent_dir.join("runtime");
    let log_dir = agent_dir.join("logs");
    let events_file = agent_dir.join("events.jsonl");
    let mut ctx = AgentContext {
        paths: AgentPaths {
            workspace_dir: agent_dir.join("workspace"),
            prompt_file: runtime_dir.join("system_prompt.md"),
            session_id_file: agent_dir.join(format!("{kind_name}_session_id.txt")),
            transcript_dir: agent_dir.join("session"),
            runtime_dir,
            log_file: log_dir.join("agent.log"),
            agent_home_dir: agent_dir.join("home"),
        },
        settings: AgentSettings {
            model: options.model.clone(),
            permission_mode: options.permission_mode.clone(),
            initial_prompt: "Begin".to_string(),
            lifecycle_prompt:
                "You will be reset in 5 minutes. Update your workspace memory files now."
                    .to_string(),
            auto_continue_prompt: options.auto_continue_prompt.clone(),
            extra_args: options.extra_args.clone(),
            env: options.env.clone(),
            sandbox_enabled: options.sandbox,
            version: options.version.clone(),
        },
        sandbox: None,
    };

    if let Err(err) = ctx.prepare_paths() {
        let message = format!("failed to prepare agent paths: {err}");
        let _ =
            append_operation_report(&events_file, send_failure_report(&ctx, &options, &message));
        return Err(message);
    }

    if options.sandbox {
        ctx.sandbox = Some(build_sandbox_layout(
            &ctx,
            agent.as_ref(),
            options.binary.clone(),
            &options.sandbox_config,
        )?);
    }

    if let Err(err) = agent.check_setup(&ctx) {
        let message = format!("{} setup failed: {err}", agent.name());
        let _ =
            append_operation_report(&events_file, send_failure_report(&ctx, &options, &message));
        return Err(message);
    }
    if let Err(err) = agent.configure_auto_continue(&ctx) {
        let message = format!("failed to configure {} hooks: {err}", agent.name());
        let _ =
            append_operation_report(&events_file, send_failure_report(&ctx, &options, &message));
        return Err(message);
    }
    let command = match agent.continue_session(&ctx, &options.prompt) {
        Ok(command) => command,
        Err(err) => {
            let message = format!("failed to build {} send command: {err}", agent.name());
            let _ = append_operation_report(
                &events_file,
                send_failure_report(&ctx, &options, &message),
            );
            return Err(message);
        }
    };

    let tmux_session = options
        .tmux_session
        .clone()
        .unwrap_or_else(|| format!("clawblox-{}-{}-send", agent.name(), safe_name));
    let tmux_window = options
        .tmux_window
        .as_ref()
        .map(|window| format!("{window}-send"));
    if let Err(message) = ensure_no_active_agent(&ctx) {
        let _ =
            append_operation_report(&events_file, send_failure_report(&ctx, &options, &message));
        return Err(message);
    }
    if tmux_window.is_none() && tmux_session_exists(&tmux_session) {
        let message = format!(
            "tmux session '{}' already exists. Stop the current agent process before sending another prompt.",
            tmux_session
        );
        let _ =
            append_operation_report(&events_file, send_failure_report(&ctx, &options, &message));
        return Err(message);
    }

    let command = if let Some(layout) = &ctx.sandbox {
        wrap_command(&ctx, layout, command, agent.needs_socat_sandbox()).map_err(|err| {
            let message = format!("failed to build {} sandbox command: {err}", agent.name());
            let _ = append_operation_report(
                &events_file,
                send_failure_report(&ctx, &options, &message),
            );
            message
        })?
    } else {
        command
    };
    let command_file = ctx.paths.runtime_dir.join("send.sh");
    let shell_command = command_spec_to_shell(&command, false);
    if let Err(err) = write_agent_command_file(
        &command_file,
        &shell_command,
        &ProcessEventContext {
            agent_name: options.name.clone(),
            agent_kind: agent.name().to_string(),
            agent_dir: agent_dir.clone(),
            events_file: events_file.clone(),
            log_file: ctx.paths.log_file.clone(),
            tmux_session: Some(tmux_session.clone()),
            command_file: command_file.clone(),
        },
    ) {
        let message = format!(
            "failed to write command file {}: {err}",
            command_file.display()
        );
        let _ =
            append_operation_report(&events_file, send_failure_report(&ctx, &options, &message));
        return Err(message);
    }

    let attach_command = format!("tmux attach -t {}", tmux_session);
    append_operation_report(
        &events_file,
        OperationReport {
            time: utc_now(),
            operation: "agent.send".to_string(),
            status: "starting".to_string(),
            agent_name: options.name.clone(),
            agent_kind: agent.name().to_string(),
            agent_dir: agent_dir.clone(),
            tmux_session: Some(tmux_session.clone()),
            attach_command: Some(attach_command.clone()),
            command_file: Some(command_file.clone()),
            log_file: Some(ctx.paths.log_file.clone()),
            events_file: events_file.clone(),
            error: None,
        },
    )
    .map_err(|err| {
        format!(
            "failed to write operation event {}: {err}",
            events_file.display()
        )
    })?;

    let agent_name = agent_display_name_from_env(&ctx.settings.env).unwrap_or(&options.name);
    if let Err(err) =
        start_workspace_recording_for_agent(&ctx, &options.name, agent_name, &options.name)
    {
        let message = format!("failed to start workspace recorder: {err}");
        let _ =
            append_operation_report(&events_file, send_failure_report(&ctx, &options, &message));
        return Err(message);
    }

    let pane_id = match start_tmux_target(
        &tmux_session,
        tmux_window.as_deref(),
        &command_file,
        &ctx.paths.log_file,
        &command.env,
    ) {
        Ok(pane_id) => Some(pane_id),
        Err(err) => {
            let message = format!(
                "{err}\nagent dir: {}\ncommand file: {}\nevents file: {}\nattach: {}",
                agent_dir.display(),
                command_file.display(),
                events_file.display(),
                attach_command
            );
            let _ = append_operation_report(
                &events_file,
                send_failure_report(&ctx, &options, &message),
            );
            stop_workspace_recording_for_agent(&agent_dir);
            return Err(message);
        }
    };
    if let Some(input) = agent.send_tmux_input(&ctx, &options.prompt) {
        let session = tmux_session.clone();
        let window = tmux_window.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(5));
            tmux_send_literal(&session, window.as_deref(), &input);
        });
    }
    if let Err(err) = write_active_agent(
        &ctx,
        ActiveAgent {
            name: options.name.clone(),
            kind: agent.name().to_string(),
            operation: "agent.send".to_string(),
            tmux_session: tmux_session.clone(),
            tmux_window: tmux_window.clone(),
            command_file: command_file.clone(),
            started_at: utc_now(),
        },
    ) {
        let message = format!(
            "failed to write active agent state {}: {err}",
            active_agent_file(&ctx).display()
        );
        let _ =
            append_operation_report(&events_file, send_failure_report(&ctx, &options, &message));
        return Err(message);
    }

    append_operation_report(
        &events_file,
        OperationReport {
            time: utc_now(),
            operation: "agent.send".to_string(),
            status: "started".to_string(),
            agent_name: options.name.clone(),
            agent_kind: agent.name().to_string(),
            agent_dir: agent_dir.clone(),
            tmux_session: Some(tmux_session.clone()),
            attach_command: Some(attach_command.clone()),
            command_file: Some(command_file.clone()),
            log_file: Some(ctx.paths.log_file.clone()),
            events_file: events_file.clone(),
            error: None,
        },
    )
    .map_err(|err| {
        format!(
            "failed to write operation event {}: {err}",
            events_file.display()
        )
    })?;

    Ok(StartedAgent {
        name: options.name,
        kind: agent.name(),
        tmux_session,
        tmux_window,
        tmux_pane_id: pane_id,
        agent_dir,
        command_file,
        events_file,
        attach_command,
    })
}

pub fn stop_agent(options: AgentStopOptions) -> Result<StoppedAgent, String> {
    let agent_dir = absolutize_path(&options.agent_dir);
    let agent = build_agent(&options.kind, options.binary.clone(), options.bare)?;
    let kind_name = agent.name();
    let runtime_dir = agent_dir.join("runtime");
    let log_dir = agent_dir.join("logs");
    let events_file = agent_dir.join("events.jsonl");
    let ctx = AgentContext {
        paths: AgentPaths {
            workspace_dir: agent_dir.join("workspace"),
            prompt_file: runtime_dir.join("system_prompt.md"),
            session_id_file: agent_dir.join(format!("{kind_name}_session_id.txt")),
            transcript_dir: agent_dir.join("session"),
            runtime_dir,
            log_file: log_dir.join("agent.log"),
            agent_home_dir: agent_dir.join("home"),
        },
        settings: AgentSettings {
            model: options.model.clone(),
            permission_mode: options.permission_mode.clone(),
            initial_prompt: "Begin".to_string(),
            lifecycle_prompt:
                "You will be reset in 5 minutes. Update your workspace memory files now."
                    .to_string(),
            auto_continue_prompt: options.auto_continue_prompt.clone(),
            extra_args: options.extra_args.clone(),
            env: options.env.clone(),
            sandbox_enabled: options.sandbox,
            version: None,
        },
        sandbox: None,
    };

    if let Err(err) = ctx.prepare_paths() {
        let message = format!("failed to prepare agent paths: {err}");
        let _ =
            append_operation_report(&events_file, stop_failure_report(&ctx, &options, &message));
        return Err(message);
    }

    let active = match read_active_agent(&ctx) {
        Ok(active) => active,
        Err(err) => {
            let message = format!(
                "failed to read active agent state {}: {err}",
                active_agent_file(&ctx).display()
            );
            let _ = append_operation_report(
                &events_file,
                stop_failure_report(&ctx, &options, &message),
            );
            return Err(message);
        }
    };
    let tmux_session = active
        .as_ref()
        .map(|active| active.tmux_session.clone())
        .or_else(|| options.tmux_session.clone());
    let tmux_window = active
        .as_ref()
        .and_then(|active| active.tmux_window.clone())
        .or_else(|| options.tmux_window.clone());

    append_operation_report(
        &events_file,
        OperationReport {
            time: utc_now(),
            operation: "agent.stop".to_string(),
            status: "stopping".to_string(),
            agent_name: options.name.clone(),
            agent_kind: agent.name().to_string(),
            agent_dir: agent_dir.clone(),
            tmux_session: tmux_session.clone(),
            attach_command: tmux_session
                .as_ref()
                .map(|session| format!("tmux attach -t {session}")),
            command_file: active.as_ref().map(|active| active.command_file.clone()),
            log_file: Some(ctx.paths.log_file.clone()),
            events_file: events_file.clone(),
            error: None,
        },
    )
    .map_err(|err| {
        format!(
            "failed to write operation event {}: {err}",
            events_file.display()
        )
    })?;

    if let Some(session) = &tmux_session {
        if tmux_target_exists(session, tmux_window.as_deref()) {
            send_tmux_ctrl_c(session, tmux_window.as_deref());
            let deadline = Instant::now() + Duration::from_secs(options.grace_seconds);
            while Instant::now() < deadline {
                if !tmux_target_exists(session, tmux_window.as_deref()) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            if tmux_target_exists(session, tmux_window.as_deref()) {
                kill_tmux_target(session, tmux_window.as_deref());
            }
            if tmux_target_exists(session, tmux_window.as_deref()) {
                let message = format!(
                    "tmux target '{}' still exists after stop. Attach with: tmux attach -t {}",
                    tmux_target(session, tmux_window.as_deref()),
                    session
                );
                let _ = append_operation_report(
                    &events_file,
                    stop_failure_report(&ctx, &options, &message),
                );
                return Err(message);
            }
        }
    }

    stop_workspace_recording_for_agent(&agent_dir);

    if let Err(err) = agent.collect_transcripts(&ctx) {
        let message = format!("failed to collect {} transcript: {err}", agent.name());
        let _ =
            append_operation_report(&events_file, stop_failure_report(&ctx, &options, &message));
        return Err(message);
    }
    prune_recreateable_agent_artifacts(&ctx, &options, agent.name(), &events_file)?;
    let _ = std::fs::remove_file(active_agent_file(&ctx));

    append_operation_report(
        &events_file,
        OperationReport {
            time: utc_now(),
            operation: "agent.stop".to_string(),
            status: "stopped".to_string(),
            agent_name: options.name.clone(),
            agent_kind: agent.name().to_string(),
            agent_dir: agent_dir.clone(),
            tmux_session: tmux_session.clone(),
            attach_command: None,
            command_file: active.as_ref().map(|active| active.command_file.clone()),
            log_file: Some(ctx.paths.log_file.clone()),
            events_file: events_file.clone(),
            error: None,
        },
    )
    .map_err(|err| {
        format!(
            "failed to write operation event {}: {err}",
            events_file.display()
        )
    })?;

    Ok(StoppedAgent {
        name: options.name,
        kind: agent.name(),
        tmux_session,
        agent_dir,
        events_file,
        log_file: ctx.paths.log_file,
    })
}

fn prune_recreateable_agent_artifacts(
    ctx: &AgentContext,
    options: &AgentStopOptions,
    agent_kind: &str,
    events_file: &Path,
) -> Result<(), String> {
    let agent_dir = agent_dir_from_context(ctx);
    let mut removed = Vec::new();
    for path in [
        ctx.paths.workspace_dir.join(".venv"),
        ctx.paths.runtime_dir.join("python"),
        agent_dir
            .join("sandbox-home")
            .join(".local")
            .join("share")
            .join("claude")
            .join("versions"),
    ] {
        if !path.exists() {
            continue;
        }
        remove_recreateable_artifact(&path).map_err(|err| {
            format!(
                "failed to prune recreateable agent artifact '{}': {err}",
                path.display()
            )
        })?;
        removed.push(path);
    }
    if removed.is_empty() {
        return Ok(());
    }
    append_operation_report(
        events_file,
        OperationReport {
            time: utc_now(),
            operation: "agent.cleanup".to_string(),
            status: "pruned_recreateable_artifacts".to_string(),
            agent_name: options.name.clone(),
            agent_kind: agent_kind.to_string(),
            agent_dir,
            tmux_session: options.tmux_session.clone(),
            attach_command: None,
            command_file: None,
            log_file: Some(ctx.paths.log_file.clone()),
            events_file: events_file.to_path_buf(),
            error: None,
        },
    )
    .map_err(|err| {
        format!(
            "failed to write cleanup event {}: {err}",
            events_file.display()
        )
    })
}

fn start_workspace_recording_for_agent(
    ctx: &AgentContext,
    agent_id: &str,
    agent_name: &str,
    run_name: &str,
) -> Result<(), String> {
    let agent_dir = agent_dir_from_context(ctx);
    let config = WorkspaceRecorderConfig::for_agent(agent_id, agent_name, &agent_dir);
    if let Some(manifest_path) = manifest_path_from_env(&ctx.settings.env) {
        record_agent_workspace_recording(
            &manifest_path,
            &agent_dir,
            agent_id,
            agent_name,
            run_name,
            &config.events_file,
            &config.blobs_dir,
        )?;
    }

    let registry = WORKSPACE_RECORDERS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut recorders = registry
        .lock()
        .map_err(|_| "workspace recorder registry lock was poisoned".to_string())?;
    if recorders.contains_key(&agent_dir) {
        return Ok(());
    }
    let handle = start_workspace_recorder(config)?;
    recorders.insert(agent_dir, handle);
    Ok(())
}

fn agent_display_name_from_env(env: &[(String, String)]) -> Option<&str> {
    env.iter()
        .find(|(key, value)| key == "WORLD_AGENT_NAME" && !value.trim().is_empty())
        .map(|(_, value)| value.as_str())
}

fn stop_workspace_recording_for_agent(agent_dir: &Path) {
    let Some(registry) = WORKSPACE_RECORDERS.get() else {
        return;
    };
    let Ok(mut recorders) = registry.lock() else {
        return;
    };
    if let Some(handle) = recorders.remove(agent_dir) {
        handle.stop();
    }
}

fn manifest_path_from_env(env: &[(String, String)]) -> Option<PathBuf> {
    env.iter().find_map(|(key, value)| {
        if key == "CLAWBLOX_MANIFEST_PATH" && !value.trim().is_empty() {
            Some(PathBuf::from(value))
        } else {
            None
        }
    })
}

fn remove_recreateable_artifact(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

fn build_sandbox_layout(
    ctx: &AgentContext,
    agent: &dyn Agent,
    binary: Option<PathBuf>,
    sandbox_config: &SandboxConfig,
) -> Result<SandboxLayout, String> {
    let host_binary = resolve_agent_binary(agent, binary)?;
    let mut layout =
        SandboxLayout::for_agent(ctx, host_binary.clone(), agent.sandbox_guest_binary_name())
            .with_host_deps_root(detect_sandbox_deps_root(
                sandbox_config,
                agent.needs_socat_sandbox(),
            )?)
            .with_add_deps_lib_to_library_path(sandbox_config.add_deps_lib_to_library_path)
            .with_enable_weaker_nested_sandbox(sandbox_config.enable_weaker_nested_sandbox)
            .with_hosts_file(ctx.paths.runtime_dir.join("sandbox_hosts"));
    // network isolation, fail-closed on a bad config.
    if let Some(mut bridges) = crate::agent_sandbox::net_bridges_from_env()? {
        if let Some(port) = ctx
            .settings
            .env
            .iter()
            .rfind(|(k, _)| k == "CLAWBLOX_WORLD_HTTP_PROXY_PORT")
            .and_then(|(_, v)| v.parse::<u16>().ok())
        {
            bridges.push(crate::agent_sandbox::NetBridge::new("world", "127.0.0.1", port));
        }
        layout = layout.with_net_bridges(bridges);
    }
    if let Some((_, shm_dir)) = ctx.settings.env.iter().rfind(|(k, _)| k == "WORLD_SHM_DIR") {
        // External run-command worlds may need a state-local /dev/shm that is
        // shared with sandboxed agents without exposing the host's global shm.
        layout = layout.with_shm_dir(PathBuf::from(shm_dir));
    }
    agent
        .configure_sandbox_layout(layout, &host_binary)
        .map_err(|e| e.to_string())
}

fn resolve_agent_binary(agent: &dyn Agent, binary: Option<PathBuf>) -> Result<PathBuf, String> {
    let candidate = binary.unwrap_or_else(|| agent.binary().to_path_buf());
    if candidate.components().count() > 1 || candidate.is_absolute() {
        return canonicalize_if_possible(&candidate);
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return Err(format!("{} CLI was not found in PATH", agent.name()));
    };
    for dir in std::env::split_paths(&paths) {
        let path = dir.join(&candidate);
        if path.exists() {
            return canonicalize_if_possible(&path);
        }
    }
    Err(format!("{} CLI was not found in PATH", agent.name()))
}

fn canonicalize_if_possible(path: &Path) -> Result<PathBuf, String> {
    if !path.exists() {
        return Err(format!("agent binary was not found at {}", path.display()));
    }
    let resolved = std::fs::canonicalize(path)
        .map_err(|err| format!("failed to canonicalize {}: {err}", path.display()))?;
    let metadata = std::fs::metadata(&resolved).map_err(|err| {
        format!(
            "failed to inspect agent binary {}: {err}",
            resolved.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "agent binary is not a file: {}",
            resolved.display()
        ));
    }
    if metadata.len() == 0 {
        return Err(format!("agent binary is empty: {}", resolved.display()));
    }
    Ok(resolved)
}

fn local_world_url(port: u16, base_path: &str) -> String {
    format!("http://localhost:{port}{}", base_path.trim_end_matches('/'))
}

fn tmux_session_exists(name: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn tmux_target(session: &str, window: Option<&str>) -> String {
    match window {
        Some(window) => format!("{session}:{window}"),
        None => session.to_string(),
    }
}

fn tmux_target_exists(session: &str, window: Option<&str>) -> bool {
    match window {
        Some(window) => Command::new("tmux")
            .args(["list-windows", "-t", session, "-F", "#{window_name}"])
            .stderr(std::process::Stdio::null())
            .output()
            .map(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .any(|name| name == window)
            })
            .unwrap_or(false),
        None => tmux_session_exists(session),
    }
}

fn start_tmux_target(
    tmux_session: &str,
    tmux_window: Option<&str>,
    command_file: &Path,
    log_file: &Path,
    env: &[(String, String)],
) -> Result<String, String> {
    prepare_agent_log(log_file)
        .map_err(|err| format!("failed to prepare agent log {}: {err}", log_file.display()))?;
    let tmux_command = format!("exec {}", shell_quote(&command_file.display().to_string()));
    let mut command = Command::new("tmux");
    if let Some(window) = tmux_window {
        if tmux_session_exists(tmux_session) {
            command.args(["new-window", "-d", "-t", tmux_session, "-n", window]);
        } else {
            command.args(["new-session", "-d", "-s", tmux_session, "-n", window]);
        }
    } else {
        command.args(["new-session", "-d", "-s", tmux_session]);
    }
    for (key, value) in env {
        command.args(["-e", &format!("{key}={value}")]);
        command.env(key, value);
    }
    let status = command
        .arg(&tmux_command)
        .status()
        .map_err(|err| format!("failed to start tmux: {err}"))?;
    if !status.success() {
        return Err(format!(
            "tmux failed to start agent target '{}'",
            tmux_target(tmux_session, tmux_window)
        ));
    }
    if let Some(parent) = log_file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create log directory {}: {err}", parent.display()))?;
    }
    let pipe_command = format!("cat >> {}", shell_quote(&log_file.display().to_string()));
    let status = Command::new("tmux")
        .args([
            "pipe-pane",
            "-o",
            "-t",
            &tmux_target(tmux_session, tmux_window),
            &pipe_command,
        ])
        .status()
        .map_err(|err| format!("failed to attach tmux log pipe: {err}"))?;
    if !status.success() {
        kill_tmux_target(tmux_session, tmux_window);
        return Err(format!(
            "tmux failed to attach log pipe for agent target '{}'",
            tmux_target(tmux_session, tmux_window)
        ));
    }
    tmux_pane_id(tmux_session, tmux_window)
}

fn tmux_pane_id(tmux_session: &str, tmux_window: Option<&str>) -> Result<String, String> {
    let output = Command::new("tmux")
        .args([
            "display-message",
            "-p",
            "-t",
            &tmux_target(tmux_session, tmux_window),
            "#{pane_id}",
        ])
        .output()
        .map_err(|err| format!("failed to read tmux pane id: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "tmux failed to report pane id for target '{}'",
            tmux_target(tmux_session, tmux_window)
        ));
    }
    let pane_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if pane_id.is_empty() {
        return Err(format!(
            "tmux reported an empty pane id for target '{}'",
            tmux_target(tmux_session, tmux_window)
        ));
    }
    Ok(pane_id)
}

fn tmux_send_literal(session: &str, window: Option<&str>, text: &str) {
    let target = tmux_target(session, window);
    // Deliver the text through a tmux paste buffer rather than `send-keys -l`.
    // `send-keys` passes the whole string as one command over tmux's
    // client/server socket, which rejects payloads larger than ~16 KiB with
    // "command too long" and silently drops the input. `load-buffer -` reads the
    // text from stdin (no length limit); `paste-buffer -p` then pastes it using
    // bracketed paste, so multi-line text is delivered as a single input instead
    // of being submitted line by line. Enter is sent separately to submit,
    // matching the previous behaviour.
    let buffer = format!("clawblox-{}", target.replace([':', '.'], "-"));
    let loaded = load_tmux_buffer(&buffer, text);
    if loaded {
        let _ = Command::new("tmux")
            .args(["paste-buffer", "-d", "-p", "-b", &buffer, "-t", &target])
            .status();
    } else {
        // Fallback for the small-input case if the buffer could not be loaded.
        let _ = Command::new("tmux")
            .args(["send-keys", "-t", &target, "-l", text])
            .status();
    }
    let _ = Command::new("tmux")
        .args(["send-keys", "-t", &target, "", "Enter"])
        .status();
}

fn load_tmux_buffer(buffer: &str, text: &str) -> bool {
    let mut child = match Command::new("tmux")
        .args(["load-buffer", "-b", buffer, "-"])
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(text.as_bytes()).is_err() {
            return false;
        }
    }
    child.wait().map(|status| status.success()).unwrap_or(false)
}

fn recovery_watchdog_enabled() -> bool {
    !matches!(
        std::env::var("CLAWBLOX_CLAUDE_RECOVERY")
            .unwrap_or_else(|_| "1".to_string())
            .to_ascii_lowercase()
            .as_str(),
        "0" | "false" | "no" | "off"
    )
}

fn kimi_autoloop_enabled() -> bool {
    !matches!(
        std::env::var("CLAWBLOX_KIMI_AUTOLOOP_WATCHER")
            .unwrap_or_else(|_| "1".to_string())
            .to_ascii_lowercase()
            .as_str(),
        "0" | "false" | "no" | "off"
    )
}

struct KimiAutoloopConfig {
    tmux_session: String,
    tmux_window: Option<String>,
    marker_file: PathBuf,
    message: String,
    poll: Duration,
}

/// Driver-side auto-continue loop for Kimi — the analogue of Claude/Codex's
/// self-looping Stop hook, implemented here because Kimi cannot loop in the hook.
///
/// WHY THIS EXISTS, AND WHY WE DON'T JUST USE THE KIMI STOP HOOK:
/// Claude and Codex keep an agent running by returning `decision:block` from
/// their Stop hook on every stop; their runtimes re-block indefinitely so the
/// turn never ends. Kimi is different: its Stop hook is documented to continue a
/// turn *at most once* ("the turn may continue once"), and the cap is enforced in
/// Kimi's runtime above the hook — after one block-continuation Kimi stops
/// invoking the Stop hook for that turn (verified in agent-core `turn/index.ts`
/// and empirically: exactly one continuation per turn). So a blocking Kimi Stop
/// hook gives one extra step and then the agent goes idle, and the cap is not
/// liftable from config or the hook script (only by patching Kimi, which we
/// don't do).
///
/// So the Kimi Stop hook is configured NON-BLOCKING and used purely as a
/// turn-ended signal: it appends a line to `marker_file` on every turn-end (a
/// non-blocking hook never trips the one-continuation cap, so it fires every
/// turn). This watcher tails that file and, on each new line, types the same
/// auto-continue message into the live Kimi TUI via tmux, starting the next
/// turn. The model-visible conversation is identical to Claude/Codex
/// (`assistant stop -> "please don't stop" -> continue`); the only difference is
/// a sub-second driver round-trip per turn instead of an in-process block.
///
/// Lifetime: the thread runs inside the process that called `start_agent` (the
/// orchestrator, which stays alive for the generation). It exits when the
/// agent's tmux window disappears — `stop_agent` kills the window at generation
/// end, so the watcher never re-prompts during the SAVE_PROMPT / shutdown phase.
fn start_kimi_autoloop_watcher(cfg: KimiAutoloopConfig) {
    std::thread::spawn(move || {
        // Baseline at the current count so pre-existing fires are never replayed.
        let mut last = count_nonempty_lines(&cfg.marker_file);
        loop {
            std::thread::sleep(cfg.poll);
            if !tmux_target_exists(&cfg.tmux_session, cfg.tmux_window.as_deref()) {
                return; // agent window gone (stopped) — end the loop
            }
            let fires = count_nonempty_lines(&cfg.marker_file);
            if fires > last {
                // The hook only fires at turn-end, after which the TUI is idle,
                // so it is safe to type the next prompt now. Only the watcher
                // starts new turns, so fires increment one at a time.
                tmux_send_literal(&cfg.tmux_session, cfg.tmux_window.as_deref(), &cfg.message);
                last = fires;
            }
        }
    });
}

fn count_nonempty_lines(path: &Path) -> u64 {
    std::fs::read_to_string(path)
        .map(|text| text.lines().filter(|line| !line.trim().is_empty()).count() as u64)
        .unwrap_or(0)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn send_tmux_ctrl_c(tmux_session: &str, tmux_window: Option<&str>) {
    let _ = Command::new("tmux")
        .args([
            "send-keys",
            "-t",
            &tmux_target(tmux_session, tmux_window),
            "C-c",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

fn kill_tmux_target(tmux_session: &str, tmux_window: Option<&str>) {
    let mut command = Command::new("tmux");
    if let Some(window) = tmux_window {
        command.args([
            "kill-window",
            "-t",
            &tmux_target(tmux_session, Some(window)),
        ]);
    } else {
        command.args(["kill-session", "-t", tmux_session]);
    }
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let _ = command.status();
}

#[cfg(test)]
fn write_command_file(path: &Path, shell_command: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        format!("#!/usr/bin/env bash\nset -euo pipefail\n{shell_command}\n"),
    )?;
    make_executable(path)
}

/// the agent log is appended to from outside the sandbox (tmux's
/// pipe and start.sh), so it must be a regular file, never a link the agent planted.
/// An existing regular log is kept.
pub(crate) fn prepare_agent_log(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_file() => return Ok(()),
        Ok(meta) if meta.is_dir() => {
            return Err(std::io::Error::other(format!("{} is a directory", path.display())))
        }
        Ok(_) => std::fs::remove_file(path)?,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }
    std::fs::OpenOptions::new().create_new(true).append(true).open(path)?;
    Ok(())
}

fn write_agent_command_file(
    path: &Path,
    shell_command: &str,
    event: &ProcessEventContext,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmux_session = event.tmux_session.as_deref().unwrap_or("");
    crate::agent::write_no_follow(
        path,
        format!(
            "#!/usr/bin/env bash\n\
set -uo pipefail\n\
CLAWBLOX_AGENT_NAME={agent_name}\n\
CLAWBLOX_AGENT_KIND={agent_kind}\n\
CLAWBLOX_AGENT_DIR={agent_dir}\n\
CLAWBLOX_AGENT_EVENTS_FILE={events_file}\n\
CLAWBLOX_AGENT_LOG_FILE={log_file}\n\
CLAWBLOX_AGENT_TMUX_SESSION={tmux_session}\n\
CLAWBLOX_AGENT_COMMAND_FILE={command_file}\n\
json_escape() {{\n\
  local s=${{1-}}\n\
  s=${{s//\\\\/\\\\\\\\}}\n\
  s=${{s//\\\"/\\\\\\\"}}\n\
  s=${{s//$'\\n'/\\\\n}}\n\
  s=${{s//$'\\r'/\\\\r}}\n\
  s=${{s//$'\\t'/\\\\t}}\n\
  printf '%s' \"$s\"\n\
}}\n\
append_process_event() {{\n\
  local status=$1 exit_code=$2 error_json=$3 now\n\
  now=$(date -u +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || date -u)\n\
  mkdir -p \"$(dirname \"$CLAWBLOX_AGENT_EVENTS_FILE\")\" \"$(dirname \"$CLAWBLOX_AGENT_LOG_FILE\")\"\n\
  printf '{{\"time\":\"%s\",\"operation\":\"agent.process\",\"status\":\"%s\",\"agent_name\":\"%s\",\"agent_kind\":\"%s\",\"agent_dir\":\"%s\",\"tmux_session\":%s,\"attach_command\":%s,\"command_file\":\"%s\",\"log_file\":\"%s\",\"events_file\":\"%s\",\"exit_code\":%s,\"error\":%s}}\\n' \\\n\
    \"$(json_escape \"$now\")\" \\\n\
    \"$(json_escape \"$status\")\" \\\n\
    \"$(json_escape \"$CLAWBLOX_AGENT_NAME\")\" \\\n\
    \"$(json_escape \"$CLAWBLOX_AGENT_KIND\")\" \\\n\
    \"$(json_escape \"$CLAWBLOX_AGENT_DIR\")\" \\\n\
    \"$(if [[ -n \"$CLAWBLOX_AGENT_TMUX_SESSION\" ]]; then printf '\\\"%s\\\"' \"$(json_escape \"$CLAWBLOX_AGENT_TMUX_SESSION\")\"; else printf 'null'; fi)\" \\\n\
    \"$(if [[ -n \"$CLAWBLOX_AGENT_TMUX_SESSION\" ]]; then printf '\\\"tmux attach -t %s\\\"' \"$(json_escape \"$CLAWBLOX_AGENT_TMUX_SESSION\")\"; else printf 'null'; fi)\" \\\n\
    \"$(json_escape \"$CLAWBLOX_AGENT_COMMAND_FILE\")\" \\\n\
    \"$(json_escape \"$CLAWBLOX_AGENT_LOG_FILE\")\" \\\n\
    \"$(json_escape \"$CLAWBLOX_AGENT_EVENTS_FILE\")\" \\\n\
    \"$exit_code\" \\\n\
    \"$error_json\" >>\"$CLAWBLOX_AGENT_EVENTS_FILE\"\n\
}}\n\
append_process_event running null null\n\
printf '[clawblox] agent process starting: %s\\n' \"$(date -u +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || date -u)\" >>\"$CLAWBLOX_AGENT_LOG_FILE\"\n\
set +e\n\
{shell_command}\n\
status=$?\n\
set -e\n\
if [[ \"$status\" -eq 0 ]]; then\n\
  append_process_event exited \"$status\" null\n\
else\n\
  error=\"agent command exited with status $status\"\n\
  printf '[clawblox] %s\\n' \"$error\" >>\"$CLAWBLOX_AGENT_LOG_FILE\"\n\
  append_process_event exited \"$status\" \"\\\"$(json_escape \"$error\")\\\"\"\n\
fi\n\
exit \"$status\"\n",
            agent_name = shell_quote(&event.agent_name),
            agent_kind = shell_quote(&event.agent_kind),
            agent_dir = shell_quote(&event.agent_dir.display().to_string()),
            events_file = shell_quote(&event.events_file.display().to_string()),
            log_file = shell_quote(&event.log_file.display().to_string()),
            tmux_session = shell_quote(tmux_session),
            command_file = shell_quote(&event.command_file.display().to_string()),
        ).as_bytes(),
    )?;
    make_executable(path)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

fn append_operation_report(path: &Path, report: OperationReport) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(&report)?;
    writeln!(file, "{line}")?;
    Ok(())
}

fn active_agent_file(ctx: &AgentContext) -> PathBuf {
    ctx.paths.runtime_dir.join("active.json")
}

fn read_active_agent(ctx: &AgentContext) -> Result<Option<ActiveAgent>, String> {
    let path = active_agent_file(ctx);
    if !path.exists() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(&path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    serde_json::from_str(&contents)
        .map(Some)
        .map_err(|err| format!("failed to parse {}: {err}", path.display()))
}

fn write_active_agent(ctx: &AgentContext, active: ActiveAgent) -> std::io::Result<()> {
    let path = active_agent_file(ctx);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::agent::write_no_follow(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&active)?).as_bytes(),
    )
}

fn ensure_no_active_agent(ctx: &AgentContext) -> Result<(), String> {
    let Some(active) = read_active_agent(ctx)? else {
        return Ok(());
    };
    if tmux_target_exists(&active.tmux_session, active.tmux_window.as_deref()) {
        return Err(format!(
            "agent '{}' already has an active process in tmux target '{}'. Stop it before starting another prompt. Attach with: tmux attach -t {}",
            active.name,
            tmux_target(&active.tmux_session, active.tmux_window.as_deref()),
            active.tmux_session
        ));
    }
    let _ = std::fs::remove_file(active_agent_file(ctx));
    Ok(())
}

fn failure_report(ctx: &AgentContext, options: &AgentStartOptions, error: &str) -> OperationReport {
    OperationReport {
        time: utc_now(),
        operation: "agent.start".to_string(),
        status: "failed".to_string(),
        agent_name: options.name.clone(),
        agent_kind: options.kind.clone(),
        agent_dir: agent_dir_from_context(ctx),
        tmux_session: options.tmux_session.clone(),
        attach_command: options
            .tmux_session
            .as_ref()
            .map(|session| format!("tmux attach -t {session}")),
        command_file: Some(ctx.paths.runtime_dir.join("start.sh")),
        log_file: Some(ctx.paths.log_file.clone()),
        events_file: agent_dir_from_context(ctx).join("events.jsonl"),
        error: Some(error.to_string()),
    }
}

fn send_failure_report(
    ctx: &AgentContext,
    options: &AgentSendOptions,
    error: &str,
) -> OperationReport {
    OperationReport {
        time: utc_now(),
        operation: "agent.send".to_string(),
        status: "failed".to_string(),
        agent_name: options.name.clone(),
        agent_kind: options.kind.clone(),
        agent_dir: agent_dir_from_context(ctx),
        tmux_session: options.tmux_session.clone(),
        attach_command: options
            .tmux_session
            .as_ref()
            .map(|session| format!("tmux attach -t {session}")),
        command_file: Some(ctx.paths.runtime_dir.join("send.sh")),
        log_file: Some(ctx.paths.log_file.clone()),
        events_file: agent_dir_from_context(ctx).join("events.jsonl"),
        error: Some(error.to_string()),
    }
}

fn stop_failure_report(
    ctx: &AgentContext,
    options: &AgentStopOptions,
    error: &str,
) -> OperationReport {
    OperationReport {
        time: utc_now(),
        operation: "agent.stop".to_string(),
        status: "failed".to_string(),
        agent_name: options.name.clone(),
        agent_kind: options.kind.clone(),
        agent_dir: agent_dir_from_context(ctx),
        tmux_session: options.tmux_session.clone(),
        attach_command: options
            .tmux_session
            .as_ref()
            .map(|session| format!("tmux attach -t {session}")),
        command_file: None,
        log_file: Some(ctx.paths.log_file.clone()),
        events_file: agent_dir_from_context(ctx).join("events.jsonl"),
        error: Some(error.to_string()),
    }
}

fn agent_dir_from_context(ctx: &AgentContext) -> PathBuf {
    ctx.paths
        .runtime_dir
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_path_buf()
}

fn utc_now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn command_spec_to_shell(command: &CommandSpec, exec_program: bool) -> String {
    let mut parts = Vec::new();
    parts.push("cd".to_string());
    parts.push(shell_quote(&command.cwd.display().to_string()));
    parts.push("&&".to_string());
    if exec_program {
        parts.push("exec".to_string());
    }
    if !command.env.is_empty() {
        parts.push("env".to_string());
        for (key, value) in &command.env {
            if is_sensitive_env_key(key) {
                parts.push(format!("{key}=\"${{{key}:?set {key}}}\""));
            } else {
                parts.push(format!("{key}={}", shell_quote(value)));
            }
        }
    }
    parts.push(shell_quote(&command.program.display().to_string()));
    for arg in &command.args {
        parts.push(shell_quote(arg));
    }
    parts.join(" ")
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn sanitize_agent_name(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "agent".to_string()
    } else {
        sanitized
    }
}

fn is_sensitive_env_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    upper.contains("TOKEN")
        || upper.contains("SECRET")
        || upper.contains("PASSWORD")
        || upper.contains("API_KEY")
}

fn absolutize_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "clawblox-runner-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(unix)]
    #[test]
    fn host_writes_replace_a_planted_symlink_instead_of_following_it() {
        // a symlink the agent planted in an agent-writable dir
        // must not redirect a host-side write to another file.
        let dir = temp_dir("symlink");
        let victim = dir.join("victim.txt");
        std::fs::write(&victim, "untouched").unwrap();
        let path = dir.join("start.sh");
        std::os::unix::fs::symlink(&victim, &path).unwrap();

        crate::agent::write_no_follow(&path, b"new").unwrap();

        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "untouched");
        assert!(!std::fs::symlink_metadata(&path).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    }

    #[cfg(unix)]
    #[test]
    fn host_writes_rewrite_a_regular_file_in_place() {
        // A read-only bind held by a leftover sandbox process covers the inode, so
        // the host must truncate and rewrite, never unlink and recreate.
        use std::os::unix::fs::MetadataExt;
        let dir = temp_dir("inplace");
        let path = dir.join("start.sh");
        std::fs::write(&path, "old contents").unwrap();
        let inode = std::fs::metadata(&path).unwrap().ino();

        crate::agent::write_no_follow(&path, b"new").unwrap();

        assert_eq!(std::fs::metadata(&path).unwrap().ino(), inode);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    }

    #[cfg(unix)]
    #[test]
    fn agent_log_is_a_regular_file_before_the_host_appends_to_it() {
        let dir = temp_dir("log");
        let victim = dir.join("victim.txt");
        std::fs::write(&victim, "untouched").unwrap();
        let log = dir.join("logs/agent.log");
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&victim, &log).unwrap();

        prepare_agent_log(&log).unwrap();

        let meta = std::fs::symlink_metadata(&log).unwrap();
        assert!(meta.file_type().is_file());
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "untouched");
        // An existing regular log is kept, not truncated.
        std::fs::write(&log, "earlier lines\n").unwrap();
        prepare_agent_log(&log).unwrap();
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "earlier lines\n");
    }

    #[test]
    fn shell_quotes_command_specs() {
        let command = CommandSpec {
            program: PathBuf::from("/bin/echo"),
            args: vec!["hello world".to_string(), "it's ok".to_string()],
            env: vec![("A".to_string(), "b c".to_string())],
            cwd: PathBuf::from("/tmp/a b"),
        };

        assert_eq!(
            command_spec_to_shell(&command, true),
            "cd '/tmp/a b' && exec env A='b c' '/bin/echo' 'hello world' 'it'\\''s ok'"
        );
    }

    #[test]
    fn command_files_do_not_persist_sensitive_env_values() {
        let command = CommandSpec {
            program: PathBuf::from("/bin/echo"),
            args: vec!["hi".to_string()],
            env: vec![
                (
                    "CLAUDE_CODE_OAUTH_TOKEN".to_string(),
                    "super-secret".to_string(),
                ),
                (
                    "WORLD_BASE_URL".to_string(),
                    "http://localhost:1".to_string(),
                ),
            ],
            cwd: PathBuf::from("/tmp"),
        };
        let shell = command_spec_to_shell(&command, true);

        assert!(!shell.contains("super-secret"));
        assert!(shell.contains(
            "CLAUDE_CODE_OAUTH_TOKEN=\"${CLAUDE_CODE_OAUTH_TOKEN:?set CLAUDE_CODE_OAUTH_TOKEN}\""
        ));
        assert!(shell.contains("WORLD_BASE_URL='http://localhost:1'"));
    }

    #[test]
    fn sanitizes_empty_and_unsafe_agent_names() {
        assert_eq!(sanitize_agent_name("mesa agent/1"), "mesa_agent_1");
        assert_eq!(sanitize_agent_name(""), "agent");
    }

    #[test]
    fn writes_rerunnable_command_files() {
        let root = test_root();
        let command_file = root.join("runtime/start.sh");

        write_command_file(&command_file, "cd '/tmp' && exec '/bin/echo' 'hi'").unwrap();

        let contents = std::fs::read_to_string(&command_file).unwrap();
        assert!(contents.starts_with("#!/usr/bin/env bash\nset -euo pipefail\n"));
        assert!(contents.contains("exec '/bin/echo' 'hi'"));
    }

    #[test]
    fn rejects_empty_agent_binary() {
        let root = test_root();
        std::fs::create_dir_all(&root).unwrap();
        let binary = root.join("empty-agent");
        std::fs::write(&binary, "").unwrap();

        let err = canonicalize_if_possible(&binary).unwrap_err();

        assert!(
            err.contains("agent binary is empty"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn agent_command_files_record_process_exit() {
        let root = test_root();
        let command_file = root.join("runtime/start.sh");
        let events_file = root.join("events.jsonl");
        let log_file = root.join("logs/agent.log");
        let event = ProcessEventContext {
            agent_name: "Eko".to_string(),
            agent_kind: "claude".to_string(),
            agent_dir: root.clone(),
            events_file: events_file.clone(),
            log_file,
            tmux_session: Some("test-session".to_string()),
            command_file: command_file.clone(),
        };

        write_agent_command_file(&command_file, "cd '/' && /bin/sh -c 'exit 7'", &event).unwrap();
        let status = std::process::Command::new("bash")
            .arg(&command_file)
            .status()
            .unwrap();

        assert_eq!(status.code(), Some(7));
        let lines: Vec<serde_json::Value> = std::fs::read_to_string(events_file)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(lines[0]["operation"], "agent.process");
        assert_eq!(lines[0]["status"], "running");
        assert!(lines[0]["exit_code"].is_null());
        assert_eq!(lines[1]["operation"], "agent.process");
        assert_eq!(lines[1]["status"], "exited");
        assert_eq!(lines[1]["exit_code"], 7);
        assert_eq!(lines[1]["error"], "agent command exited with status 7");
    }

    #[test]
    fn appends_operation_reports_as_jsonl() {
        let root = test_root();
        let events_file = root.join("events.jsonl");
        let report = OperationReport {
            time: "2026-05-17T00:00:00Z".to_string(),
            operation: "agent.start".to_string(),
            status: "started".to_string(),
            agent_name: "agent-0".to_string(),
            agent_kind: "claude".to_string(),
            agent_dir: root.clone(),
            tmux_session: Some("session".to_string()),
            attach_command: Some("tmux attach -t session".to_string()),
            command_file: Some(root.join("runtime/start.sh")),
            log_file: Some(root.join("logs/agent.log")),
            events_file: events_file.clone(),
            error: None,
        };

        append_operation_report(&events_file, report).unwrap();

        let contents = std::fs::read_to_string(&events_file).unwrap();
        let value: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
        assert_eq!(value["operation"], "agent.start");
        assert_eq!(value["status"], "started");
        assert_eq!(value["tmux_session"], "session");
    }

    #[test]
    fn writes_and_reads_active_agent_state() {
        let ctx = test_context(test_root(), "claude");
        ctx.prepare_paths().unwrap();
        let active = ActiveAgent {
            name: "agent-0".to_string(),
            kind: "claude".to_string(),
            operation: "agent.start".to_string(),
            tmux_session: "session".to_string(),
            tmux_window: None,
            command_file: ctx.paths.runtime_dir.join("start.sh"),
            started_at: "2026-05-17T00:00:00Z".to_string(),
        };

        write_active_agent(&ctx, active.clone()).unwrap();

        assert_eq!(read_active_agent(&ctx).unwrap(), Some(active));
    }

    #[test]
    fn clears_stale_active_agent_state() {
        let ctx = test_context(test_root(), "claude");
        ctx.prepare_paths().unwrap();
        write_active_agent(
            &ctx,
            ActiveAgent {
                name: "agent-0".to_string(),
                kind: "claude".to_string(),
                operation: "agent.start".to_string(),
                tmux_session: "definitely-not-running".to_string(),
                tmux_window: None,
                command_file: ctx.paths.runtime_dir.join("start.sh"),
                started_at: "2026-05-17T00:00:00Z".to_string(),
            },
        )
        .unwrap();

        ensure_no_active_agent(&ctx).unwrap();

        assert!(!active_agent_file(&ctx).exists());
    }

    fn test_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "clawblox-agent-runner-test-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap()
        ))
    }

    fn test_context(root: PathBuf, kind: &str) -> AgentContext {
        AgentContext {
            paths: AgentPaths {
                workspace_dir: root.join("workspace"),
                prompt_file: root.join("runtime/system_prompt.md"),
                session_id_file: root.join(format!("{kind}_session_id.txt")),
                transcript_dir: root.join("session"),
                runtime_dir: root.join("runtime"),
                log_file: root.join("logs/agent.log"),
                agent_home_dir: root.join("home"),
            },
            settings: AgentSettings {
                model: "test-model".to_string(),
                permission_mode: "bypassPermissions".to_string(),
                initial_prompt: "Begin".to_string(),
                lifecycle_prompt: "Save".to_string(),
                auto_continue_prompt: "Continue".to_string(),
                extra_args: Vec::new(),
                env: Vec::new(),
                sandbox_enabled: false,
                version: None,
            },
            sandbox: None,
        }
    }
}
