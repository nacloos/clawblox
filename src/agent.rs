use std::{
    error::Error,
    fmt,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::agent_sandbox::SandboxLayout;
use crate::replay_agent_log::{
    ClaudeTranscriptParser, CodexTranscriptParser, KimiTranscriptParser, TranscriptLineParser,
};

pub type AgentResult<T> = Result<T, AgentError>;

#[derive(Debug)]
pub enum AgentError {
    InvalidConfig(String),
    Io(std::io::Error),
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(f, "invalid agent config: {message}"),
            Self::Io(err) => write!(f, "{err}"),
        }
    }
}

impl Error for AgentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidConfig(_) => None,
            Self::Io(err) => Some(err),
        }
    }
}

impl From<std::io::Error> for AgentError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentPaths {
    pub workspace_dir: PathBuf,
    pub prompt_file: PathBuf,
    pub session_id_file: PathBuf,
    pub transcript_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub log_file: PathBuf,
    pub agent_home_dir: PathBuf,
}

impl AgentPaths {
    pub fn validate(&self) -> AgentResult<()> {
        require_nonempty_path("workspace_dir", &self.workspace_dir)?;
        require_nonempty_path("prompt_file", &self.prompt_file)?;
        require_nonempty_path("session_id_file", &self.session_id_file)?;
        require_nonempty_path("transcript_dir", &self.transcript_dir)?;
        require_nonempty_path("runtime_dir", &self.runtime_dir)?;
        require_nonempty_path("log_file", &self.log_file)?;
        require_nonempty_path("agent_home_dir", &self.agent_home_dir)?;
        Ok(())
    }

