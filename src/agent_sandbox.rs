use std::path::{Path, PathBuf};

use crate::agent::{AgentContext, CommandSpec};
use crate::config::SandboxConfig;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SandboxLayout {
    pub host_home_dir: PathBuf,
    pub host_binary: PathBuf,
    pub guest_workspace: PathBuf,
    pub guest_home: PathBuf,
    pub guest_logs: PathBuf,
    pub guest_runtime: PathBuf,
    pub guest_prompt: PathBuf,
    pub guest_plugin: Option<PathBuf>,
    pub guest_binary: PathBuf,
    pub guest_binary_root: Option<PathBuf>,
    pub host_binary_root: Option<PathBuf>,
    pub guest_binary_rel: Option<PathBuf>,
    pub host_node_root: Option<PathBuf>,
    pub guest_node_root: Option<PathBuf>,
    pub host_deps_root: Option<PathBuf>,
    pub add_deps_lib_to_library_path: bool,
    pub host_hosts_file: Option<PathBuf>,
    pub host_shm_dir: Option<PathBuf>,
    pub enable_weaker_nested_sandbox: bool,
    pub unset_env: Vec<String>,
    /// when set, the sandbox gets no network except these
    /// endpoints, each bridged in through a unix socket.
    pub net_bridges: Option<Vec<NetBridge>>,
    /// host programs masked (bound over with /dev/null) inside.
    pub hide_programs: Vec<PathBuf>,
}

/// One TCP endpoint reachable from inside a network-isolated sandbox: the sandbox
/// sees it at 127.0.0.1:`port` (and as `name` in /etc/hosts); outside, the bridge
/// connects to `host`:`port`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetBridge {
    pub name: String,
    pub host: String,
    pub port: u16,
}

impl NetBridge {
    pub fn new(name: &str, host: &str, port: u16) -> Self {
        Self {
            name: name.to_string(),
            host: host.to_string(),
            port,
        }
    }
}

/// Host-side dir for one agent's bridge sockets. It lives in the run container's own
/// temp dir: run dirs may be bind mounts that can't hold unix sockets (Docker Desktop
/// on macOS), and the sandbox gets a private tmpfs at /tmp, so it can't see this.
pub fn net_bridge_dir(ctx: &AgentContext) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    agent_dir_from_context(ctx).hash(&mut hasher);
    std::env::temp_dir().join(format!("clawblox-net-{:016x}", hasher.finish()))
}

/// Parse `name=host:port[,name=host:port...]`.
pub fn parse_net_bridges(spec: &str) -> Result<Vec<NetBridge>, String> {
    let mut bridges = Vec::new();
    for item in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (name, target) = item
            .split_once('=')
            .ok_or_else(|| format!("bad net bridge {item:?}: expected name=host:port"))?;
        let (host, port) = target
            .rsplit_once(':')
            .ok_or_else(|| format!("bad net bridge {item:?}: expected name=host:port"))?;
        let ok = |v: &str| {
            !v.is_empty()
                && v
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
        };
        if !ok(name) || !ok(host) {
            return Err(format!("bad net bridge {item:?}"));
        }
        let port: u16 = port
            .parse()
            .map_err(|_| format!("bad net bridge port in {item:?}"))?;
        bridges.push(NetBridge::new(name, host, port));
    }
    Ok(bridges)
}

/// `CLAWBLOX_SANDBOX_NETWORK=isolated` with `CLAWBLOX_SANDBOX_NET_BRIDGES`.
/// `Ok(None)` when isolation isn't requested.
pub fn net_bridges_from_env() -> Result<Option<Vec<NetBridge>>, String> {
    match std::env::var("CLAWBLOX_SANDBOX_NETWORK").ok().as_deref() {
        None | Some("") | Some("shared") => Ok(None),
        Some("isolated") => parse_net_bridges(
            &std::env::var("CLAWBLOX_SANDBOX_NET_BRIDGES").unwrap_or_default(),
        )
        .map(Some),
        Some(other) => Err(format!("unknown CLAWBLOX_SANDBOX_NETWORK={other:?}")),
    }
}

impl SandboxLayout {
    pub fn for_agent(ctx: &AgentContext, host_binary: PathBuf, guest_binary_name: &str) -> Self {
        let host_home_dir = agent_dir_from_context(ctx).join("sandbox-home");
        let guest_binary = PathBuf::from(format!("/sandbox-bin/{guest_binary_name}"));
        Self {
            host_home_dir,
            host_binary,
            guest_workspace: PathBuf::from("/workspace"),
            guest_home: PathBuf::from("/home/agent"),
            guest_logs: PathBuf::from("/logs"),
            guest_runtime: PathBuf::from("/runtime"),
            guest_prompt: PathBuf::from("/tmp/system_prompt.md"),
            guest_plugin: None,
            guest_binary,
            guest_binary_root: None,
            host_binary_root: None,
            guest_binary_rel: None,
            host_node_root: None,
            guest_node_root: None,
            host_deps_root: None,
            add_deps_lib_to_library_path: true,
            host_hosts_file: None,
            host_shm_dir: None,
            enable_weaker_nested_sandbox: false,
            unset_env: Vec::new(),
            net_bridges: None,
            hide_programs: Vec::new(),
        }
    }

    pub fn with_net_bridges(mut self, bridges: Vec<NetBridge>) -> Self {
        self.net_bridges = Some(bridges);
        self
    }