    pub fn create_dirs(&self) -> AgentResult<()> {
        self.validate()?;
        std::fs::create_dir_all(&self.workspace_dir)?;
        std::fs::create_dir_all(&self.transcript_dir)?;
        std::fs::create_dir_all(&self.runtime_dir)?;
        std::fs::create_dir_all(&self.agent_home_dir)?;
        create_parent_dir(&self.prompt_file)?;
        create_parent_dir(&self.session_id_file)?;
        create_parent_dir(&self.log_file)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentSettings {
    pub model: String,
    pub permission_mode: String,
    pub initial_prompt: String,
    pub lifecycle_prompt: String,
    pub auto_continue_prompt: String,
    pub extra_args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub sandbox_enabled: bool,
    /// Pinned CLI version. When set, the backend verifies the binary reports
    /// this exact version at setup (for reproducibility).
    pub version: Option<String>,
}

impl AgentSettings {
    pub fn validate(&self) -> AgentResult<()> {
        require_nonempty("model", &self.model)?;
        require_nonempty("permission_mode", &self.permission_mode)?;
        require_nonempty("initial_prompt", &self.initial_prompt)?;
        require_nonempty("lifecycle_prompt", &self.lifecycle_prompt)?;
        require_nonempty("auto_continue_prompt", &self.auto_continue_prompt)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentContext {
    pub paths: AgentPaths,
    pub settings: AgentSettings,
    pub sandbox: Option<SandboxLayout>,
}

impl AgentContext {
    pub fn validate(&self) -> AgentResult<()> {
        self.paths.validate()?;
        self.settings.validate()?;
        Ok(())
    }

    pub fn prepare_paths(&self) -> AgentResult<()> {
        self.paths.create_dirs()
    }

    /// Return the sandbox-equivalent of a host path when running sandboxed,
    /// or the path unchanged when not. Agents use this to inject correct paths
    /// into env vars without needing to know sandbox internals.
    pub fn guest_path(&self, host_path: &Path) -> PathBuf {
        match &self.sandbox {
            Some(layout) => layout.host_to_guest(host_path),
            None => host_path.to_path_buf(),
        }
    }

    /// Return the guest HOME directory when sandboxed, or the agent home dir otherwise.
    pub fn guest_home(&self) -> PathBuf {
        self.sandbox
            .as_ref()
            .map(|layout| layout.guest_home.clone())
            .unwrap_or_else(|| self.paths.agent_home_dir.clone())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
}

impl CommandSpec {
    pub fn new(program: impl Into<PathBuf>, cwd: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            cwd: cwd.into(),
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    pub fn envs(mut self, env: impl IntoIterator<Item = (String, String)>) -> Self {
        self.env.extend(env);
        self
    }

    pub fn with_extra_args(mut self, args: impl IntoIterator<Item = String>) -> Self {
        self.args.extend(args);
        self
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SandboxRequirements {
    pub read_only_paths: Vec<PathBuf>,
    pub writable_paths: Vec<PathBuf>,
    pub env_vars: Vec<String>,
    pub needs_network: bool,
}

pub trait Agent {
    fn name(&self) -> &'static str;

    fn default_model(&self) -> &'static str;

    fn binary(&self) -> &Path;

    fn needs_recovery_watchdog(&self) -> bool {
        false
    }

    /// Whether clawblox should run the driver-side auto-continue watcher for
    /// this agent (see `start_kimi_autoloop_watcher` in `agent_runner`).
    ///
    /// Only Kimi needs it. Claude and Codex keep looping entirely inside their
    /// Stop hook (it returns `decision:block` on every stop and the runtime
    /// re-blocks forever). Kimi's Stop hook is capped at one continuation per
    /// turn by its runtime, so the hook alone cannot loop — the watcher drives
    /// the loop instead. Default off.
    fn needs_autoloop_watcher(&self) -> bool {
        false
    }

    fn needs_socat_sandbox(&self) -> bool {
        false
    }

    fn initial_tmux_input(&self, _ctx: &AgentContext) -> Option<String> {
        None
    }

    fn send_tmux_input(&self, _ctx: &AgentContext, _prompt: &str) -> Option<String> {
        None
    }

    fn sandbox_guest_binary_name(&self) -> &str {
        self.name()
    }

    fn configure_sandbox_layout(
        &self,
        layout: SandboxLayout,
        _host_binary: &Path,
    ) -> AgentResult<SandboxLayout> {
        Ok(layout)
    }

    fn transcript_parser(&self) -> Box<dyn TranscriptLineParser>;

    fn check_setup(&self, ctx: &AgentContext) -> AgentResult<()>;

    fn start(&self, ctx: &AgentContext) -> AgentResult<CommandSpec>;

    fn continue_session(&self, ctx: &AgentContext, prompt: &str) -> AgentResult<CommandSpec>;

    fn configure_auto_continue(&self, ctx: &AgentContext) -> AgentResult<()>;

    fn collect_transcripts(&self, ctx: &AgentContext) -> AgentResult<()>;

    fn sandbox_requirements(&self, ctx: &AgentContext) -> AgentResult<SandboxRequirements>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaudeAgent {
    pub binary: PathBuf,
    pub bare: bool,
    pub disallowed_tools: Option<String>,
}

impl Default for ClaudeAgent {
    fn default() -> Self {
        Self {
            binary: PathBuf::from("claude"),
            bare: false,
            disallowed_tools: Some("WebFetch,WebSearch".to_string()),
        }
    }
}

impl Agent for ClaudeAgent {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn default_model(&self) -> &'static str {
        "claude-opus-4-6"
    }

    fn binary(&self) -> &Path {
        &self.binary
    }

    fn needs_recovery_watchdog(&self) -> bool {
        true
    }

    fn needs_socat_sandbox(&self) -> bool {
        true
    }

    fn sandbox_guest_binary_name(&self) -> &str {
        "claude"
    }

    fn configure_sandbox_layout(
        &self,
        layout: SandboxLayout,
        _host_binary: &Path,
    ) -> AgentResult<SandboxLayout> {
        Ok(layout.with_claude_plugin())
    }

    fn check_setup(&self, ctx: &AgentContext) -> AgentResult<()> {
        ctx.validate()?;
        require_program(&self.binary, "claude CLI")?;
        Ok(())
    }

    fn start(&self, ctx: &AgentContext) -> AgentResult<CommandSpec> {
        ctx.validate()?;
        let session_id = read_or_create_session_id(&ctx.paths.session_id_file)?;
        Ok(self
            .base_command(ctx)
            .arg("--session-id")
            .arg(session_id)
            .arg(&ctx.settings.initial_prompt))
    }

    fn continue_session(&self, ctx: &AgentContext, prompt: &str) -> AgentResult<CommandSpec> {
        ctx.validate()?;
        require_nonempty("prompt", prompt)?;
        Ok(self.base_command(ctx).arg("--continue").arg(prompt))
    }

    fn configure_auto_continue(&self, ctx: &AgentContext) -> AgentResult<()> {
        ctx.validate()?;
        let plugin_dir = claude_plugin_dir(ctx);
        let bin_dir = plugin_dir.join("bin");
        let hooks_dir = plugin_dir.join("hooks");
        let manifest_dir = plugin_dir.join(".claude-plugin");
        std::fs::create_dir_all(&bin_dir)?;
        std::fs::create_dir_all(&hooks_dir)?;
        std::fs::create_dir_all(&manifest_dir)?;

        write_file(
            &manifest_dir.join("plugin.json"),
            r#"{"name":"clawblox-claude","version":"0.0.1","description":"Claude hooks for clawblox agents"}
"#,
        )?;
        write_file(&hooks_dir.join("hooks.json"), CLAUDE_HOOKS_JSON)?;
        write_executable_file(&bin_dir.join("session_start.sh"), CLAUDE_SESSION_START_SH)?;
        write_executable_file(&bin_dir.join("session_end.sh"), CLAUDE_SESSION_END_SH)?;
        write_executable_file(&bin_dir.join("stop.sh"), CLAUDE_STOP_SH)?;
        write_executable_file(
            &bin_dir.join("post_tool_use_chat.py"),
            CLAUDE_POST_TOOL_USE_CHAT_PY,
        )?;
        write_executable_file(
            &bin_dir.join("user_prompt_submit.sh"),
            "#!/usr/bin/env bash\nexit 0\n",
        )?;
        write_executable_file(
            &bin_dir.join("stop_failure.sh"),
            "#!/usr/bin/env bash\nexit 0\n",
        )?;
        write_executable_file(
            &bin_dir.join("notification.sh"),
            "#!/usr/bin/env bash\nexit 0\n",
        )?;
        if let Some(layout) = &ctx.sandbox {
            prepare_claude_sandbox_home(ctx, layout)?;
            write_claude_sandbox_settings(ctx)?;
            write_sandbox_hosts_file(ctx)?;
        }
        Ok(())
    }

    fn transcript_parser(&self) -> Box<dyn TranscriptLineParser> {
        Box::new(ClaudeTranscriptParser)
    }

    fn collect_transcripts(&self, ctx: &AgentContext) -> AgentResult<()> {
        collect_claude_transcript(ctx)
    }

    fn sandbox_requirements(&self, ctx: &AgentContext) -> AgentResult<SandboxRequirements> {
        ctx.validate()?;
        Ok(SandboxRequirements {
            read_only_paths: vec![
                ctx.paths.prompt_file.clone(),
                claude_plugin_dir(ctx),
                self.binary.clone(),
            ],
            writable_paths: vec![
                ctx.paths.workspace_dir.clone(),
                ctx.paths.agent_home_dir.clone(),
                ctx.paths.runtime_dir.clone(),
                ctx.paths.transcript_dir.clone(),
                ctx.paths
                    .log_file
                    .parent()
                    .unwrap_or_else(|| Path::new(""))
                    .to_path_buf(),
            ],
            env_vars: vec![
                "HOME".to_string(),
                "PATH".to_string(),
                "CLAWBLOX_CLAUDE_AUTO_LOOP".to_string(),
                "CLAUDE_AUTO_LOOP_MESSAGE".to_string(),
                "CLAUDE_CODE_STOP_HOOK_BLOCK_CAP".to_string(),
                "CLAWBLOX_CLAUDE_RUNTIME_DIR".to_string(),
                "WORLD_INTERNAL_BASE_URL".to_string(),
            ],
            needs_network: ctx.settings.sandbox_enabled,
        })
    }
}

impl ClaudeAgent {
    fn base_command(&self, ctx: &AgentContext) -> CommandSpec {
        let sandbox = ctx.sandbox.as_ref();
        let program = sandbox
            .map(|layout| layout.guest_binary.clone())
            .unwrap_or_else(|| self.binary.clone());
        let plugin_dir = sandbox
            .and_then(|layout| layout.guest_plugin.clone())
            .unwrap_or_else(|| claude_plugin_dir(ctx));
        let log_dir = ctx.guest_path(ctx.paths.log_file.parent().unwrap_or(Path::new("")));
        let metadata_file = ctx.guest_path(&claude_hook_metadata_file(ctx));
        let runtime_dir = ctx.guest_path(&ctx.paths.runtime_dir);
        let cwd = ctx.guest_path(&ctx.paths.workspace_dir);
        let prompt_file = ctx.guest_path(&ctx.paths.prompt_file);
        // Claude hooks use agent_dir to locate workspace files — map to guest workspace.
        let agent_dir = cwd.clone();

        let mut command = CommandSpec::new(program, cwd)
            .arg("--plugin-dir")
            .arg(plugin_dir.display().to_string())
            .arg("--append-system-prompt-file")
            .arg(prompt_file.display().to_string())
            .arg("--permission-mode")
            .arg(&ctx.settings.permission_mode)
            .arg("--model")
            .arg(&ctx.settings.model)
            .envs(ctx.settings.env.clone())
            .env("CLAWBLOX_CLAUDE_AGENT_DIR", agent_dir.display().to_string())
            .env("CLAWBLOX_CLAUDE_LOG_DIR", log_dir.display().to_string())
            .env(
                "CLAWBLOX_CLAUDE_METADATA_FILE",
                metadata_file.display().to_string(),
            )
            .env(
                "CLAWBLOX_CLAUDE_RUNTIME_DIR",
                runtime_dir.display().to_string(),
            )
            .env("CLAWBLOX_CLAUDE_AUTO_LOOP", "1")
            // Claude Code caps repeated blocking Stop hooks to avoid accidental
            // infinite loops. Clawblox deliberately uses a blocking Stop hook as
            // its unattended auto-continue mechanism, so raise the cap high
            // enough for long generation runs.
            .env("CLAUDE_CODE_STOP_HOOK_BLOCK_CAP", "1000000")
            .env(
                "CLAUDE_AUTO_LOOP_MESSAGE",
                &ctx.settings.auto_continue_prompt,
            );

        if ctx.sandbox.is_some() {
            command = command
                .arg("--settings")
                .arg("/runtime/claude_sandbox_settings.json");
            if let Some(disallowed_tools) = &self.disallowed_tools {
                if !disallowed_tools.trim().is_empty() {
                    command = command.arg("--disallowedTools").arg(disallowed_tools);
                }
            }
        }

        if self.bare {
            command.args.insert(0, "--bare".to_string());
        }

        command.args.extend(ctx.settings.extra_args.clone());
        command
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexAgent {
    pub binary: PathBuf,
}

impl Default for CodexAgent {
    fn default() -> Self {
        Self {
            binary: PathBuf::from("codex"),
        }
    }
}

impl Agent for CodexAgent {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn default_model(&self) -> &'static str {
        "gpt-5.5"
    }

    fn binary(&self) -> &Path {
        &self.binary
    }

    fn sandbox_guest_binary_name(&self) -> &str {
        "codex"
    }

    fn configure_sandbox_layout(
        &self,
        layout: SandboxLayout,
        host_binary: &Path,
    ) -> AgentResult<SandboxLayout> {
        // Codex gives CODEX_HOME precedence over HOME. Controller sessions often
        // set CODEX_HOME to their own temp/config location, which does not exist
        // inside the agent bubblewrap. Drop it so sandboxed Codex consistently
        // resolves /home/agent/.codex, the home we generated for this agent.
        let mut layout = layout.with_unset_env("CODEX_HOME");
        if let Some((root, rel)) = codex_bind_root_for_bin(host_binary) {
            layout = layout.with_codex_binary_root(root, rel);
        }
        if let Some(root) = node_root_for_path(host_binary) {
            layout = layout.with_node_root(root);
        } else if let Some(root) = resolve_node_root() {
            layout = layout.with_node_root(root);
        }
        Ok(layout)
    }

    fn check_setup(&self, ctx: &AgentContext) -> AgentResult<()> {
        ctx.validate()?;
        require_program(&self.binary, "codex CLI")?;
        Ok(())
    }

    fn start(&self, ctx: &AgentContext) -> AgentResult<CommandSpec> {
        ctx.validate()?;
        Ok(self.base_command(ctx).arg(&ctx.settings.initial_prompt))
    }

    fn continue_session(&self, ctx: &AgentContext, prompt: &str) -> AgentResult<CommandSpec> {
        ctx.validate()?;
        require_nonempty("prompt", prompt)?;
        let session_id = read_required_session_id(&ctx.paths.session_id_file)?;
        Ok(self
            .base_resume_command(ctx)
            .arg(session_id)
            .arg(prompt.to_string()))
    }

    fn configure_auto_continue(&self, ctx: &AgentContext) -> AgentResult<()> {
        ctx.validate()?;
        if let Some(layout) = &ctx.sandbox {
            prepare_codex_sandbox_home(layout)?;
            write_sandbox_hosts_file(ctx)?;
        }
        prepare_codex_home(ctx)?;
        let stop_hook_command = if ctx.sandbox.is_some() {
            "python3 /home/agent/.codex/stop_hook.py".to_string()
        } else {
            format!("python3 {}", codex_stop_hook_path(ctx).display())
        };
        let hooks_json = serde_json::json!({
            "hooks": {
                "Stop": [
                    {
                        "hooks": [
                            {
                                "type": "command",
                                "command": stop_hook_command,
                            }
                        ]
                    }
                ]
            }
        });
        write_executable_file(&codex_stop_hook_path(ctx), CODEX_STOP_HOOK_PY)?;
        write_file(
            &codex_hooks_path(ctx),
            &format!(
                "{}\n",
                serde_json::to_string_pretty(&hooks_json)
                    .map_err(|err| AgentError::InvalidConfig(err.to_string()))?
            ),
        )?;
        // Codex treats hooks as code that can run outside its sandbox. The
        // bash launcher pre-trusts this generated Stop hook so unattended
        // generation runs do not stall on the interactive "Hooks need review"
        // prompt; keep the Python/Rust agent path aligned with that behavior.
        trust_codex_hooks(&self.binary, ctx)?;
        Ok(())
    }

    fn transcript_parser(&self) -> Box<dyn TranscriptLineParser> {
        Box::new(CodexTranscriptParser)
    }

    fn collect_transcripts(&self, ctx: &AgentContext) -> AgentResult<()> {
        collect_codex_transcript(ctx)
    }

    fn sandbox_requirements(&self, ctx: &AgentContext) -> AgentResult<SandboxRequirements> {
        ctx.validate()?;
        Ok(SandboxRequirements {
            read_only_paths: vec![ctx.paths.prompt_file.clone(), self.binary.clone()],
            writable_paths: vec![
                ctx.paths.workspace_dir.clone(),
                ctx.paths.agent_home_dir.clone(),
                ctx.paths.runtime_dir.clone(),
                ctx.paths.transcript_dir.clone(),
                ctx.paths
                    .log_file
                    .parent()
                    .unwrap_or_else(|| Path::new(""))
                    .to_path_buf(),
            ],
            env_vars: vec![
                "HOME".to_string(),
                "PATH".to_string(),
                "CLAWBLOX_CODEX_AUTO_LOOP".to_string(),
                "CODEX_AUTO_LOOP_MESSAGE".to_string(),
            ],
            needs_network: ctx.settings.sandbox_enabled,
        })
    }
}

impl CodexAgent {
    fn base_command(&self, ctx: &AgentContext) -> CommandSpec {
        let sandbox = ctx.sandbox.as_ref();
        let sandbox_mode = if ctx.sandbox.is_some() {
            "workspace-write"
        } else {
            "danger-full-access"
        };
        let program = sandbox
            .and_then(|layout| layout.codex_guest_node())
            .or_else(|| sandbox.map(|layout| layout.codex_guest_program()))
            .unwrap_or_else(|| self.binary.clone());
        let codex_program_arg = sandbox.and_then(|layout| {
            layout
                .codex_guest_node()
                .map(|_| layout.codex_guest_program().display().to_string())
        });
        let workspace = ctx.guest_path(&ctx.paths.workspace_dir);
        let home = ctx.guest_home();
        let runtime_dir = ctx.guest_path(&ctx.paths.runtime_dir);
        let log_dir = ctx.guest_path(ctx.paths.log_file.parent().unwrap_or(Path::new("")));
        let mut command = CommandSpec::new(program, workspace.clone());
        if let Some(codex_program_arg) = codex_program_arg {
            command = command.arg(codex_program_arg);
        }
        command = command
            .arg("--no-alt-screen")
            .arg("-C")
            .arg(workspace.display().to_string())
            .arg("-m")
            .arg(&ctx.settings.model);
        if ctx.sandbox.is_some() {
            // Clawblox already runs sandboxed Codex inside an outer bubblewrap
            // that mounts only the agent workspace/home/runtime/logs plus the
            // Codex executable. Codex 0.133.0's interactive TUI does not route
            // local/private requests through its managed network_proxy reliably
            // under `-s workspace-write`, so the world capability proxy is
            // blocked before it can be reached. Use Codex's explicit
            // externally-sandboxed mode here: the security boundary remains the
            // Clawblox bubblewrap, while Codex shell commands can reach
            // http://world:<capability-proxy-port> as the bash launcher did on
            // 0.130.0.
            command = command.arg("--dangerously-bypass-approvals-and-sandbox");
        } else {
            command = command
                .arg("-s")
                .arg(sandbox_mode)
                .arg("-a")
                .arg(codex_approval_policy(&ctx.settings.permission_mode));
        }
        command
            .envs(ctx.settings.env.clone())
            .env("HOME", home.display().to_string())
            .env(
                "CLAWBLOX_CODEX_RUNTIME_DIR",
                runtime_dir.display().to_string(),
            )
            .env("CLAWBLOX_CODEX_LOG_DIR", log_dir.display().to_string())
            .env("CLAWBLOX_CODEX_AUTO_LOOP", "1")
            .env(
                "CODEX_AUTO_LOOP_MESSAGE",
                &ctx.settings.auto_continue_prompt,
            )
            .with_extra_args(ctx.settings.extra_args.clone())
    }

    fn base_resume_command(&self, ctx: &AgentContext) -> CommandSpec {
        let mut command = self.base_command(ctx);
        let insert_at = if ctx
            .sandbox
            .as_ref()
            .and_then(|layout| layout.codex_guest_node())
            .is_some()
        {
            1
        } else {
            0
        };
        command.args.insert(insert_at, "resume".to_string());
        command
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KimiAgent {
    pub binary: PathBuf,
}

impl Default for KimiAgent {
    fn default() -> Self {
        Self {
            binary: PathBuf::from("kimi"),
        }
    }
}

impl Agent for KimiAgent {
    fn name(&self) -> &'static str {
        "kimi"
    }

    fn default_model(&self) -> &'static str {
        "kimi-code/kimi-for-coding"
    }

    fn binary(&self) -> &Path {
        &self.binary
    }

    fn needs_autoloop_watcher(&self) -> bool {
        true
    }

    fn check_setup(&self, ctx: &AgentContext) -> AgentResult<()> {
        ctx.validate()?;
        require_program(&self.binary, "kimi CLI")?;
        if let Some(pin) = ctx.settings.version.as_deref() {
            verify_kimi_version(&self.binary, pin)?;
        }
        Ok(())
    }

    fn initial_tmux_input(&self, ctx: &AgentContext) -> Option<String> {
        ctx.validate().ok()?;
        let system_prompt = std::fs::read_to_string(&ctx.paths.prompt_file).unwrap_or_default();
        Some(if system_prompt.trim().is_empty() {
            ctx.settings.initial_prompt.clone()
        } else {
            format!(
                "{}\n\n{}",
                system_prompt.trim(),
                ctx.settings.initial_prompt
            )
        })
    }

    fn send_tmux_input(&self, _ctx: &AgentContext, prompt: &str) -> Option<String> {
        Some(prompt.to_string())
    }

    fn start(&self, ctx: &AgentContext) -> AgentResult<CommandSpec> {
        ctx.validate()?;
        Ok(self.base_command(ctx))
    }

    fn continue_session(&self, ctx: &AgentContext, prompt: &str) -> AgentResult<CommandSpec> {
        ctx.validate()?;
        require_nonempty("prompt", prompt)?;
        let session_id = read_required_session_id(&ctx.paths.session_id_file)?;
        Ok(self.base_command(ctx).arg("-S").arg(session_id))
    }

    fn configure_auto_continue(&self, ctx: &AgentContext) -> AgentResult<()> {
        ctx.validate()?;
        let home = kimi_home_dir(ctx);
        std::fs::create_dir_all(home.join("sessions"))?;
        std::fs::create_dir_all(home.join("logs"))?;
        write_kimi_config(ctx)?;
        write_executable_file(&kimi_stop_hook_path(ctx), KIMI_STOP_HOOK_PY)?;
        // Kimi Code checks for a newer release at startup and, in a TTY, drops
        // into an interactive "install update" picker. Under clawblox the CLI runs
        // headlessly in a tmux pane, so that picker breaks the run and would also
        // change the CLI version mid-experiment. Seed the update cache with no
        // newer version so the preflight finds no update target.
        write_kimi_update_freeze(&home)?;
        if ctx.settings.sandbox_enabled {
            write_sandbox_hosts_file(ctx)?;
        }
        Ok(())
    }

    fn transcript_parser(&self) -> Box<dyn TranscriptLineParser> {
        Box::new(KimiTranscriptParser)
    }

    fn collect_transcripts(&self, ctx: &AgentContext) -> AgentResult<()> {
        collect_kimi_transcript(ctx)
    }

    fn sandbox_requirements(&self, ctx: &AgentContext) -> AgentResult<SandboxRequirements> {
        ctx.validate()?;
        Ok(SandboxRequirements {
            read_only_paths: vec![ctx.paths.prompt_file.clone(), self.binary.clone()],
            writable_paths: vec![
                ctx.paths.workspace_dir.clone(),
                ctx.paths.agent_home_dir.clone(),
                ctx.paths.runtime_dir.clone(),
                ctx.paths.transcript_dir.clone(),
                ctx.paths
                    .log_file
                    .parent()
                    .unwrap_or_else(|| Path::new(""))
                    .to_path_buf(),
            ],
            env_vars: vec![
                "HOME".to_string(),
                "PATH".to_string(),
                "KIMI_CODE_HOME".to_string(),
                "CLAWBLOX_KIMI_AUTO_LOOP".to_string(),
                "KIMI_AUTO_LOOP_MESSAGE".to_string(),
            ],
            needs_network: ctx.settings.sandbox_enabled,
        })
    }
}

impl KimiAgent {
    fn base_command(&self, ctx: &AgentContext) -> CommandSpec {
        let program = ctx
            .sandbox
            .as_ref()
            .map(|layout| layout.guest_binary.clone())
            .unwrap_or_else(|| self.binary.clone());
        let cwd = ctx.guest_path(&ctx.paths.workspace_dir);
        let log_dir = ctx.guest_path(ctx.paths.log_file.parent().unwrap_or(Path::new("")));
        let mut cmd = CommandSpec::new(program, cwd)
            .arg("-m")
            .arg(&ctx.settings.model)
            .envs(ctx.settings.env.clone())
            .env("CLAWBLOX_KIMI_AUTO_LOOP", "1")
            .env("CLAWBLOX_KIMI_LOG_DIR", log_dir.display().to_string())
            .env("KIMI_AUTO_LOOP_MESSAGE", &ctx.settings.auto_continue_prompt)
            .with_extra_args(ctx.settings.extra_args.clone());
        // In sandbox mode HOME is remapped to the guest home dir, so kimi finds
        // ~/.kimi-code there automatically. Outside the sandbox we set KIMI_CODE_HOME
        // explicitly to keep each agent isolated from the user's own ~/.kimi-code.
        if !ctx.settings.sandbox_enabled {
            cmd = cmd.env("KIMI_CODE_HOME", kimi_home_dir(ctx).display().to_string());
        }
        cmd
    }
}

fn verify_kimi_version(binary: &Path, pin: &str) -> AgentResult<()> {
    let output = std::process::Command::new(binary)
        .arg("--version")
        .output()
        .map_err(AgentError::Io)?;
    if !output.status.success() {
        return Err(AgentError::InvalidConfig(format!(
            "could not determine kimi CLI version from `{} --version`",
            binary.display()
        )));
    }
    let actual = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if actual != pin {
        return Err(AgentError::InvalidConfig(format!(
            "kimi CLI version {actual} does not match the pinned version {pin}; \
             install the pinned version or change the requested version"
        )));
    }
    Ok(())
}

fn write_kimi_update_freeze(home: &Path) -> AgentResult<()> {
    // `<KIMI_CODE_HOME>/updates/latest.json`; `latest: null` makes
    // selectUpdateTarget() return no target, so the update preflight is a no-op.
    let updates = home.join("updates");
    std::fs::create_dir_all(&updates)?;
    std::fs::write(
        updates.join("latest.json"),
        "{\"source\":\"cdn\",\"checkedAt\":null,\"latest\":null}",
    )?;
    Ok(())
}

fn create_parent_dir(path: &Path) -> AgentResult<()> {
    let Some(parent) = path.parent() else {
        return Err(AgentError::InvalidConfig(format!(
            "{} has no parent directory",
            path.display()
        )));
    };
    std::fs::create_dir_all(parent)?;
    Ok(())
}

fn write_file(path: &Path, contents: &str) -> AgentResult<()> {
    create_parent_dir(path)?;
    write_no_follow(path, contents.as_bytes())?;
    Ok(())
}

/// host-side writes into agent-visible directories never follow
/// a symlink (a planted one is removed and replaced by a regular file), and rewrite a
/// regular file in place, so a read-only bind over its inode stays in force.
#[cfg(unix)]
pub(crate) fn write_no_follow(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    write_no_follow_with_mode(path, contents, None)
}

/// `write_no_follow`, then set `mode` through the open file (fchmod), so the change can't
/// follow a symlink planted after the write.
#[cfg(unix)]
pub(crate) fn write_no_follow_with_mode(
    path: &Path,
    contents: &[u8],
    mode: Option<std::fs::Permissions>,
) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            std::fs::remove_file(path)?;
        }
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    file.write_all(contents)?;
    if let Some(mode) = mode {
        file.set_permissions(mode)?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn write_no_follow(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, contents)
}

#[cfg(not(unix))]
pub(crate) fn write_no_follow_with_mode(
    path: &Path,
    contents: &[u8],
    mode: Option<std::fs::Permissions>,
) -> std::io::Result<()> {
    std::fs::write(path, contents)?;
    if let Some(mode) = mode {
        std::fs::set_permissions(path, mode)?;
    }
    Ok(())
}

/// A regular file, not a symlink to one.
fn is_regular_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_file())
        .unwrap_or(false)
}

fn write_executable_file(path: &Path, contents: &str) -> AgentResult<()> {
    write_file(path, contents)?;
    make_executable(path)?;
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> AgentResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> AgentResult<()> {
    Ok(())
}

fn require_program(path: &Path, label: &str) -> AgentResult<()> {
    if path.components().count() > 1 || path.is_absolute() {
        if path.exists() {
            return Ok(());
        }
        return Err(AgentError::InvalidConfig(format!(
            "{label} was not found at {}",
            path.display()
        )));
    }

    let Some(paths) = std::env::var_os("PATH") else {
        return Err(AgentError::InvalidConfig(format!(
            "{label} was not found in PATH"
        )));
    };
    for dir in std::env::split_paths(&paths) {
        if dir.join(path).exists() {
            return Ok(());
        }
    }
    Err(AgentError::InvalidConfig(format!(
        "{label} was not found in PATH"
    )))
}

fn read_or_create_session_id(path: &Path) -> AgentResult<String> {
    if let Ok(existing) = std::fs::read_to_string(path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    create_parent_dir(path)?;
    let id = uuid::Uuid::new_v4().to_string();
    std::fs::write(path, format!("{id}\n"))?;
    Ok(id)
}

fn read_required_session_id(path: &Path) -> AgentResult<String> {
    let id = std::fs::read_to_string(path)
        .map_err(AgentError::Io)?
        .trim()
        .to_string();
    require_nonempty("session_id", &id)?;
    Ok(id)
}

fn claude_plugin_dir(ctx: &AgentContext) -> PathBuf {
    ctx.paths.runtime_dir.join("claude-plugin")
}

fn claude_hook_metadata_file(ctx: &AgentContext) -> PathBuf {
    ctx.paths.runtime_dir.join("hook_session.env")
}

fn codex_home_dir(ctx: &AgentContext) -> PathBuf {
    agent_home_dir(ctx).join(".codex")
}

fn agent_home_dir(ctx: &AgentContext) -> PathBuf {
    if ctx.settings.sandbox_enabled {
        sandbox_home_dir(ctx)
    } else {
        ctx.paths.agent_home_dir.clone()
    }
}

fn sandbox_home_dir(ctx: &AgentContext) -> PathBuf {
    agent_dir_from_context(ctx).join("sandbox-home")
}

fn agent_dir_from_context(ctx: &AgentContext) -> PathBuf {
    ctx.paths
        .runtime_dir
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_path_buf()
}

fn codex_history_file(ctx: &AgentContext) -> PathBuf {
    codex_home_dir(ctx).join("history.jsonl")
}

fn codex_stop_hook_path(ctx: &AgentContext) -> PathBuf {
    codex_home_dir(ctx).join("stop_hook.py")
}

fn codex_hooks_path(ctx: &AgentContext) -> PathBuf {
    codex_home_dir(ctx).join("hooks.json")
}

fn kimi_home_dir(ctx: &AgentContext) -> PathBuf {
    agent_home_dir(ctx).join(".kimi-code")
}

fn kimi_stop_hook_path(ctx: &AgentContext) -> PathBuf {
    kimi_home_dir(ctx).join("stop_hook.py")
}

fn user_kimi_home() -> Option<PathBuf> {
    if let Ok(val) = std::env::var("KIMI_CODE_HOME") {
        let path = PathBuf::from(val);
        if path.is_dir() {
            return Some(path);
        }
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".kimi-code"))
}

fn sync_kimi_user_credentials(ctx: &AgentContext) -> AgentResult<()> {
    let Some(user_home) = user_kimi_home() else {
        return Ok(());
    };
    let target = kimi_home_dir(ctx);
    sync_codex_user_entry(
        &user_home.join("credentials"),
        &target.join("credentials"),
        ctx.settings.sandbox_enabled,
    )
}

fn write_kimi_config(ctx: &AgentContext) -> AgentResult<()> {
    // Inside the bwrap sandbox the home dir is remapped to /home/agent, so we
    // must use the guest path for the stop hook command in config.toml.
    let stop_hook = if ctx.settings.sandbox_enabled {
        PathBuf::from("/home/agent/.kimi-code/stop_hook.py")
    } else {
        kimi_stop_hook_path(ctx)
    };
    let api_key = env_value(ctx, "KIMI_API_KEY").unwrap_or_default();
    let config = format!(
        "default_model = \"{model}\"\n\
         default_permission_mode = \"yolo\"\n\
         \n\
         [providers.\"managed:kimi-code\"]\n\
         type = \"kimi\"\n\
         api_key = \"{api_key}\"\n\
         base_url = \"https://api.kimi.com/coding/v1\"\n\
         \n\
         [models.\"kimi-code/kimi-for-coding\"]\n\
         provider = \"managed:kimi-code\"\n\
         model = \"kimi-for-coding\"\n\
         max_context_size = 262144\n\
         capabilities = [\"thinking\", \"image_in\", \"video_in\", \"tool_use\"]\n\
         display_name = \"Kimi-k2.6\"\n\
         \n\
         [models.\"kimi-code/k3\"]\n\
         provider = \"managed:kimi-code\"\n\
         model = \"k3\"\n\
         max_context_size = 1048576\n\
         capabilities = [\"thinking\", \"always_thinking\", \"image_in\", \"video_in\", \"tool_use\"]\n\
         display_name = \"K3\"\n\
         support_efforts = [\"max\"]\n\
         default_effort = \"max\"\n\
         \n\
         [[hooks]]\n\
         event = \"Stop\"\n\
         command = \"python3 {stop_hook}\"\n\
         timeout = 30\n",
        model = ctx.settings.model,
        stop_hook = stop_hook.display(),
    );
    write_file(&kimi_home_dir(ctx).join("config.toml"), &config)
}

fn collect_kimi_transcript(ctx: &AgentContext) -> AgentResult<()> {
    let session_index = kimi_home_dir(ctx).join("session_index.jsonl");
    if !session_index.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(&session_index)?;
    let Some(last_line) = content.lines().filter(|l| !l.trim().is_empty()).last() else {
        return Ok(());
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(last_line) else {
        return Ok(());
    };
    let session_id = value
        .get("sessionId")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let session_dir = value
        .get("sessionDir")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if session_id.is_empty() {
        return Ok(());
    }
    create_parent_dir(&ctx.paths.session_id_file)?;
    std::fs::write(&ctx.paths.session_id_file, format!("{session_id}\n"))?;
    if session_dir.is_empty() {
        return Ok(());
    }
    // sessionDir is a guest path (/home/agent/...) when running in the sandbox.
    // Translate it to the corresponding host path (sandbox-home/...).
    const GUEST_HOME: &str = "/home/agent";
    let session_dir_host = if ctx.settings.sandbox_enabled {
        if let Ok(rel) = Path::new(&session_dir).strip_prefix(GUEST_HOME) {
            sandbox_home_dir(ctx).join(rel)
        } else {
            PathBuf::from(&session_dir)
        }
    } else {
        PathBuf::from(&session_dir)
    };
    let wire = session_dir_host.join("agents/main/wire.jsonl");
    if wire.is_file() {
        std::fs::create_dir_all(&ctx.paths.transcript_dir)?;
        std::fs::copy(
            wire,
            ctx.paths.transcript_dir.join(format!("{session_id}.jsonl")),
        )?;
    }
    Ok(())
}

fn prepare_codex_home(ctx: &AgentContext) -> AgentResult<()> {
    let home = codex_home_dir(ctx);
    std::fs::create_dir_all(home.join("log"))?;
    std::fs::create_dir_all(home.join("sessions"))?;
    std::fs::create_dir_all(home.join("tmp"))?;
    sync_codex_user_state(ctx)?;
    write_codex_config(ctx)
}

fn sync_codex_user_state(ctx: &AgentContext) -> AgentResult<()> {
    let Some(user_home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Ok(());
    };
    let source = user_home.join(".codex");
    if !source.is_dir() {
        return Ok(());
    }
    let target = codex_home_dir(ctx);
    sync_codex_user_entry(
        &source.join("auth.json"),
        &target.join("auth.json"),
        ctx.settings.sandbox_enabled,
    )?;
    sync_codex_user_entry(
        &source.join("skills"),
        &target.join("skills"),
        ctx.settings.sandbox_enabled,
    )?;
    sync_codex_user_entry(
        &source.join("rules"),
        &target.join("rules"),
        ctx.settings.sandbox_enabled,
    )?;
    Ok(())
}

fn sync_codex_user_entry(source: &Path, target: &Path, copy: bool) -> AgentResult<()> {
    if !source.exists() || target.exists() {
        return Ok(());
    }
    create_parent_dir(target)?;
    if copy {
        copy_path(source, target)?;
    } else {
        symlink_path(source, target)?;
    }
    Ok(())
}

fn copy_path(source: &Path, target: &Path) -> AgentResult<()> {
    if source.is_dir() {
        std::fs::create_dir_all(target)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            copy_path(&entry.path(), &target.join(entry.file_name()))?;
        }
    } else {
        create_parent_dir(target)?;
        // keep the source's mode, as std::fs::copy did, so copied scripts stay executable
        let mode = std::fs::metadata(source)?.permissions();
        write_no_follow_with_mode(target, &std::fs::read(source)?, Some(mode))?;
    }
    Ok(())
}

fn prepare_claude_sandbox_home(ctx: &AgentContext, layout: &SandboxLayout) -> AgentResult<()> {
    let claude_dir = layout.host_home_dir.join(".claude");
    for rel in [
        ".local/bin",
        ".claude/projects",
        ".claude/session-env",
        ".claude/sessions",
        ".claude/plugins",
        ".claude/cache",
        ".claude/backups",
        ".claude/downloads",
        ".claude/file-history",
        ".claude/paste-cache",
        ".claude/plans",
        ".claude/shell-snapshots",
    ] {
        std::fs::create_dir_all(layout.host_home_dir.join(rel))?;
    }
    symlink_or_copy_file(
        &layout.guest_binary,
        &layout.host_home_dir.join(".local/bin/claude"),
    )?;

    let host_claude_dir = effective_env_value(ctx, "CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".claude")));
    if let Some(token) = effective_env_value(ctx, "ANTHROPIC_OAUTH_TOKEN") {
        write_file(
            &claude_dir.join(".credentials.json"),
            &format!(
                "{{\"claudeAiOauth\":{{\"accessToken\":{}}}}}\n",
                serde_json::to_string(&token)
                    .map_err(|err| AgentError::InvalidConfig(err.to_string()))?
            ),
        )?;
    } else if effective_env_value(ctx, "CLAUDE_CODE_OAUTH_TOKEN").is_some() {
        let _ = std::fs::remove_file(claude_dir.join(".credentials.json"));
    } else if let Some(host_dir) = &host_claude_dir {
        copy_if_exists(
            &host_dir.join(".credentials.json"),
            &claude_dir.join(".credentials.json"),
        )?;
    }
    if let Some(host_dir) = &host_claude_dir {
        copy_if_exists(
            &host_dir.join("settings.json"),
            &claude_dir.join("settings.json"),
        )?;
        copy_if_exists(
            &host_dir.join(".claude.json"),
            &layout.host_home_dir.join(".claude.json"),
        )?;
    }
    write_claude_sandbox_autonomous_settings(&claude_dir.join("settings.json"))?;
    write_claude_workspace_trust(ctx, layout)?;
    Ok(())
}

fn write_claude_sandbox_autonomous_settings(path: &Path) -> AgentResult<()> {
    let mut data = if is_regular_file(&path) {
        serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(path)?)
            .unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    if !data.is_object() {
        data = serde_json::json!({});
    }
    let obj = data.as_object_mut().expect("object");
    obj.entry("skipDangerousModePermissionPrompt".to_string())
        .or_insert(serde_json::json!(true));
    obj.entry("skipAutoPermissionPrompt".to_string())
        .or_insert(serde_json::json!(true));
    let permissions = obj
        .entry("permissions".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !permissions.is_object() {
        *permissions = serde_json::json!({});
    }
    permissions
        .as_object_mut()
        .expect("object")
        .entry("defaultMode".to_string())
        .or_insert(serde_json::json!("auto"));
    write_json_file(path, &data)
}

fn write_claude_workspace_trust(_ctx: &AgentContext, layout: &SandboxLayout) -> AgentResult<()> {
    let path = layout.host_home_dir.join(".claude.json");
    let mut data = if is_regular_file(&path) {
        serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&path)?)
            .unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    let Some(obj) = data.as_object_mut() else {
        data = serde_json::json!({});
        let obj = data.as_object_mut().expect("object");
        obj.insert("projects".to_string(), serde_json::json!({}));
        return write_json_file(&path, &data);
    };
    let projects = obj
        .entry("projects".to_string())
        .or_insert_with(|| serde_json::json!({}));
    let project_obj = projects.as_object_mut().ok_or_else(|| {
        AgentError::InvalidConfig("Claude projects config is not an object".into())
    })?;
    project_obj.insert(
        "/workspace".to_string(),
        serde_json::json!({
            "hasTrustDialogAccepted": true,
            "allowedTools": [],
            "mcpContextUris": [],
            "mcpServers": {},
            "enabledMcpjsonServers": [],
            "disabledMcpjsonServers": [],
        }),
    );
    obj.insert(
        "hasCompletedOnboarding".to_string(),
        serde_json::json!(true),
    );
    obj.entry("lastOnboardingVersion".to_string())
        .or_insert_with(|| serde_json::json!("2.1.116"));
    write_json_file(&path, &data)
}

fn write_claude_sandbox_settings(ctx: &AgentContext) -> AgentResult<()> {
    let native_sandbox_enabled = env_value(ctx, "CLAWBLOX_CLAUDE_NATIVE_SANDBOX")
        .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
        .unwrap_or(true);
    if !native_sandbox_enabled {
        let settings = serde_json::json!({
            "sandbox": {
                "enabled": false,
            }
        });
        return write_json_file(
            &ctx.paths.runtime_dir.join("claude_sandbox_settings.json"),
            &settings,
        );
    }

    let world_base_url = env_value(ctx, "WORLD_BASE_URL").unwrap_or_default();
    let host = parse_http_host(&world_base_url).unwrap_or_default();
    let mut allowed_domains = Vec::new();
    if !host.is_empty() && !matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1") {
        allowed_domains.push(host);
    }
    let allow_loopback = env_value(ctx, "WORLD_ALLOW_LOOPBACK")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let denied: serde_json::Value = if allow_loopback {
        serde_json::json!([])
    } else {
        serde_json::json!(["localhost", "127.0.0.1"])
    };
    let mut network = serde_json::json!({
        "allowedDomains": allowed_domains,
        "deniedDomains": denied,
    });
    if let Some(port) = world_proxy_port(ctx) {
        network["httpProxyPort"] = serde_json::json!(port);
    }
    let settings = serde_json::json!({
        "sandbox": {
            "enabled": true,
            "failIfUnavailable": true,
            "allowUnsandboxedCommands": false,
            "autoAllowBashIfSandboxed": true,
            "network": network,
        }
    });
    write_json_file(
        &ctx.paths.runtime_dir.join("claude_sandbox_settings.json"),
        &settings,
    )
}

fn prepare_codex_sandbox_home(layout: &SandboxLayout) -> AgentResult<()> {
    for rel in [
        ".local/bin",
        ".codex/log",
        ".codex/sessions",
        ".codex/tmp",
        ".codex/cache",
        ".codex/memories",
    ] {
        std::fs::create_dir_all(layout.host_home_dir.join(rel))?;
    }
    symlink_or_copy_file(
        &layout.guest_binary,
        &layout.host_home_dir.join(".local/bin/codex"),
    )
}

fn write_sandbox_hosts_file(ctx: &AgentContext) -> AgentResult<()> {
    let path = ctx.paths.runtime_dir.join("sandbox_hosts");
    let mut hosts = "127.0.0.1 localhost\n::1 localhost ip6-localhost ip6-loopback\n".to_string();
    if let Some(host) = env_value(ctx, "WORLD_BASE_URL").and_then(|url| parse_http_host(&url)) {
        if !matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1") && !hosts.contains(&host) {
            hosts = format!("{}\n127.0.0.1 {host}\n", hosts.trim_end());
        }
    }
    write_file(&path, &hosts)
}

fn copy_if_exists(source: &Path, target: &Path) -> AgentResult<()> {
    if source.exists() {
        copy_path(source, target)?;
    }
    Ok(())
}

#[cfg(unix)]
fn symlink_or_copy_file(source: &Path, target: &Path) -> AgentResult<()> {
    create_parent_dir(target)?;
    let _ = std::fs::remove_file(target);
    std::os::unix::fs::symlink(source, target)?;
    Ok(())
}

#[cfg(not(unix))]
fn symlink_or_copy_file(source: &Path, target: &Path) -> AgentResult<()> {
    create_parent_dir(target)?;
    std::fs::copy(source, target)?;
    Ok(())
}

fn write_json_file(path: &Path, value: &serde_json::Value) -> AgentResult<()> {
    write_file(
        path,
        &format!(
            "{}\n",
            serde_json::to_string_pretty(value)
                .map_err(|err| AgentError::InvalidConfig(err.to_string()))?
        ),
    )
}

fn env_value(ctx: &AgentContext, key: &str) -> Option<String> {
    ctx.settings
        .env
        .iter()
        .rev()
        .find_map(|(name, value)| (name == key).then(|| value.clone()))
}

fn effective_env_value(ctx: &AgentContext, key: &str) -> Option<String> {
    env_value(ctx, key).or_else(|| std::env::var(key).ok())
}

fn world_proxy_port(ctx: &AgentContext) -> Option<u16> {
    env_value(ctx, "CLAWBLOX_WORLD_HTTP_PROXY_PORT").and_then(|value| value.parse().ok())
}

fn parse_http_host(url: &str) -> Option<String> {
    let without_scheme = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))?;
    let authority = without_scheme.split('/').next().unwrap_or("");
    let host = authority
        .rsplit('@')
        .next()
        .unwrap_or(authority)
        .split(':')
        .next()
        .unwrap_or("")
        .trim();
    (!host.is_empty()).then(|| host.to_string())
}

#[cfg(unix)]
fn symlink_path(source: &Path, target: &Path) -> AgentResult<()> {
    std::os::unix::fs::symlink(source, target)?;
    Ok(())
}

#[cfg(windows)]
fn symlink_path(source: &Path, target: &Path) -> AgentResult<()> {
    if source.is_dir() {
        std::os::windows::fs::symlink_dir(source, target)?;
    } else {
        std::os::windows::fs::symlink_file(source, target)?;
    }
    Ok(())
}

fn write_codex_config(ctx: &AgentContext) -> AgentResult<()> {
    let mut config = String::new();
    config.push_str("developer_instructions = ");
    config.push_str(
        &serde_json::to_string(
            &std::fs::read_to_string(&ctx.paths.prompt_file).unwrap_or_default(),
        )
        .map_err(|err| AgentError::InvalidConfig(err.to_string()))?,
    );
    if let Some(port) = ctx.sandbox.as_ref().and_then(|_| world_proxy_port(ctx)) {
        config.push_str(
            "\ndefault_permissions = \"workspace\"\n\n\
             [sandbox_workspace_write]\n\
             writable_roots = [\"/workspace\"]\n\
             network_access = true\n\n\
             [permissions.workspace.filesystem]\n\
             \":minimal\" = \"read\"\n\
             \"/workspace\" = \"write\"\n\n\
             [permissions.workspace.network]\n\
             enabled = true\n",
        );
        config.push_str(&format!("proxy_url = \"http://127.0.0.1:{port}\"\n"));
        config.push_str(
            "enable_socks5 = false\n\
             allow_upstream_proxy = false\n\
             allow_local_binding = true\n\n\
             [permissions.workspace.network.domains]\n\
             \"world\" = \"allow\"\n\n",
        );
    }
    config.push_str("\n[features]\nhooks = true\n");
    if ctx.sandbox.is_some() && world_proxy_port(ctx).is_some() {
        config.push_str("network_proxy = true\n");
    }
    config.push('\n');
    let project_workspace = ctx
        .sandbox
        .as_ref()
        .map(|layout| layout.guest_workspace.display().to_string())
        .unwrap_or_else(|| ctx.paths.workspace_dir.display().to_string());
    config.push_str(&format!(
        "[projects.{}]\ntrust_level = \"trusted\"\n",
        serde_json::to_string(&project_workspace)
            .map_err(|err| AgentError::InvalidConfig(err.to_string()))?
    ));
    write_file(&codex_home_dir(ctx).join("config.toml"), &config)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CodexHookTrust {
    key: String,
    current_hash: String,
}

fn trust_codex_hooks(codex_binary: &Path, ctx: &AgentContext) -> AgentResult<()> {
    let Some(trust) = codex_stop_hook_trust(codex_binary, ctx)? else {
        return Ok(());
    };
    if trust.current_hash.trim().is_empty() {
        return Ok(());
    }

    let mut updates = vec![trust.clone()];
    if ctx.sandbox.is_some() {
        // The hook is discovered from the host path, but Codex later evaluates
        // it from inside bubblewrap as /home/agent/.codex/hooks.json. Trust both
        // keys so sandboxed Codex runs do not prompt for the same hook twice.
        let guest_key = "/home/agent/.codex/hooks.json:stop:0:0".to_string();
        if trust.key != guest_key {
            updates.push(CodexHookTrust {
                key: guest_key,
                current_hash: trust.current_hash,
            });
        }
    }
    append_codex_hook_trust(ctx, &updates)
}

fn codex_stop_hook_trust(
    codex_binary: &Path,
    ctx: &AgentContext,
) -> AgentResult<Option<CodexHookTrust>> {
    // Ask Codex itself for the hook key/hash instead of reimplementing its hash
    // algorithm. If the installed CLI cannot answer, fall back to the normal
    // interactive review flow rather than failing agent startup.
    //
    // Clear CODEX_HOME because this process may be launched by Codex itself,
    // where CODEX_HOME points at the controller's home and overrides HOME. The
    // generated agent config lives under the agent HOME instead.
    let mut child = match Command::new(codex_binary)
        .arg("app-server")
        .arg("--listen")
        .arg("stdio://")
        .env("HOME", agent_home_dir(ctx))
        .env_remove("CODEX_HOME")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return Ok(None),
    };

    let result = (|| -> AgentResult<Option<CodexHookTrust>> {
        let mut stdin = child.stdin.take().ok_or_else(|| {
            AgentError::InvalidConfig("Codex app-server stdin was unavailable".into())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AgentError::InvalidConfig("Codex app-server stdout was unavailable".into())
        })?;
        let mut stdout = BufReader::new(stdout);

        write_json_rpc(
            &mut stdin,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "clientInfo": {
                        "name": "clawblox",
                        "title": "clawblox",
                        "version": "0.1",
                    }
                },
            }),
        )?;
        if read_json_rpc_response(&mut stdout, 1)?.is_none() {
            return Ok(None);
        }
        write_json_rpc(
            &mut stdin,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "method": "initialized",
                "params": {},
            }),
        )?;
        write_json_rpc(
            &mut stdin,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "hooks/list",
                "params": {
                    "cwds": [ctx.paths.workspace_dir.display().to_string()],
                },
            }),
        )?;
        let Some(response) = read_json_rpc_response(&mut stdout, 2)? else {
            return Ok(None);
        };
        Ok(parse_codex_stop_hook_trust(&response))
    })();

    let _ = child.kill();
    let _ = child.wait();
    match result {
        Ok(value) => Ok(value),
        Err(_) => Ok(None),
    }
}

fn write_json_rpc(stdin: &mut impl Write, value: &serde_json::Value) -> AgentResult<()> {
    serde_json::to_writer(&mut *stdin, value)
        .map_err(|err| AgentError::InvalidConfig(err.to_string()))?;
    stdin.write_all(b"\n")?;
    stdin.flush()?;
    Ok(())
}

fn read_json_rpc_response(
    stdout: &mut impl BufRead,
    response_id: i64,
) -> AgentResult<Option<serde_json::Value>> {
    let mut line = String::new();
    for _ in 0..200 {
        line.clear();
        if stdout.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            continue;
        };
        if value.get("id").and_then(|id| id.as_i64()) == Some(response_id) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn parse_codex_stop_hook_trust(response: &serde_json::Value) -> Option<CodexHookTrust> {
    let hooks = response
        .get("result")?
        .get("data")?
        .as_array()?
        .first()?
        .get("hooks")?
        .as_array()?;
    hooks.iter().find_map(|hook| {
        (hook.get("eventName")?.as_str()? == "stop").then(|| CodexHookTrust {
            key: hook
                .get("key")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string(),
            current_hash: hook
                .get("currentHash")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string(),
        })
    })
}

fn append_codex_hook_trust(ctx: &AgentContext, updates: &[CodexHookTrust]) -> AgentResult<()> {
    let config_path = codex_home_dir(ctx).join("config.toml");
    let mut config = std::fs::read_to_string(&config_path).unwrap_or_default();
    for update in updates {
        if update.key.trim().is_empty() || update.current_hash.trim().is_empty() {
            continue;
        }
        let quoted_key = serde_json::to_string(&update.key)
            .map_err(|err| AgentError::InvalidConfig(err.to_string()))?;
        let quoted_hash = serde_json::to_string(&update.current_hash)
            .map_err(|err| AgentError::InvalidConfig(err.to_string()))?;
        let marker = format!("[hooks.state.{quoted_key}]");
        let block = format!("{marker}\ntrusted_hash = {quoted_hash}\n");
        config = replace_or_append_toml_table(&config, &marker, &block);
    }
    write_file(&config_path, &config)
}

fn replace_or_append_toml_table(config: &str, marker: &str, block: &str) -> String {
    let trimmed = config.trim_end();
    let Some(start) = trimmed.find(marker) else {
        if trimmed.is_empty() {
            return block.to_string();
        }
        return format!("{trimmed}\n\n{block}");
    };
    let before = trimmed[..start].trim_end();
    let after = &trimmed[start + marker.len()..];
    let next_table = after
        .find("\n[")
        .map(|offset| start + marker.len() + offset + 1);
    let suffix = next_table
        .map(|index| trimmed[index..].trim_start())
        .unwrap_or("");
    match (before.is_empty(), suffix.is_empty()) {
        (true, true) => block.to_string(),
        (true, false) => format!("{block}\n{suffix}"),
        (false, true) => format!("{before}\n\n{block}"),
        (false, false) => format!("{before}\n\n{block}\n{suffix}"),
    }
}

fn codex_approval_policy(input: &str) -> String {
    match input {
        "bypassPermissions" | "bypass" | "never" => "never".to_string(),
        "acceptEdits" | "default" => "on-request".to_string(),
        other => other.to_string(),
    }
}

fn collect_claude_transcript(ctx: &AgentContext) -> AgentResult<()> {
    let metadata = claude_hook_metadata_file(ctx);
    if !metadata.exists() {
        return Ok(());
    }

    let mut session_id = String::new();
    let mut transcript_path = String::new();
    for line in std::fs::read_to_string(&metadata)?.lines() {
        if let Some(value) = line.strip_prefix("SESSION_ID=") {
            session_id = unquote_shell_value(value);
        } else if let Some(value) = line.strip_prefix("TRANSCRIPT_PATH=") {
            transcript_path = unquote_shell_value(value);
        }
    }

    if !session_id.trim().is_empty() {
        create_parent_dir(&ctx.paths.session_id_file)?;
        std::fs::write(
            &ctx.paths.session_id_file,
            format!("{}\n", session_id.trim()),
        )?;
    }

    if transcript_path.trim().is_empty() {
        return Ok(());
    }
    let src = map_sandbox_path(ctx, transcript_path.trim());
    if src.is_file() {
        std::fs::create_dir_all(&ctx.paths.transcript_dir)?;
        let dst = ctx.paths.transcript_dir.join(
            src.file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("transcript.jsonl")),
        );
        std::fs::copy(src, dst)?;
    }
    Ok(())
}