    /// Translate a host path to its guest (sandbox) equivalent.
    ///
    /// Walks the known host→guest bind-mount pairs and strips/replaces the
    /// matching prefix. Falls back to the original path if no mapping matches,
    /// so callers never need to special-case the non-sandbox scenario.
    pub fn host_to_guest(&self, host_path: &Path) -> PathBuf {
        let agent_dir = self.host_home_dir.parent().unwrap_or(&self.host_home_dir);
        let host_log_dir = agent_dir.join("logs");
        let host_runtime_dir = agent_dir.join("runtime");
        let host_workspace_dir = agent_dir.join("workspace");

        let pairs: &[(&Path, &Path)] = &[
            (&self.host_home_dir, &self.guest_home),
            (host_workspace_dir.as_path(), &self.guest_workspace),
            (host_log_dir.as_path(), &self.guest_logs),
            (host_runtime_dir.as_path(), &self.guest_runtime),
        ];
        // Prompt file is a special 1:1 mapping outside the standard dirs.
        if host_path == agent_dir.join("runtime/system_prompt.md") {
            return self.guest_prompt.clone();
        }
        for (host_root, guest_root) in pairs {
            if let Ok(rel) = host_path.strip_prefix(host_root) {
                return if rel.as_os_str().is_empty() {
                    guest_root.to_path_buf()
                } else {
                    guest_root.join(rel)
                };
            }
        }
        host_path.to_path_buf()
    }

    pub fn with_claude_plugin(mut self) -> Self {
        self.guest_plugin = Some(PathBuf::from("/plugin"));
        self
    }

    pub fn with_hosts_file(mut self, path: PathBuf) -> Self {
        self.host_hosts_file = Some(path);
        self
    }

    pub fn with_host_deps_root(mut self, root: Option<PathBuf>) -> Self {
        self.host_deps_root = root;
        self
    }

    pub fn with_add_deps_lib_to_library_path(mut self, enabled: bool) -> Self {
        self.add_deps_lib_to_library_path = enabled;
        self
    }

    pub fn with_enable_weaker_nested_sandbox(mut self, enabled: bool) -> Self {
        self.enable_weaker_nested_sandbox = enabled;
        self
    }

    pub fn with_shm_dir(mut self, path: PathBuf) -> Self {
        self.host_shm_dir = Some(path);
        self
    }

    pub fn with_unset_env(mut self, name: impl Into<String>) -> Self {
        self.unset_env.push(name.into());
        self
    }

    pub fn with_codex_binary_root(mut self, host_root: PathBuf, rel: PathBuf) -> Self {
        self.host_binary_root = Some(host_root);
        self.guest_binary_root = Some(PathBuf::from("/sandbox-codex"));
        self.guest_binary_rel = Some(rel);
        self
    }

    pub fn with_node_root(mut self, host_root: PathBuf) -> Self {
        self.host_node_root = Some(host_root);
        self.guest_node_root = Some(PathBuf::from("/sandbox-node"));
        self
    }

    pub fn codex_guest_program(&self) -> PathBuf {
        match (&self.guest_binary_root, &self.guest_binary_rel) {
            (Some(root), Some(rel)) => root.join(rel.strip_prefix("/").unwrap_or(rel)),
            _ => self.guest_binary.clone(),
        }
    }

    pub fn codex_guest_node(&self) -> Option<PathBuf> {
        self.guest_node_root
            .as_ref()
            .map(|root| root.join("bin/node"))
    }
}

pub fn validate_runtime(layout: &SandboxLayout, needs_socat: bool) -> Result<(), String> {
    require_program("bwrap", "bubblewrap (bwrap)")?;
    if needs_socat {
        require_program("socat", "socat for Claude Code's native Linux sandbox")?;
    }
    if let Some(root) = &layout.host_deps_root {
        if !root.join("bin").is_dir() {
            return Err(format!(
                "sandbox.deps_root must contain a bin directory: {}",
                root.display()
            ));
        }
    }
    Ok(())
}

pub fn detect_sandbox_deps_root(
    config: &SandboxConfig,
    needs_socat: bool,
) -> Result<Option<PathBuf>, String> {
    if config.deps_root.is_some() {
        return Ok(config.deps_root.clone());
    }
    if let Some(root) = std::env::var_os("SANDBOX_DEPS_ROOT").map(PathBuf::from) {
        return Ok(Some(root));
    }
    if !needs_socat {
        return Ok(None);
    }

    let bwrap = find_program("bwrap", "bubblewrap (bwrap)")?;
    let socat = find_program("socat", "socat for Claude Code's native Linux sandbox")?;
    let bwrap_root = program_prefix_root(&bwrap)?;
    let socat_root = program_prefix_root(&socat)?;
    let bwrap_non_system = non_system_prefix(&bwrap_root);
    let socat_non_system = non_system_prefix(&socat_root);

    match (bwrap_non_system, socat_non_system) {
        (false, false) => Ok(None),
        (true, false) => Ok(Some(bwrap_root)),
        (false, true) => Ok(Some(socat_root)),
        (true, true) if bwrap_root == socat_root => Ok(Some(bwrap_root)),
        (true, true) => Err(format!(
            "bwrap and socat are in different non-system prefixes; set sandbox.deps_root in world.toml explicitly.\n  bwrap root: {}\n  socat root: {}",
            bwrap_root.display(),
            socat_root.display()
        )),
    }
}