fn collect_codex_transcript(ctx: &AgentContext) -> AgentResult<()> {
    let history = codex_history_file(ctx);
    if !history.exists() {
        return Ok(());
    }

    let mut latest_session_id = String::new();
    for raw in std::fs::read_to_string(&history)?.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(raw.trim()) else {
            continue;
        };
        if let Some(session_id) = value.get("session_id").and_then(|value| value.as_str()) {
            if !session_id.trim().is_empty() {
                latest_session_id = session_id.trim().to_string();
            }
        }
    }

    if latest_session_id.is_empty() {
        return Ok(());
    }

    create_parent_dir(&ctx.paths.session_id_file)?;
    std::fs::write(&ctx.paths.session_id_file, format!("{latest_session_id}\n"))?;

    let sessions_dir = codex_home_dir(ctx).join("sessions");
    if let Some(transcript) = find_latest_matching_jsonl(&sessions_dir, &latest_session_id)? {
        std::fs::create_dir_all(&ctx.paths.transcript_dir)?;
        std::fs::copy(
            transcript,
            ctx.paths
                .transcript_dir
                .join(format!("{latest_session_id}.jsonl")),
        )?;
    }
    Ok(())
}

fn map_sandbox_path(ctx: &AgentContext, path: &str) -> PathBuf {
    if ctx.settings.sandbox_enabled {
        if let Some(rel) = path.strip_prefix("/home/agent/") {
            return sandbox_home_dir(ctx).join(rel);
        }
        if let Some(rel) = path.strip_prefix("/workspace/") {
            return ctx.paths.workspace_dir.join(rel);
        }
        if let Some(rel) = path.strip_prefix("/runtime/") {
            return ctx.paths.runtime_dir.join(rel);
        }
        if let Some(rel) = path.strip_prefix("/logs/") {
            return ctx
                .paths
                .log_file
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .join(rel);
        }
    }
    PathBuf::from(path)
}

fn find_latest_matching_jsonl(root: &Path, needle: &str) -> AgentResult<Option<PathBuf>> {
    let mut matches = Vec::new();
    collect_matching_jsonl(root, needle, &mut matches)?;
    matches.sort();
    Ok(matches.pop())
}

fn collect_matching_jsonl(
    root: &Path,
    needle: &str,
    matches: &mut Vec<PathBuf>,
) -> AgentResult<()> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Ok(());
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_matching_jsonl(&path, needle, matches)?;
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.contains(needle) && name.ends_with(".jsonl"))
        {
            matches.push(path);
        }
    }
    Ok(())
}

fn unquote_shell_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        if (bytes[0] == b'\'' && bytes[trimmed.len() - 1] == b'\'')
            || (bytes[0] == b'"' && bytes[trimmed.len() - 1] == b'"')
        {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    }
    trimmed.to_string()
}

const CLAUDE_HOOKS_JSON: &str = r#"{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "\"${CLAUDE_PLUGIN_ROOT}/bin/session_start.sh\""
          }
        ]
      }
    ],
    "UserPromptSubmit": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "\"${CLAUDE_PLUGIN_ROOT}/bin/user_prompt_submit.sh\""
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "\"${CLAUDE_PLUGIN_ROOT}/bin/post_tool_use_chat.py\""
          }
        ]
      }
    ],
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "\"${CLAUDE_PLUGIN_ROOT}/bin/stop.sh\""
          }
        ]
      }
    ],
    "StopFailure": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "\"${CLAUDE_PLUGIN_ROOT}/bin/stop_failure.sh\""
          }
        ]
      }
    ],
    "Notification": [
      {
        "matcher": "idle_prompt",
        "hooks": [
          {
            "type": "command",
            "command": "\"${CLAUDE_PLUGIN_ROOT}/bin/notification.sh\""
          }
        ]
      }
    ],
    "SessionEnd": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "\"${CLAUDE_PLUGIN_ROOT}/bin/session_end.sh\""
          }
        ]
      }
    ]
  }
}
"#;

const CLAUDE_SESSION_START_SH: &str = r#"#!/usr/bin/env bash
set -euo pipefail

input="$(cat)"
session_id="$(INPUT_JSON="$input" node -e 'var data = JSON.parse(process.env.INPUT_JSON || "{}"); process.stdout.write(String(data.session_id || ""));')"
transcript_path="$(INPUT_JSON="$input" node -e 'var data = JSON.parse(process.env.INPUT_JSON || "{}"); process.stdout.write(String(data.transcript_path || ""));')"
log_dir="${CLAWBLOX_CLAUDE_LOG_DIR:-}"
metadata_file="${CLAWBLOX_CLAUDE_METADATA_FILE:-}"