/// Claude Code user configuration in the sandbox home, read-only.
pub const CLAUDE_READ_ONLY: &[&str] = &[
    ".claude/settings.json",
    ".claude/skills",
    ".claude/agents",
    ".claude/commands",
];

/// Files in the agent's runtime dir that launch or configure it.
pub const RUNTIME_CONTROL_FILES: &[&str] = &[
    "start.sh",
    "send.sh",
    "claude_sandbox_settings.json",
    "sandbox_hosts",
    "system_prompt.md",
    "claude-plugin",
    "active.json",
];

pub fn wrap_command(
    ctx: &AgentContext,
    layout: &SandboxLayout,
    command: CommandSpec,
    needs_socat: bool,
) -> Result<CommandSpec, String> {
    validate_runtime(layout, needs_socat)?;
    Ok(wrap_command_unchecked(ctx, layout, command))
}

fn wrap_command_unchecked(
    ctx: &AgentContext,
    layout: &SandboxLayout,
    command: CommandSpec,
) -> CommandSpec {
    let mut args = Vec::new();
    if layout.enable_weaker_nested_sandbox {
        // Docker-compatible nested bwrap needs a user namespace. Keep the
        // guest uid non-root so Claude Code can run bypass-permissions mode.
        args.extend([
            "--unshare-user".to_string(),
            "--uid".to_string(),
            "1000".to_string(),
            "--gid".to_string(),
            "1000".to_string(),
        ]);
    }
    args.extend([
        "--ro-bind".to_string(),
        "/usr".to_string(),
        "/usr".to_string(),
        "--ro-bind".to_string(),
        "/bin".to_string(),
        "/bin".to_string(),
        "--ro-bind".to_string(),
        "/sbin".to_string(),
        "/sbin".to_string(),
        "--ro-bind".to_string(),
        "/lib".to_string(),
        "/lib".to_string(),
        "--ro-bind".to_string(),
        "/etc".to_string(),
        "/etc".to_string(),
        "--ro-bind".to_string(),
        "/run".to_string(),
        "/run".to_string(),
        "--proc".to_string(),
        "/proc".to_string(),
        "--dev".to_string(),
        "/dev".to_string(),
        "--tmpfs".to_string(),
        "/tmp".to_string(),
        "--dir".to_string(),
        "/sandbox-bin".to_string(),
    ]);
    // e.g. the clawblox CLI, visible under /usr.
    for program in &layout.hide_programs {
        args.extend([
            "--ro-bind".to_string(),
            "/dev/null".to_string(),
            program.display().to_string(),
        ]);
    }
    if let Some(shm_dir) = &layout.host_shm_dir {
        args.extend([
            "--bind".to_string(),
            shm_dir.display().to_string(),
            "/dev/shm".to_string(),
        ]);
    }

    if layout.guest_plugin.is_some() {
        args.extend(["--dir".to_string(), "/plugin".to_string()]);
    }
    args.extend([
        "--dir".to_string(),
        "/logs".to_string(),
        "--dir".to_string(),
        "/runtime".to_string(),
        "--dir".to_string(),
        "/home".to_string(),
        "--bind".to_string(),
        ctx.paths.workspace_dir.display().to_string(),
        layout.guest_workspace.display().to_string(),
        "--bind".to_string(),
        layout.host_home_dir.display().to_string(),
        layout.guest_home.display().to_string(),
        "--bind".to_string(),
        ctx.paths
            .log_file
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .display()
            .to_string(),
        layout.guest_logs.display().to_string(),
        "--bind".to_string(),
        ctx.paths.runtime_dir.display().to_string(),
        layout.guest_runtime.display().to_string(),
    ]);
    // /runtime stays writable for hook state, but the files
    // that launch or configure the agent are read-only over it. `-try` binds
    // whatever exists when bwrap starts (start.sh always does: it runs bwrap).
    // A bound file is also a mount point, so the agent can't unlink or replace it.
    for name in RUNTIME_CONTROL_FILES {
        args.extend([
            "--ro-bind-try".to_string(),
            ctx.paths.runtime_dir.join(name).display().to_string(),
            layout.guest_runtime.join(name).display().to_string(),
        ]);
    }
    // The host appends to the agent log from outside the sandbox (tmux pipe, start.sh).
    if let Some(name) = ctx.paths.log_file.file_name() {
        args.extend([
            "--ro-bind-try".to_string(),
            ctx.paths.log_file.display().to_string(),
            layout.guest_logs.join(name).display().to_string(),
        ]);
    }
    // Claude Code's user configuration is read-only. ~/.claude
    // is its own mount point, so it can't be renamed aside and replaced; inside it, the
    // settings and the user skills and agents (which can declare hooks) are read-only.
    args.extend([
        "--bind-try".to_string(),
        layout.host_home_dir.join(".claude").display().to_string(),
        layout.guest_home.join(".claude").display().to_string(),
    ]);
    for rel in CLAUDE_READ_ONLY {
        args.extend([
            "--ro-bind-try".to_string(),
            layout.host_home_dir.join(rel).display().to_string(),
            layout.guest_home.join(rel).display().to_string(),
        ]);
    }

    if layout.guest_plugin.is_some() {
        args.extend([
            "--ro-bind".to_string(),
            ctx.paths.prompt_file.display().to_string(),
            layout.guest_prompt.display().to_string(),
        ]);
        if let Some(guest_plugin) = &layout.guest_plugin {
            args.extend([
                "--ro-bind".to_string(),
                ctx.paths
                    .runtime_dir
                    .join("claude-plugin")
                    .display()
                    .to_string(),
                guest_plugin.display().to_string(),
            ]);
        }
    }

    match (&layout.host_binary_root, &layout.guest_binary_root) {
        (Some(host_root), Some(guest_root)) => args.extend([
            "--ro-bind".to_string(),
            host_root.display().to_string(),
            guest_root.display().to_string(),
        ]),
        _ => args.extend([
            "--ro-bind".to_string(),
            layout.host_binary.display().to_string(),
            layout.guest_binary.display().to_string(),
        ]),
    }
    if let (Some(host_root), Some(guest_root)) = (&layout.host_node_root, &layout.guest_node_root) {
        args.extend([
            "--ro-bind".to_string(),
            host_root.display().to_string(),
            guest_root.display().to_string(),
        ]);
    }

    args.extend([
        "--setenv".to_string(),
        "HOME".to_string(),
        layout.guest_home.display().to_string(),
        "--setenv".to_string(),
        "USER".to_string(),
        "agent".to_string(),
        "--setenv".to_string(),
        "LOGNAME".to_string(),
        "agent".to_string(),
        "--setenv".to_string(),
        "SHELL".to_string(),
        "/bin/bash".to_string(),
        "--setenv".to_string(),
        "PATH".to_string(),
        sandbox_path(ctx, layout),
        "--chdir".to_string(),
        layout.guest_workspace.display().to_string(),
        "--die-with-parent".to_string(),
        // nothing an agent starts outlives its session.
        "--unshare-pid".to_string(),
    ]);
    if ctx.paths.workspace_dir.join(".venv").is_dir() {
        args.extend([
            "--setenv".to_string(),
            "VIRTUAL_ENV".to_string(),
            layout.guest_workspace.join(".venv").display().to_string(),
            "--setenv".to_string(),
            "PYTHONNOUSERSITE".to_string(),
            "1".to_string(),
        ]);
    }
    if layout.guest_plugin.is_some() {
        for name in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "CLAUDE_CONFIG_DIR",
            "CLAUDE_CODE_EXECPATH",
            "CLAUDE_CODE_ENTRYPOINT",
            "CLAUDECODE",
        ] {
            args.extend(["--unsetenv".to_string(), name.to_string()]);
        }
    }
    for name in &layout.unset_env {
        args.extend(["--unsetenv".to_string(), name.clone()]);
    }
    // the world's operator credential never enters the sandbox.
    args.extend(["--unsetenv".to_string(), "WORLD_OPERATOR_TOKEN".to_string()]);

    if let Some(root) = &layout.host_deps_root {
        args.extend([
            "--ro-bind".to_string(),
            root.display().to_string(),
            "/sandbox-deps".to_string(),
        ]);
        if root.is_absolute() && root != Path::new("/sandbox-deps") {
            args.extend([
                "--ro-bind".to_string(),
                root.display().to_string(),
                root.display().to_string(),
            ]);
        }
        if layout.add_deps_lib_to_library_path && root.join("lib").is_dir() {
            args.extend([
                "--setenv".to_string(),
                "LD_LIBRARY_PATH".to_string(),
                "/sandbox-deps/lib".to_string(),
            ]);
        } else {
            args.extend(["--unsetenv".to_string(), "LD_LIBRARY_PATH".to_string()]);
        }
    }
    if Path::new("/lib64").is_dir() {
        args.extend([
            "--ro-bind".to_string(),
            "/lib64".to_string(),
            "/lib64".to_string(),
        ]);
    }
    if let Some(hosts_file) = &layout.host_hosts_file {
        if hosts_file.is_file() {
            args.extend([
                "--ro-bind".to_string(),
                hosts_file.display().to_string(),
                "/etc/hosts".to_string(),
            ]);
        }
    }
    if Path::new("/mnt/wsl/resolv.conf").is_file() {
        args.extend([
            "--ro-bind".to_string(),
            "/mnt/wsl/resolv.conf".to_string(),
            "/mnt/wsl/resolv.conf".to_string(),
        ]);
    }

    let Some(bridges) = layout.net_bridges.as_ref() else {
        args.push("--".to_string());
        args.push(command.program.display().to_string());
        args.extend(command.args);
        return CommandSpec {
            program: PathBuf::from("bwrap"),
            args,
            env: command.env,
            cwd: ctx.paths.workspace_dir.clone(),
        };
    };

    // a private network namespace. Outside, one socat per
    // endpoint listens on a unix socket in a dir only the host can write; inside, one
    // socat per endpoint listens on 127.0.0.1 and forwards to that socket.
    let net_dir = net_bridge_dir(ctx);
    let socat = if Path::new("/usr/bin/socat").is_file() {
        "/usr/bin/socat"
    } else {
        "socat"
    };
    args.extend([
        "--unshare-net".to_string(),
        "--ro-bind".to_string(),
        net_dir.display().to_string(),
        "/net-bridge".to_string(),
    ]);
    if let Some(world) = bridges.iter().find(|b| b.name == "world") {
        args.extend([
            "--setenv".to_string(),
            "WORLD_INTERNAL_BASE_URL".to_string(),
            format!("http://world:{}", world.port),
        ]);
    }
    let mut inner = String::new();
    for b in bridges {
        inner.push_str(&format!(
            "{socat} TCP-LISTEN:{port},bind=127.0.0.1,fork,reuseaddr UNIX-CONNECT:/net-bridge/{name}.sock & ",
            port = b.port,
            name = b.name
        ));
    }
    // Fail closed: start the agent only once every bridge is listening on loopback.
    let ports: Vec<String> = bridges.iter().map(|b| format!("{:04X}", b.port)).collect();
    inner.push_str(&format!(
        "for p in {}; do n=0; until grep -q \"0100007F:$p 00000000:0000 0A\" /proc/net/tcp; do n=$((n+1)); [ $n -gt 50 ] && {{ echo \"clawblox: sandbox bridge $p not listening\" >&2; exit 1; }}; sleep 0.1; done; done; ",
        ports.join(" ")
    ));
    inner.push_str("exec \"$@\"");
    args.extend([
        "--".to_string(),
        "/bin/sh".to_string(),
        "-c".to_string(),
        inner,
        "clawblox-net".to_string(),
        command.program.display().to_string(),
    ]);
    args.extend(command.args);

    let quoted_dir = format!("'{}'", net_dir.display().to_string().replace('\'', "'\\''"));
    let mut outer = format!(
        "set -u; d={quoted_dir}; mkdir -p \"$d\" && chmod 700 \"$d\" || exit 1; rm -f \"$d\"/*.sock; pids=''; trap 'kill $pids 2>/dev/null' EXIT HUP; "
    );
    for b in bridges {
        outer.push_str(&format!(
            "{socat} UNIX-LISTEN:\"$d/{name}.sock\",fork,mode=600,unlink-close=0 TCP:{host}:{port} & pids=\"$pids $!\"; ",
            name = b.name,
            host = b.host,
            port = b.port
        ));
    }
    let names: Vec<&str> = bridges.iter().map(|b| b.name.as_str()).collect();
    outer.push_str(&format!(
        "for s in {}; do n=0; until [ -S \"$d/$s.sock\" ]; do n=$((n+1)); [ $n -gt 50 ] && {{ echo \"clawblox: bridge $s did not start\" >&2; exit 1; }}; sleep 0.1; done; done; \"$@\"",
        names.join(" ")
    ));
    let mut outer_args = vec![
        "-c".to_string(),
        outer,
        "clawblox-net".to_string(),
        "bwrap".to_string(),
    ];
    outer_args.extend(args);
    CommandSpec {
        program: PathBuf::from("/bin/sh"),
        args: outer_args,
        env: command.env,
        cwd: ctx.paths.workspace_dir.clone(),
    }
}