if [[ -n "$log_dir" ]]; then
  mkdir -p "$log_dir"
  printf '%s\n' "$input" >"$log_dir/session_start.json"
  if [[ -n "$session_id" ]]; then
    printf '%s\n' "$input" >"$log_dir/session_start.${session_id}.json"
  fi
fi

if [[ -n "$metadata_file" ]]; then
  mkdir -p "$(dirname "$metadata_file")"
  {
    printf 'SESSION_ID=%q\n' "$session_id"
    printf 'TRANSCRIPT_PATH=%q\n' "$transcript_path"
  } >"$metadata_file"
fi
"#;

const CLAUDE_SESSION_END_SH: &str = r#"#!/usr/bin/env bash
set -euo pipefail

input="$(cat)"
session_id="$(INPUT_JSON="$input" node -e 'var data = JSON.parse(process.env.INPUT_JSON || "{}"); process.stdout.write(String(data.session_id || ""));')"
log_dir="${CLAWBLOX_CLAUDE_LOG_DIR:-}"

if [[ -n "$log_dir" ]]; then
  mkdir -p "$log_dir"
  printf '%s\n' "$input" >"$log_dir/session_end.json"
  if [[ -n "$session_id" ]]; then
    printf '%s\n' "$input" >"$log_dir/session_end.${session_id}.json"
  fi
fi
"#;

const CLAUDE_STOP_SH: &str = r#"#!/usr/bin/env bash
set -euo pipefail

input="$(cat)"
log_dir="${CLAWBLOX_CLAUDE_LOG_DIR:-}"
session_id="$(INPUT_JSON="$input" node -e 'var data = JSON.parse(process.env.INPUT_JSON || "{}"); process.stdout.write(String(data.session_id || ""));')"

if [[ -n "$log_dir" ]]; then
  mkdir -p "$log_dir"
  printf '%s\n' "$input" >"$log_dir/stop.json"
  if [[ -n "$session_id" ]]; then
    printf '%s\n' "$input" >"$log_dir/stop.${session_id}.json"
  fi
fi

auto_loop_message="${CLAUDE_AUTO_LOOP_MESSAGE:-}"
if [[ -z "$auto_loop_message" ]]; then
  auto_loop_message="Please don't stop. Just continue doing whatever you want to do."
fi

if [[ "${CLAWBLOX_CLAUDE_AUTO_LOOP:-0}" == "1" ]]; then
  CLAUDE_AUTO_LOOP_MESSAGE="$auto_loop_message" node -e 'process.stdout.write(JSON.stringify({ decision: "block", reason: process.env.CLAUDE_AUTO_LOOP_MESSAGE }))'
fi
"#;

const CLAUDE_POST_TOOL_USE_CHAT_PY: &str = r#"#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path
from urllib.error import URLError
from urllib.parse import urlencode
from urllib.request import Request, urlopen


CHAT_MESSAGES_LIMIT = 20


def normalize_name(value):
    return str(value or "").strip().lower()


def truncate_chat_content(value):
    trimmed = str(value or "").strip()
    if len(trimmed) <= 80:
        return trimmed
    return trimmed[:77] + "..."


def format_self_chat_feedback(message, world_agent_name):
    content = str(message.get("content") or "").strip()
    if not content:
        return None

    audibility = message.get("audibility")
    if not isinstance(audibility, list):
        audibility = []

    hearers = []
    for entry in audibility:
        if not isinstance(entry, dict) or not entry.get("heard"):
            continue
        name = str(entry.get("agent_name") or "").strip()
        if name and normalize_name(name) != world_agent_name:
            hearers.append(name)
    unique_hearers = list(dict.fromkeys(hearers))
    quoted_content = '"' + truncate_chat_content(content) + '"'

    if unique_hearers:
        return "Your message " + quoted_content + " was heard by " + ", ".join(unique_hearers) + "."
    if audibility:
        return "Your message " + quoted_content + " was not heard by anyone else."
    return "Your message " + quoted_content + " was sent, but hearing feedback was unavailable."


def format_chat_notification(message, world_agent_name):
    content = str(message.get("content") or "").strip()
    if not content:
        return None
    sender = str(message.get("agent_name") or "Someone").strip() or "Someone"
    if world_agent_name and normalize_name(sender) == world_agent_name:
        return format_self_chat_feedback(message, world_agent_name)
    return "New chat message from " + sender + ": " + content


def message_key(message):
    message_id = str(message.get("id") or "").strip()
    if message_id:
        return "id:" + message_id
    return "anon:" + "|".join(
        str(message.get(key) or "").strip()
        for key in ("created_at", "agent_name", "content")
    )


def load_state(path):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return {"baseline_loaded": False, "seen": [], "after": ""}


def save_state(path, state):
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(state, separators=(",", ":")) + "\n", encoding="utf-8")
    tmp.replace(path)


def fetch_messages(base_url, session_id, after):
    query = {"limit": str(CHAT_MESSAGES_LIMIT)}
    if after:
        query["after"] = after
    url = base_url.rstrip("/") + "/chat/messages?" + urlencode(query)
    request = Request(url, headers={"X-Session": session_id})
    with urlopen(request, timeout=2) as response:
        if response.status in (404, 405):
            return []
        if response.status != 200:
            raise RuntimeError("chat messages returned HTTP " + str(response.status))
        payload = json.loads(response.read().decode("utf-8"))
    messages = payload.get("messages")
    return messages if isinstance(messages, list) else []


def main():
    try:
        sys.stdin.read()
    except Exception:
        pass

    base_url = (
        os.environ.get("WORLD_INTERNAL_BASE_URL", "").strip()
        or os.environ.get("WORLD_BASE_URL", "").strip()
    )
    session_id = os.environ.get("WORLD_SESSION_ID", "").strip()
    world_agent_name = normalize_name(os.environ.get("WORLD_AGENT_NAME", ""))
    runtime_dir = os.environ.get("CLAWBLOX_CLAUDE_RUNTIME_DIR", "").strip()
    if not base_url or not session_id or not runtime_dir:
        return

    state_path = Path(runtime_dir) / "chat_post_tool_use_state.json"
    lock_path = Path(runtime_dir) / "chat_post_tool_use_state.lock"
    lock_path.parent.mkdir(parents=True, exist_ok=True)

    lock_handle = lock_path.open("a+", encoding="utf-8")
    try:
        try:
            import fcntl

            fcntl.flock(lock_handle.fileno(), fcntl.LOCK_EX)
        except Exception:
            pass

        state = load_state(state_path)
        seen = set(str(value) for value in state.get("seen", []) if value)
        messages = fetch_messages(base_url, session_id, str(state.get("after") or ""))

        notifications = []
        for message in reversed(messages):
            if not isinstance(message, dict):
                continue
            key = message_key(message)
            if key in seen:
                continue
            seen.add(key)

            created_at = str(message.get("created_at") or "").strip()
            if created_at and created_at > str(state.get("after") or ""):
                state["after"] = created_at

            if state.get("baseline_loaded"):
                text = format_chat_notification(message, world_agent_name)
                if text:
                    notifications.append(text)

        state["baseline_loaded"] = True
        state["seen"] = sorted(seen)[-200:]
        save_state(state_path, state)

        if notifications:
            print(json.dumps({
                "hookSpecificOutput": {
                    "hookEventName": "PostToolUse",
                    "additionalContext": "\n".join(notifications),
                }
            }))
    except (OSError, URLError, TimeoutError, RuntimeError, json.JSONDecodeError):
        return
    finally:
        try:
            lock_handle.close()
        except Exception:
            pass


if __name__ == "__main__":
    main()
"#;

const CODEX_STOP_HOOK_PY: &str = r#"#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path

payload = json.load(sys.stdin)
log_dir = os.environ.get("CLAWBLOX_CODEX_LOG_DIR", "")
session_id = str(payload.get("session_id") or "").strip()

if log_dir:
    path = Path(log_dir)
    path.mkdir(parents=True, exist_ok=True)
    raw = json.dumps(payload)
    (path / "stop.json").write_text(raw + "\n", encoding="utf-8")
    if session_id:
        (path / f"stop.{session_id}.json").write_text(raw + "\n", encoding="utf-8")

auto_loop = os.environ.get("CLAWBLOX_CODEX_AUTO_LOOP", "0")
auto_loop_message = os.environ.get("CODEX_AUTO_LOOP_MESSAGE", "")
if not auto_loop_message:
    auto_loop_message = "Please don't stop. Just continue doing whatever you want to do."

if auto_loop == "1":
    print(json.dumps({"decision": "block", "reason": auto_loop_message}))
"#;

// Kimi Stop hook — a non-blocking turn-ended SIGNAL, NOT an in-hook loop.
//
// The intuitive approach (what Claude and Codex do) is to BLOCK the stop: return
// `decision:block` / exit 2 from the Stop hook so the runtime re-prompts and the
// turn never ends. That does not work on Kimi. Kimi's Stop hook is documented to
// continue a turn *at most once* ("the turn may continue once"), and the cap is
// enforced in Kimi's runtime ABOVE the hook: after one block-continuation it
// returns early and stops invoking the Stop hook for the rest of that turn
// (verified in agent-core `turn/index.ts` and empirically — exactly one
// continuation per turn). So an exit-2 hook buys one extra step and then the
// agent goes idle. The cap is not configurable and cannot be overridden from the
// hook; the only code-level fix would be patching Kimi, which we do not do.
//
// Instead we keep the hook NON-BLOCKING (exit 0). A hook that never blocks never
// trips the one-continuation cap, so Kimi invokes it on EVERY turn-end. The hook
// just appends one line to <CLAWBLOX_KIMI_LOG_DIR>/autoloop_fires; clawblox's
// driver-side auto-continue watcher (see `start_kimi_autoloop_watcher`) tails
// that file and types the auto-continue message into the live TUI to start the
// next turn. From the model's point of view the conversation is identical to
// Claude/Codex (`assistant stop -> "please don't stop" -> continue`).
const KIMI_STOP_HOOK_PY: &str = r#"#!/usr/bin/env python3
import json
import os
import sys
import time
from pathlib import Path

try:
    payload = json.load(sys.stdin)
except Exception:
    payload = {}
log_dir = os.environ.get("CLAWBLOX_KIMI_LOG_DIR", "")
session_id = str(payload.get("session_id") or "").strip()

if log_dir:
    path = Path(log_dir)
    path.mkdir(parents=True, exist_ok=True)
    raw = json.dumps(payload)
    (path / "stop.json").write_text(raw + "\n", encoding="utf-8")
    if session_id:
        (path / f"stop.{session_id}.json").write_text(raw + "\n", encoding="utf-8")
    # Turn-ended signal tailed by clawblox's auto-continue watcher: one line per
    # turn-end. Append-only so the watcher can detect new fires by line count.
    with open(path / "autoloop_fires", "a", encoding="utf-8") as handle:
        handle.write(f"{time.time():.3f} {session_id}\n")

# Exit 0 (do NOT block). Continuation is driven by clawblox's watcher, not the
# hook — see the comment above for why an exit-2 block cannot loop on Kimi.
"#;

pub fn codex_bind_root_for_bin(path: &Path) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let path = std::fs::canonicalize(path).ok()?;
    let text = path.to_string_lossy();
    for marker in [
        "/node_modules/.bin/codex",
        "/node_modules/@openai/codex/bin/codex.js",
    ] {
        if let Some(root) = text.strip_suffix(marker) {
            return Some((PathBuf::from(root), PathBuf::from(marker)));
        }
    }
    None
}

pub fn node_root_for_path(path: &Path) -> Option<PathBuf> {
    let path = std::fs::canonicalize(path).ok()?;
    let components: Vec<_> = path.components().collect();
    for index in (2..components.len()).rev() {
        let candidate: PathBuf = components[..index].iter().collect();
        if candidate.join("bin/node").is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn resolve_node_root() -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let node = dir.join("node");
        if node.exists() {
            return node_root_for_path(&node);
        }
    }
    None
}

fn require_nonempty(name: &str, value: &str) -> AgentResult<()> {
    if value.trim().is_empty() {
        return Err(AgentError::InvalidConfig(format!("{name} is required")));
    }
    Ok(())
}

fn require_nonempty_path(name: &str, value: &Path) -> AgentResult<()> {
    if value.as_os_str().is_empty() {
        return Err(AgentError::InvalidConfig(format!("{name} is required")));
    }
    Ok(())
}

/// Construct an agent by name. This is the single place to register a new agent.
pub fn build_agent(
    kind: &str,
    binary: Option<PathBuf>,
    bare: bool,
) -> Result<Box<dyn Agent>, String> {
    match kind {
        "claude" => Ok(Box::new(ClaudeAgent {
            binary: binary.unwrap_or_else(|| ClaudeAgent::default().binary),
            bare,
            ..ClaudeAgent::default()
        })),
        "codex" => Ok(Box::new(CodexAgent {
            binary: binary.unwrap_or_else(|| CodexAgent::default().binary),
        })),
        "kimi" => Ok(Box::new(KimiAgent {
            binary: binary.unwrap_or_else(|| KimiAgent::default().binary),
        })),
        other => Err(format!("unknown agent kind '{other}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct EchoAgent;

    impl Agent for EchoAgent {
        fn name(&self) -> &'static str {
            "echo"
        }

        fn default_model(&self) -> &'static str {
            "echo-model"
        }

        fn binary(&self) -> &Path {
            Path::new("echo")
        }

        fn check_setup(&self, ctx: &AgentContext) -> AgentResult<()> {
            ctx.validate()
        }

        fn start(&self, ctx: &AgentContext) -> AgentResult<CommandSpec> {
            ctx.validate()?;
            Ok(CommandSpec::new("echo", &ctx.paths.workspace_dir)
                .arg(&ctx.settings.initial_prompt)
                .env("AGENT_MODEL", &ctx.settings.model)
                .env(
                    "SESSION_ID_FILE",
                    ctx.paths.session_id_file.display().to_string(),
                ))
        }

        fn continue_session(&self, ctx: &AgentContext, prompt: &str) -> AgentResult<CommandSpec> {
            ctx.validate()?;
            require_nonempty("prompt", prompt)?;
            Ok(CommandSpec::new("echo", &ctx.paths.workspace_dir)
                .arg(prompt)
                .env(
                    "SESSION_ID_FILE",
                    ctx.paths.session_id_file.display().to_string(),
                ))
        }

        fn configure_auto_continue(&self, ctx: &AgentContext) -> AgentResult<()> {
            ctx.settings.validate()
        }

        fn transcript_parser(&self) -> Box<dyn TranscriptLineParser> {
            Box::new(ClaudeTranscriptParser)
        }

        fn collect_transcripts(&self, ctx: &AgentContext) -> AgentResult<()> {
            ctx.paths.validate()
        }

        fn sandbox_requirements(&self, ctx: &AgentContext) -> AgentResult<SandboxRequirements> {
            ctx.validate()?;
            Ok(SandboxRequirements {
                read_only_paths: vec![ctx.paths.prompt_file.clone()],
                writable_paths: vec![
                    ctx.paths.workspace_dir.clone(),
                    ctx.paths.runtime_dir.clone(),
                    ctx.paths.transcript_dir.clone(),
                    ctx.paths.agent_home_dir.clone(),
                ],
                env_vars: vec!["HOME".to_string()],
                needs_network: ctx.settings.sandbox_enabled,
            })
        }
    }

    #[test]
    fn prepares_explicit_agent_paths_without_assuming_layout() {
        let ctx = test_context();
        ctx.prepare_paths().unwrap();

        assert!(ctx.paths.workspace_dir.is_dir());
        assert!(ctx.paths.transcript_dir.is_dir());
        assert!(ctx.paths.runtime_dir.is_dir());
        assert!(ctx.paths.agent_home_dir.is_dir());
        assert!(ctx.paths.prompt_file.parent().unwrap().is_dir());
        assert!(ctx.paths.session_id_file.parent().unwrap().is_dir());
        assert!(ctx.paths.log_file.parent().unwrap().is_dir());
    }

    #[test]
    fn agent_commands_use_provided_paths_and_settings() {
        let ctx = test_context();
        let agent = EchoAgent;

        let start = agent.start(&ctx).unwrap();
        assert_eq!(agent.name(), "echo");
        assert_eq!(start.cwd, ctx.paths.workspace_dir);
        assert_eq!(start.args, vec!["Begin".to_string()]);
        assert!(start
            .env
            .contains(&("AGENT_MODEL".to_string(), "test-model".to_string())));
        assert!(start.env.contains(&(
            "SESSION_ID_FILE".to_string(),
            ctx.paths.session_id_file.display().to_string()
        )));

        let next = agent.continue_session(&ctx, "Keep going").unwrap();
        assert_eq!(next.args, vec!["Keep going".to_string()]);
    }

    #[test]
    fn rejects_missing_required_settings() {
        let mut ctx = test_context();
        ctx.settings.auto_continue_prompt.clear();

        let err = ctx.validate().unwrap_err().to_string();
        assert!(err.contains("auto_continue_prompt is required"));
    }

    #[test]
    fn exposes_sandbox_requirements_as_data() {
        let ctx = test_context();
        let requirements = EchoAgent.sandbox_requirements(&ctx).unwrap();

        assert!(requirements.needs_network);
        assert!(requirements
            .writable_paths
            .contains(&ctx.paths.workspace_dir));
        assert!(requirements
            .read_only_paths
            .contains(&ctx.paths.prompt_file));
    }

    #[test]
    fn claude_agent_builds_start_and_continue_commands() {
        let ctx = test_context();
        ctx.prepare_paths().unwrap();
        let agent = ClaudeAgent {
            binary: PathBuf::from("/bin/echo"),
            bare: true,
            disallowed_tools: Some("WebFetch,WebSearch".to_string()),
        };

        agent.configure_auto_continue(&ctx).unwrap();
        let start = agent.start(&ctx).unwrap();

        assert_eq!(start.program, PathBuf::from("/bin/echo"));
        assert_eq!(start.cwd, ctx.paths.workspace_dir);
        assert!(start.args.contains(&"--bare".to_string()));
        assert!(start.args.contains(&"--plugin-dir".to_string()));
        assert!(start.args.contains(&"--session-id".to_string()));
        assert!(ctx.paths.session_id_file.is_file());
        assert!(claude_plugin_dir(&ctx).join("bin/stop.sh").is_file());
        assert!(claude_plugin_dir(&ctx)
            .join("bin/post_tool_use_chat.py")
            .is_file());
        for hook in [
            "session_start.sh",
            "session_end.sh",
            "stop.sh",
            "post_tool_use_chat.py",
        ] {
            let script =
                std::fs::read_to_string(claude_plugin_dir(&ctx).join("bin").join(hook)).unwrap();
            assert!(
                !script.contains("??") && !script.contains("?."),
                "{hook} should stay compatible with older Node runtimes"
            );
        }
        assert!(start.env.contains(&(
            "CLAUDE_AUTO_LOOP_MESSAGE".to_string(),
            "Keep going".to_string()
        )));
        assert!(start.env.contains(&(
            "CLAWBLOX_CLAUDE_RUNTIME_DIR".to_string(),
            ctx.paths.runtime_dir.display().to_string()
        )));

        let next = agent.continue_session(&ctx, "Later").unwrap();
        assert!(next.args.contains(&"--continue".to_string()));
        assert_eq!(next.args.last(), Some(&"Later".to_string()));
    }

    #[test]
    fn codex_agent_configures_hooks_and_resumes_saved_session() {
        let ctx = test_context();
        ctx.prepare_paths().unwrap();
        std::fs::write(&ctx.paths.prompt_file, "System prompt").unwrap();
        std::fs::write(&ctx.paths.session_id_file, "codex-session\n").unwrap();
        let agent = CodexAgent {
            binary: PathBuf::from("/bin/echo"),
        };

        agent.configure_auto_continue(&ctx).unwrap();
        let start = agent.start(&ctx).unwrap();
        assert_eq!(start.args.first(), Some(&"--no-alt-screen".to_string()));
        assert!(codex_hooks_path(&ctx).is_file());
        assert!(codex_stop_hook_path(&ctx).is_file());
        assert!(codex_home_dir(&ctx).join("config.toml").is_file());

        let next = agent.continue_session(&ctx, "Resume prompt").unwrap();
        assert_eq!(next.args.first(), Some(&"resume".to_string()));
        assert!(next.args.contains(&"codex-session".to_string()));
        assert_eq!(next.args.last(), Some(&"Resume prompt".to_string()));
    }

    #[test]
    fn kimi_agent_configures_k3_model() {
        let mut ctx = test_context();
        ctx.settings.model = "kimi-code/k3".to_string();
        ctx.prepare_paths().unwrap();

        KimiAgent::default().configure_auto_continue(&ctx).unwrap();

        let config = std::fs::read_to_string(kimi_home_dir(&ctx).join("config.toml")).unwrap();
        assert!(config.contains("default_model = \"kimi-code/k3\""));
        assert!(config.contains("[models.\"kimi-code/k3\"]"));
        assert!(config.contains("model = \"k3\""));
        assert!(config.contains("max_context_size = 1048576"));
        assert!(config.contains("default_effort = \"max\""));
    }

    #[test]
    fn claude_agent_uses_guest_paths_when_sandboxed() {
        let mut ctx = test_context();
        ctx.settings.sandbox_enabled = true;
        ctx.settings.env = vec![
            ("WORLD_BASE_URL".to_string(), "http://world".to_string()),
            (
                "CLAWBLOX_WORLD_HTTP_PROXY_PORT".to_string(),
                "43210".to_string(),
            ),
        ];
        ctx.prepare_paths().unwrap();
        std::fs::write(&ctx.paths.prompt_file, "System prompt").unwrap();
        ctx.sandbox = Some(
            SandboxLayout::for_agent(&ctx, PathBuf::from("/bin/echo"), "claude")
                .with_claude_plugin()
                .with_hosts_file(ctx.paths.runtime_dir.join("sandbox_hosts")),
        );
        let agent = ClaudeAgent {
            binary: PathBuf::from("/bin/echo"),
            bare: false,
            disallowed_tools: Some("WebFetch,WebSearch".to_string()),
        };

        agent.configure_auto_continue(&ctx).unwrap();
        let start = agent.start(&ctx).unwrap();

        assert_eq!(start.program, PathBuf::from("/sandbox-bin/claude"));
        assert_eq!(start.cwd, PathBuf::from("/workspace"));
        assert!(start.args.contains(&"/plugin".to_string()));
        assert!(start.args.contains(&"/tmp/system_prompt.md".to_string()));
        assert!(start
            .args
            .windows(2)
            .any(|window| window == ["--settings", "/runtime/claude_sandbox_settings.json"]));
        assert!(start
            .env
            .contains(&("CLAWBLOX_CLAUDE_LOG_DIR".to_string(), "/logs".to_string())));
        assert!(start.env.contains(&(
            "CLAWBLOX_CLAUDE_METADATA_FILE".to_string(),
            "/runtime/hook_session.env".to_string()
        )));
        assert!(start.env.contains(&(
            "CLAWBLOX_CLAUDE_RUNTIME_DIR".to_string(),
            "/runtime".to_string()
        )));
        let settings =
            std::fs::read_to_string(ctx.paths.runtime_dir.join("claude_sandbox_settings.json"))
                .unwrap();
        assert!(settings.contains("\"httpProxyPort\": 43210"));
        assert!(ctx.paths.runtime_dir.join("sandbox_hosts").is_file());
    }

    #[test]
    fn claude_sandbox_home_gets_autonomous_settings_when_fresh() {
        let ctx = test_context();
        ctx.prepare_paths().unwrap();
        let layout = SandboxLayout::for_agent(&ctx, PathBuf::from("/bin/echo"), "claude")
            .with_claude_plugin();

        prepare_claude_sandbox_home(&ctx, &layout).unwrap();

        let settings: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(layout.host_home_dir.join(".claude/settings.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            settings["skipDangerousModePermissionPrompt"],
            serde_json::json!(true)
        );
        assert_eq!(
            settings["skipAutoPermissionPrompt"],
            serde_json::json!(true)
        );
        assert_eq!(
            settings["permissions"]["defaultMode"],
            serde_json::json!("auto")
        );
    }

    #[test]
    fn claude_sandbox_home_respects_context_oauth_token() {
        let mut ctx = test_context();
        ctx.prepare_paths().unwrap();
        let host_claude_dir = ctx.paths.runtime_dir.join("host-claude");
        std::fs::create_dir_all(&host_claude_dir).unwrap();
        std::fs::write(
            host_claude_dir.join(".credentials.json"),
            r#"{"claudeAiOauth":{"accessToken":"stale-host-token"}}"#,
        )
        .unwrap();
        ctx.settings.env.push((
            "CLAUDE_CONFIG_DIR".to_string(),
            host_claude_dir.display().to_string(),
        ));
        ctx.settings.env.push((
            "CLAUDE_CODE_OAUTH_TOKEN".to_string(),
            "fresh-context-token".to_string(),
        ));
        let layout = SandboxLayout::for_agent(&ctx, PathBuf::from("/bin/echo"), "claude")
            .with_claude_plugin();

        prepare_claude_sandbox_home(&ctx, &layout).unwrap();

        assert!(!layout
            .host_home_dir
            .join(".claude/.credentials.json")
            .exists());
    }

    #[test]
    fn claude_sandbox_home_preserves_existing_settings() {
        let ctx = test_context();
        ctx.prepare_paths().unwrap();
        let layout = SandboxLayout::for_agent(&ctx, PathBuf::from("/bin/echo"), "claude")
            .with_claude_plugin();
        let settings_path = layout.host_home_dir.join(".claude/settings.json");
        std::fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
        std::fs::write(
            &settings_path,
            r#"{"theme":"dark","permissions":{"defaultMode":"acceptEdits"}}"#,
        )
        .unwrap();

        write_claude_sandbox_autonomous_settings(&settings_path).unwrap();

        let settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert_eq!(settings["theme"], serde_json::json!("dark"));
        assert_eq!(
            settings["permissions"]["defaultMode"],
            serde_json::json!("acceptEdits")
        );
        assert_eq!(
            settings["skipDangerousModePermissionPrompt"],
            serde_json::json!(true)
        );
        assert_eq!(
            settings["skipAutoPermissionPrompt"],
            serde_json::json!(true)
        );
    }

    #[test]
    fn claude_agent_can_disable_native_sandbox_when_outer_sandboxed() {
        let mut ctx = test_context();
        ctx.settings.sandbox_enabled = true;
        ctx.settings.env = vec![
            (
                "WORLD_BASE_URL".to_string(),
                "http://world:43210".to_string(),
            ),
            (
                "CLAWBLOX_CLAUDE_NATIVE_SANDBOX".to_string(),
                "0".to_string(),
            ),
        ];
        ctx.prepare_paths().unwrap();
        std::fs::write(&ctx.paths.prompt_file, "System prompt").unwrap();
        ctx.sandbox = Some(
            SandboxLayout::for_agent(&ctx, PathBuf::from("/bin/echo"), "claude")
                .with_claude_plugin()
                .with_hosts_file(ctx.paths.runtime_dir.join("sandbox_hosts")),
        );
        let agent = ClaudeAgent {
            binary: PathBuf::from("/bin/echo"),
            bare: false,
            disallowed_tools: Some("WebFetch,WebSearch".to_string()),
        };

        agent.configure_auto_continue(&ctx).unwrap();
        let start = agent.start(&ctx).unwrap();

        assert!(start
            .args
            .windows(2)
            .any(|window| window == ["--settings", "/runtime/claude_sandbox_settings.json"]));
        let settings: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(ctx.paths.runtime_dir.join("claude_sandbox_settings.json"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(settings["sandbox"]["enabled"], serde_json::json!(false));
    }

    #[test]
    fn codex_agent_uses_guest_paths_and_proxy_config_when_sandboxed() {
        let mut ctx = test_context();
        ctx.settings.sandbox_enabled = true;
        ctx.settings.env = vec![
            ("WORLD_BASE_URL".to_string(), "http://world".to_string()),
            (
                "CLAWBLOX_WORLD_HTTP_PROXY_PORT".to_string(),
                "43210".to_string(),
            ),
        ];
        ctx.prepare_paths().unwrap();
        std::fs::write(&ctx.paths.prompt_file, "System prompt").unwrap();
        std::fs::write(&ctx.paths.session_id_file, "codex-session\n").unwrap();
        ctx.sandbox = Some(SandboxLayout::for_agent(
            &ctx,
            PathBuf::from("/bin/echo"),
            "codex",
        ));
        let agent = CodexAgent {
            binary: PathBuf::from("/bin/echo"),
        };

        agent.configure_auto_continue(&ctx).unwrap();
        let start = agent.start(&ctx).unwrap();

        assert_eq!(start.program, PathBuf::from("/sandbox-bin/codex"));
        assert_eq!(start.cwd, PathBuf::from("/workspace"));
        assert!(start
            .args
            .windows(2)
            .any(|window| window == ["-C", "/workspace"]));
        assert!(start
            .args
            .iter()
            .any(|arg| arg == "--dangerously-bypass-approvals-and-sandbox"));
        assert!(!start.args.iter().any(|arg| arg == "-s"));
        assert!(!start.args.iter().any(|arg| arg == "-a"));
        assert!(start
            .env
            .contains(&("HOME".to_string(), "/home/agent".to_string())));
        assert!(start.env.contains(&(
            "CLAWBLOX_CODEX_RUNTIME_DIR".to_string(),
            "/runtime".to_string()
        )));
        let config = std::fs::read_to_string(codex_home_dir(&ctx).join("config.toml")).unwrap();
        assert!(config.contains("[permissions.workspace.filesystem]"));
        assert!(config.contains("\"/workspace\" = \"write\""));
        assert!(config.contains("proxy_url = \"http://127.0.0.1:43210\""));
        assert!(config.contains("[projects.\"/workspace\"]"));
        assert!(std::fs::read_to_string(codex_hooks_path(&ctx))
            .unwrap()
            .contains("/home/agent/.codex/stop_hook.py"));
    }

    #[test]
    fn codex_agent_resumes_after_program_arg_when_sandbox_uses_node() {
        let mut ctx = test_context();
        ctx.settings.sandbox_enabled = true;
        ctx.prepare_paths().unwrap();
        std::fs::write(&ctx.paths.prompt_file, "System prompt").unwrap();
        std::fs::write(&ctx.paths.session_id_file, "codex-session\n").unwrap();
        ctx.sandbox = Some(
            SandboxLayout::for_agent(
                &ctx,
                PathBuf::from("/host/node_modules/@openai/codex/bin/codex.js"),
                "codex",
            )
            .with_codex_binary_root(
                PathBuf::from("/host"),
                PathBuf::from("/node_modules/@openai/codex/bin/codex.js"),
            )
            .with_node_root(PathBuf::from("/host/node")),
        );
        let agent = CodexAgent {
            binary: PathBuf::from("/bin/echo"),
        };

        let next = agent.continue_session(&ctx, "Resume prompt").unwrap();

        assert_eq!(next.program, PathBuf::from("/sandbox-node/bin/node"));
        assert_eq!(
            next.args.get(0),
            Some(&"/sandbox-codex/node_modules/@openai/codex/bin/codex.js".to_string())
        );
        assert_eq!(next.args.get(1), Some(&"resume".to_string()));
        assert!(next.args.contains(&"codex-session".to_string()));
        assert_eq!(next.args.last(), Some(&"Resume prompt".to_string()));
    }

    #[test]
    fn codex_hook_trust_table_is_appended_or_replaced() {
        let marker = "[hooks.state.\"/home/agent/.codex/hooks.json:stop:0:0\"]";
        let first = format!("{marker}\ntrusted_hash = \"old\"\n");
        let second = format!("{marker}\ntrusted_hash = \"new\"\n");

        let appended = replace_or_append_toml_table("[features]\nhooks = true\n", marker, &first);
        assert!(appended.contains("[features]\nhooks = true"));
        assert!(appended.contains("trusted_hash = \"old\""));

        let replaced = replace_or_append_toml_table(
            &format!(
                "[features]\nhooks = true\n\n{first}\n[projects.\"/workspace\"]\ntrust_level = \"trusted\"\n"
            ),
            marker,
            &second,
        );
        assert!(!replaced.contains("trusted_hash = \"old\""));
        assert!(replaced.contains("trusted_hash = \"new\""));
        assert!(replaced.contains("[projects.\"/workspace\"]"));
    }

    #[test]
    fn codex_agent_collects_session_id_and_transcript_from_history() {
        let ctx = test_context();
        ctx.prepare_paths().unwrap();
        let sessions_dir = codex_home_dir(&ctx).join("sessions/2026/05/17");
        std::fs::create_dir_all(&sessions_dir).unwrap();
        std::fs::create_dir_all(codex_home_dir(&ctx)).unwrap();
        std::fs::write(
            codex_history_file(&ctx),
            "{\"session_id\":\"old\"}\n{\"session_id\":\"new-session\"}\n",
        )
        .unwrap();
        std::fs::write(
            sessions_dir.join("rollout-new-session.jsonl"),
            "{\"type\":\"event\"}\n",
        )
        .unwrap();

        CodexAgent::default().collect_transcripts(&ctx).unwrap();

        assert_eq!(
            std::fs::read_to_string(&ctx.paths.session_id_file).unwrap(),
            "new-session\n"
        );
        assert!(ctx.paths.transcript_dir.join("new-session.jsonl").is_file());
    }

    fn test_context() -> AgentContext {
        let root = std::env::temp_dir().join(format!(
            "clawblox-agent-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        AgentContext {
            paths: AgentPaths {
                workspace_dir: root.join("workspace"),
                prompt_file: root.join("runtime/system_prompt.md"),
                session_id_file: root.join("state/session_id.txt"),
                transcript_dir: root.join("transcripts"),
                runtime_dir: root.join("runtime"),
                log_file: root.join("logs/agent.log"),
                agent_home_dir: root.join("home"),
            },
            settings: AgentSettings {
                model: "test-model".to_string(),
                permission_mode: "test-mode".to_string(),
                initial_prompt: "Begin".to_string(),
                lifecycle_prompt: "Wrap up".to_string(),
                auto_continue_prompt: "Keep going".to_string(),
                extra_args: vec!["--flag".to_string()],
                env: vec![("WORLD_BASE_URL".to_string(), "http://world".to_string())],
                sandbox_enabled: true,
                version: None,
            },
            sandbox: None,
        }
    }

    #[test]
    fn only_kimi_requests_the_autoloop_watcher() {
        // Claude/Codex loop inside their Stop hook; Kimi cannot (its hook is
        // capped at one continuation per turn) and relies on the driver-side
        // watcher instead.
        assert!(!EchoAgent.needs_autoloop_watcher());
        assert!(KimiAgent::default().needs_autoloop_watcher());
    }

    /// Guard the load-bearing decision: the Kimi Stop hook must be NON-blocking
    /// and emit one `autoloop_fires` line per turn-end. If someone "fixes" it
    /// back to the intuitive exit-2 block, the loop silently dies on Kimi (the
    /// runtime caps the hook at one continuation per turn), so this test fails
    /// loudly instead.
    #[test]
    fn kimi_stop_hook_is_nonblocking_and_signals_each_fire() {
        if std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("skipping: python3 not available");
            return;
        }

        let dir = std::env::temp_dir().join(format!(
            "clawblox-kimi-hook-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let log_dir = dir.join("logs");
        std::fs::create_dir_all(&log_dir).unwrap();
        let hook = dir.join("stop_hook.py");
        write_executable_file(&hook, KIMI_STOP_HOOK_PY).unwrap();

        let run = || {
            use std::io::Write;
            let mut child = std::process::Command::new("python3")
                .arg(&hook)
                .env("CLAWBLOX_KIMI_LOG_DIR", &log_dir)
                .env("CLAWBLOX_KIMI_AUTO_LOOP", "1")
                .env(
                    "KIMI_AUTO_LOOP_MESSAGE",
                    "Please don't stop. Just continue doing whatever you want to do.",
                )
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(br#"{"session_id":"s1"}"#)
                .unwrap();
            child.wait_with_output().unwrap()
        };

        let first = run();
        // Non-blocking: exit 0, and the message must NOT be written to stderr
        // (stderr + exit 2 is the blocking path that cannot loop on Kimi).
        assert_eq!(
            first.status.code(),
            Some(0),
            "hook must exit 0 (non-blocking)"
        );
        assert!(
            String::from_utf8_lossy(&first.stderr).trim().is_empty(),
            "hook must not emit a block reason on stderr"
        );

        let fires = log_dir.join("autoloop_fires");
        let count = |p: &Path| {
            std::fs::read_to_string(p)
                .map(|t| t.lines().filter(|l| !l.trim().is_empty()).count())
                .unwrap_or(0)
        };
        assert_eq!(count(&fires), 1, "one fire line per turn-end");

        run();
        assert_eq!(count(&fires), 2, "each turn-end appends another fire line");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn copy_path_keeps_the_source_mode_and_does_not_follow_a_planted_symlink() {
        use std::os::unix::fs::PermissionsExt;
        let root = test_context().paths.workspace_dir.parent().unwrap().to_path_buf();
        let source = root.join("skills/tool/run.sh");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, "#!/bin/sh\necho ok\n").unwrap();
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o755)).unwrap();

        let target = root.join("home/skills/tool/run.sh");
        copy_path(&source, &target).unwrap();
        let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755, "a copied script stays executable");

        // a symlink planted at the target is replaced, and its own target keeps its mode
        let outside = root.join("outside.txt");
        std::fs::write(&outside, "keep").unwrap();
        std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::remove_file(&target).unwrap();
        std::os::unix::fs::symlink(&outside, &target).unwrap();
        copy_path(&source, &target).unwrap();
        assert!(!std::fs::symlink_metadata(&target).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "keep");
        assert_eq!(std::fs::metadata(&outside).unwrap().permissions().mode() & 0o777, 0o600);

        let _ = std::fs::remove_dir_all(&root);
    }
}