fn sandbox_path(ctx: &AgentContext, layout: &SandboxLayout) -> String {
    let mut entries = Vec::new();
    if ctx.paths.workspace_dir.join(".venv/bin").is_dir() {
        entries.push("/workspace/.venv/bin");
    }
    if layout.host_deps_root.is_some() {
        entries.extend([
            "/home/agent/.local/bin",
            "/sandbox-bin",
            "/sandbox-deps/bin",
            "/usr/bin",
            "/bin",
        ]);
    } else {
        entries.extend(["/home/agent/.local/bin", "/sandbox-bin", "/usr/bin", "/bin"]);
    }
    entries.join(":")
}

fn require_program(name: &str, label: &str) -> Result<(), String> {
    find_program(name, label).map(|_| ())
}

fn find_program(name: &str, label: &str) -> Result<PathBuf, String> {
    let Some(paths) = std::env::var_os("PATH") else {
        return Err(format!("{label} was not found in PATH"));
    };
    for dir in std::env::split_paths(&paths) {
        if dir.join(name).exists() {
            return Ok(dir.join(name));
        }
    }
    Err(format!("{label} was not found in PATH"))
}

fn program_prefix_root(path: &Path) -> Result<PathBuf, String> {
    let path = std::fs::canonicalize(path)
        .map_err(|err| format!("failed to canonicalize {}: {err}", path.display()))?;
    let bin_dir = path
        .parent()
        .ok_or_else(|| format!("failed to find parent directory for {}", path.display()))?;
    Ok(bin_dir
        .parent()
        .unwrap_or_else(|| Path::new("/"))
        .to_path_buf())
}

fn non_system_prefix(root: &Path) -> bool {
    root != Path::new("/") && root != Path::new("/usr")
}

fn agent_dir_from_context(ctx: &AgentContext) -> PathBuf {
    ctx.paths
        .runtime_dir
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentPaths, AgentSettings};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn claude_layout(ctx: &AgentContext) -> SandboxLayout {
        SandboxLayout::for_agent(ctx, PathBuf::from("/usr/local/bin/claude"), "claude")
            .with_claude_plugin()
            .with_hosts_file(ctx.paths.runtime_dir.join("sandbox_hosts"))
    }

    #[test]
    fn agent_sandbox_has_its_own_pid_namespace_and_read_only_claude_settings() {
        // Own pid namespace; read-only Claude Code settings.
        let root = test_root();
        let ctx = test_context(root.clone());
        let layout = claude_layout(&ctx);
        let wrapped = wrap_command_unchecked(
            &ctx,
            &layout,
            CommandSpec::new("/sandbox-bin/claude", "/workspace"),
        );
        assert!(wrapped.args.contains(&"--unshare-pid".to_string()));
        assert!(has_arg_pair(&wrapped.args, "--unsetenv", "WORLD_OPERATOR_TOKEN"));
        let host = layout.host_home_dir.join(".claude/settings.json").display().to_string();
        let home_rw = wrapped
            .args
            .windows(3)
            .position(|w| w[0] == "--bind" && w[2] == "/home/agent")
            .expect("home bind");
        let settings_ro = wrapped
            .args
            .windows(3)
            .position(|w| {
                w[0] == "--ro-bind-try" && w[1] == host && w[2] == "/home/agent/.claude/settings.json"
            })
            .expect("no read-only bind for the Claude settings");
        assert!(settings_ro > home_rw);
        let claude_dir = wrapped
            .args
            .windows(3)
            .position(|w| w[0] == "--bind-try" && w[2] == "/home/agent/.claude")
            .expect("~/.claude must be its own mount point");
        assert!(claude_dir > home_rw && claude_dir < settings_ro);
        for rel in [".claude/skills", ".claude/agents", ".claude/commands"] {
            assert!(wrapped.args.windows(3).any(|w| w[0] == "--ro-bind-try"
                && w[2] == format!("/home/agent/{rel}")));
        }
        // Without network isolation configured, the command is plain bwrap.
        assert_eq!(wrapped.program, PathBuf::from("bwrap"));
        assert!(!wrapped.args.contains(&"--unshare-net".to_string()));
    }

    #[test]
    fn network_isolation_bridges_only_the_configured_endpoints() {
        // no network inside the sandbox except the
        // meter and the world proxy, each bridged through a unix socket.
        let root = test_root();
        let ctx = test_context(root.clone());
        let layout = claude_layout(&ctx).with_net_bridges(vec![
            NetBridge::new("meter", "meter", 8411),
            NetBridge::new("world", "127.0.0.1", 43210),
        ]);
        let wrapped = wrap_command_unchecked(
            &ctx,
            &layout,
            CommandSpec::new("/sandbox-bin/claude", "/workspace").arg("Begin"),
        );
        assert_eq!(wrapped.program, PathBuf::from("/bin/sh"));
        let outer = &wrapped.args[1];
        assert!(outer.contains("UNIX-LISTEN:"));
        assert!(outer.contains("meter.sock"));
        assert!(outer.contains("TCP:meter:8411"));
        assert!(outer.contains("TCP:127.0.0.1:43210"));
        assert!(outer.contains("unlink-close=0"));
        assert!(outer.contains("did not start") && outer.contains("exit 1"));
        assert!(!outer.contains("TERM"));
        let bwrap_at = wrapped.args.iter().position(|a| a == "bwrap").expect("bwrap");
        let bwrap_args = &wrapped.args[bwrap_at + 1..];
        assert!(bwrap_args.contains(&"--unshare-net".to_string()));
        let net_dir = net_bridge_dir(&ctx).display().to_string();
        assert!(!net_dir.starts_with(&root.display().to_string()));
        assert!(bwrap_args
            .windows(3)
            .any(|w| w[0] == "--ro-bind" && w[1] == net_dir && w[2] == "/net-bridge"));
        assert!(bwrap_args.windows(3).any(|w| w[0] == "--setenv"
            && w[1] == "WORLD_INTERNAL_BASE_URL"
            && w[2] == "http://world:43210"));
        let dash = bwrap_args.iter().position(|a| a == "--").expect("--");
        assert_eq!(bwrap_args[dash + 1], "/bin/sh");
        let inner = &bwrap_args[dash + 3];
        assert!(inner.contains("TCP-LISTEN:8411,bind=127.0.0.1"));
        assert!(inner.contains("UNIX-CONNECT:/net-bridge/meter.sock"));
        assert!(inner.contains("TCP-LISTEN:43210,bind=127.0.0.1"));
        assert!(inner.contains("exec \"$@\""));
        // 8411 = 0x20DB, 43210 = 0xA8CA: each bridge port must be listening on loopback.
        assert!(inner.contains("for p in 20DB A8CA;"));
        assert!(inner.contains("0100007F:$p 00000000:0000 0A"));
        assert!(inner.contains("not listening") && inner.contains("exit 1"));
        assert_eq!(bwrap_args.last().unwrap(), "Begin");
        assert!(bwrap_args.contains(&"/sandbox-bin/claude".to_string()));
    }

    #[test]
    fn hidden_programs_are_masked_inside_the_sandbox() {
        // the clawblox CLI is not usable inside.
        let root = test_root();
        let ctx = test_context(root.clone());
        let mut layout = claude_layout(&ctx);
        layout.hide_programs = vec![PathBuf::from("/usr/local/bin/clawblox")];
        let wrapped = wrap_command_unchecked(
            &ctx,
            &layout,
            CommandSpec::new("/sandbox-bin/claude", "/workspace"),
        );
        let usr = wrapped
            .args
            .windows(3)
            .position(|w| w[0] == "--ro-bind" && w[1] == "/usr" && w[2] == "/usr")
            .expect("/usr bind");
        let mask = wrapped
            .args
            .windows(3)
            .position(|w| w[0] == "--ro-bind" && w[1] == "/dev/null" && w[2] == "/usr/local/bin/clawblox")
            .expect("clawblox masked");
        assert!(mask > usr);
        let plain = wrap_command_unchecked(
            &ctx,
            &claude_layout(&ctx),
            CommandSpec::new("/sandbox-bin/claude", "/workspace"),
        );
        assert!(!plain.args.iter().any(|a| a == "/usr/local/bin/clawblox"));
    }

    #[test]
    fn parses_net_bridge_config() {
        assert_eq!(
            parse_net_bridges("meter=meter:8411"),
            Ok(vec![NetBridge::new("meter", "meter", 8411)])
        );
        assert!(parse_net_bridges("meter=meter").is_err());
        assert!(parse_net_bridges("bad name=meter:1").is_err());
        assert!(parse_net_bridges("m=h:notaport").is_err());
    }

    #[test]
    fn runtime_control_files_are_read_only_over_the_writable_runtime_bind() {
        // the agent must not be able to rewrite the files
        // that run or configure it (start.sh is re-executed outside the sandbox by
        // the recovery watchdog; the settings hold the network allowlist).
        let root = test_root();
        let ctx = test_context(root.clone());
        let layout =
            SandboxLayout::for_agent(&ctx, PathBuf::from("/usr/local/bin/claude"), "claude")
                .with_claude_plugin()
                .with_hosts_file(ctx.paths.runtime_dir.join("sandbox_hosts"));
        let wrapped = wrap_command_unchecked(
            &ctx,
            &layout,
            CommandSpec::new("/sandbox-bin/claude", "/workspace"),
        );
        let runtime = ctx.paths.runtime_dir.display().to_string();
        let rw_index = wrapped
            .args
            .windows(3)
            .position(|w| w[0] == "--bind" && w[1] == runtime && w[2] == "/runtime")
            .expect("runtime bind");
        for name in RUNTIME_CONTROL_FILES {
            let host = ctx.paths.runtime_dir.join(name).display().to_string();
            let guest = format!("/runtime/{name}");
            let ro_index = wrapped
                .args
                .windows(3)
                .position(|w| w[0] == "--ro-bind-try" && w[1] == host && w[2] == guest)
                .unwrap_or_else(|| panic!("no read-only bind for {name}"));
            assert!(ro_index > rw_index, "{name} must be bound after /runtime");
        }
        for name in ["start.sh", "claude_sandbox_settings.json", "sandbox_hosts", "claude-plugin", "active.json"] {
            assert!(RUNTIME_CONTROL_FILES.contains(&name), "{name} missing");
        }
        // The host appends to the agent log from outside the sandbox, so the agent
        // must not be able to swap it for a symlink either.
        let log = ctx.paths.log_file.display().to_string();
        let guest_log = format!(
            "/logs/{}",
            ctx.paths.log_file.file_name().unwrap().to_string_lossy()
        );
        let logs_rw = wrapped
            .args
            .windows(2)
            .position(|w| w[0] == "--bind" && w[1] == ctx.paths.log_file.parent().unwrap().display().to_string())
            .expect("logs bind");
        let log_ro = wrapped
            .args
            .windows(3)
            .position(|w| w[0] == "--ro-bind-try" && w[1] == log && w[2] == guest_log)
            .expect("no read-only bind for the agent log");
        assert!(log_ro > logs_rw);
    }

    #[test]
    fn wraps_claude_command_with_expected_mounts_and_guest_paths() {
        let root = test_root();
        let ctx = test_context(root.clone());
        let layout =
            SandboxLayout::for_agent(&ctx, PathBuf::from("/usr/local/bin/claude"), "claude")
                .with_claude_plugin()
                .with_hosts_file(ctx.paths.runtime_dir.join("sandbox_hosts"));
        let command = CommandSpec::new("/sandbox-bin/claude", "/workspace")
            .arg("--plugin-dir")
            .arg("/plugin")
            .arg("Begin")
            .env("WORLD_BASE_URL", "http://world");

        let wrapped = wrap_command_unchecked(&ctx, &layout, command);

        assert_eq!(wrapped.program, PathBuf::from("bwrap"));
        assert!(has_arg_triple(
            &wrapped.args,
            "--bind",
            &ctx.paths.workspace_dir.display().to_string(),
            "/workspace"
        ));
        assert!(has_arg_triple(
            &wrapped.args,
            "--bind",
            &root.join("sandbox-home").display().to_string(),
            "/home/agent"
        ));
        assert!(has_arg_triple(
            &wrapped.args,
            "--ro-bind",
            &ctx.paths.prompt_file.display().to_string(),
            "/tmp/system_prompt.md"
        ));
        assert!(has_arg_triple(
            &wrapped.args,
            "--ro-bind",
            &ctx.paths
                .runtime_dir
                .join("claude-plugin")
                .display()
                .to_string(),
            "/plugin"
        ));
        assert!(wrapped
            .args
            .windows(2)
            .any(|window| window == ["--", "/sandbox-bin/claude"]));
        assert!(has_arg_pair(
            &wrapped.args,
            "--unsetenv",
            "CLAUDE_CONFIG_DIR"
        ));
        assert!(has_arg_pair(
            &wrapped.args,
            "--unsetenv",
            "ANTHROPIC_API_KEY"
        ));
    }

    #[test]
    fn wraps_codex_package_root_when_binary_is_node_package() {
        let root = test_root();
        let node_root = root.join("node");
        let ctx = test_context(root.clone());
        let layout = SandboxLayout::for_agent(&ctx, root.join("node_modules/.bin/codex"), "codex")
            .with_unset_env("CODEX_HOME")
            .with_codex_binary_root(root.clone(), PathBuf::from("/node_modules/.bin/codex"))
            .with_node_root(node_root.clone());
        let command = CommandSpec::new(layout.codex_guest_node().unwrap(), "/workspace")
            .arg(layout.codex_guest_program().display().to_string())
            .arg("--no-alt-screen");

        let wrapped = wrap_command_unchecked(&ctx, &layout, command);

        assert!(has_arg_triple(
            &wrapped.args,
            "--ro-bind",
            &root.display().to_string(),
            "/sandbox-codex"
        ));
        assert!(has_arg_triple(
            &wrapped.args,
            "--ro-bind",
            &node_root.display().to_string(),
            "/sandbox-node"
        ));
        assert!(wrapped
            .args
            .windows(2)
            .any(|window| window == ["--", "/sandbox-node/bin/node"]));
        assert!(wrapped.args.windows(2).any(|window| window
            == [
                "/sandbox-node/bin/node",
                "/sandbox-codex/node_modules/.bin/codex"
            ]));
        assert!(has_arg_pair(&wrapped.args, "--unsetenv", "CODEX_HOME"));
    }

    #[test]
    fn exposes_workspace_venv_inside_sandbox() {
        let root = test_root();
        let ctx = test_context(root.clone());
        std::fs::create_dir_all(ctx.paths.workspace_dir.join(".venv/bin")).unwrap();
        let layout = SandboxLayout::for_agent(&ctx, root.join("node_modules/.bin/codex"), "codex");
        let command = CommandSpec::new("/sandbox-bin/codex", "/workspace");

        let wrapped = wrap_command_unchecked(&ctx, &layout, command);

        assert!(has_arg_pair(&wrapped.args, "--setenv", "PATH"));
        assert!(wrapped.args.windows(3).any(|window| window
            == [
                "--setenv",
                "PATH",
                "/workspace/.venv/bin:/home/agent/.local/bin:/sandbox-bin:/usr/bin:/bin"
            ]));
        assert!(wrapped
            .args
            .windows(3)
            .any(|window| window == ["--setenv", "VIRTUAL_ENV", "/workspace/.venv"]));
        assert!(wrapped
            .args
            .windows(3)
            .any(|window| window == ["--setenv", "PYTHONNOUSERSITE", "1"]));
    }

    #[test]
    fn weaker_nested_sandbox_adds_user_namespace_flags() {
        let root = test_root();
        let ctx = test_context(root.clone());
        let layout = SandboxLayout::for_agent(&ctx, root.join("bin/claude"), "claude")
            .with_enable_weaker_nested_sandbox(true);
        let command = CommandSpec::new("/sandbox-bin/claude", "/workspace");

        let wrapped = wrap_command_unchecked(&ctx, &layout, command);

        assert!(wrapped
            .args
            .windows(5)
            .any(|window| { window == ["--unshare-user", "--uid", "1000", "--gid", "1000"] }));
    }

    #[test]
    fn deps_root_is_mounted_and_added_to_path() {
        let root = test_root();
        let deps_root = root.join("deps");
        std::fs::create_dir_all(deps_root.join("bin")).unwrap();
        std::fs::create_dir_all(deps_root.join("lib")).unwrap();
        let ctx = test_context(root.clone());
        let layout = SandboxLayout::for_agent(&ctx, root.join("bin/codex"), "codex")
            .with_host_deps_root(Some(deps_root.clone()));
        let command = CommandSpec::new("/sandbox-bin/codex", "/workspace");

        let wrapped = wrap_command_unchecked(&ctx, &layout, command);

        assert!(has_arg_triple(
            &wrapped.args,
            "--ro-bind",
            &deps_root.display().to_string(),
            "/sandbox-deps"
        ));
        assert!(has_arg_triple(
            &wrapped.args,
            "--ro-bind",
            &deps_root.display().to_string(),
            &deps_root.display().to_string()
        ));
        assert!(wrapped.args.windows(3).any(|window| window
            == [
                "--setenv",
                "PATH",
                "/home/agent/.local/bin:/sandbox-bin:/sandbox-deps/bin:/usr/bin:/bin"
            ]));
        assert!(has_arg_triple(
            &wrapped.args,
            "--setenv",
            "LD_LIBRARY_PATH",
            "/sandbox-deps/lib"
        ));
    }

    #[test]
    fn deps_root_can_skip_ld_library_path() {
        let root = test_root();
        let deps_root = root.join("deps");
        std::fs::create_dir_all(deps_root.join("bin")).unwrap();
        std::fs::create_dir_all(deps_root.join("lib")).unwrap();
        let ctx = test_context(root.clone());
        let layout = SandboxLayout::for_agent(&ctx, PathBuf::from("/bin/echo"), "codex")
            .with_host_deps_root(Some(deps_root))
            .with_add_deps_lib_to_library_path(false);
        let command = CommandSpec::new("/sandbox-bin/codex", "/workspace");

        let wrapped = wrap_command_unchecked(&ctx, &layout, command);

        assert!(!has_arg_pair(&wrapped.args, "--setenv", "LD_LIBRARY_PATH"));
        assert!(has_arg_pair(&wrapped.args, "--unsetenv", "LD_LIBRARY_PATH"));
    }

    fn has_arg_triple(args: &[String], flag: &str, host: &str, guest: &str) -> bool {
        args.windows(3).any(|window| window == [flag, host, guest])
    }

    fn has_arg_pair(args: &[String], flag: &str, value: &str) -> bool {
        args.windows(2).any(|window| window == [flag, value])
    }

    fn test_context(root: PathBuf) -> AgentContext {
        AgentContext {
            paths: AgentPaths {
                workspace_dir: root.join("workspace"),
                prompt_file: root.join("runtime/system_prompt.md"),
                session_id_file: root.join("state/session_id.txt"),
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
                sandbox_enabled: true,
                version: None,
            },
            sandbox: None,
        }
    }

    fn test_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "clawblox-agent-sandbox-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
