use std::{
    fs::OpenOptions,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use clawblox::agent::build_agent;
use clawblox::agent_runner::{
    default_system_prompt as rust_default_system_prompt, send_agent, start_agent, stop_agent,
    AgentSendOptions, AgentStartOptions, AgentStopOptions,
};
use clawblox::config::{SandboxConfig, WorldConfig};
use clawblox::manifest::{
    manifest_path_for_state_root, record_agent_connected, record_checkpoint,
    record_run as record_manifest_run, record_world_started, refresh_agent_from_dir,
    AgentConnected, RunRecorded, WorldStarted,
};
use clawblox::world_capability_proxy::{
    start_world_capability_proxy, WorldCapabilityProxy, WorldCapabilityProxyConfig,
};
use clawblox::world_runner::{
    spawn_external_world, write_external_world_command_file, ExternalWorldCommand,
};
use clawblox::world_source;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

#[pyclass(name = "Agent")]
#[derive(Clone, Debug)]
struct PyAgent {
    agent: String,
    name: String,
    run_name: String,
    binary: Option<PathBuf>,
    version: Option<String>,
    dir: PathBuf,
    model: String,
    permission_mode: String,
    auto_continue_prompt: String,
    tmux: Option<String>,
    bare: bool,
    extra_args: Vec<String>,
    sandbox: bool,
    claude_native_sandbox: bool,
    sandbox_config: Arc<Mutex<SandboxConfig>>,
    run_tmux: Arc<Mutex<Option<AgentRunTmux>>>,
    world_env: Arc<Mutex<Vec<(String, String)>>>,
}

#[derive(Clone, Debug)]
struct AgentRunTmux {
    session: String,
    window: String,
}

#[pyclass(name = "AgentStartResult")]
#[derive(Clone, Debug)]
struct PyAgentStartResult {
    tmux_session: String,
    tmux_window: Option<String>,
    tmux_pane_id: Option<String>,
    agent_dir: PathBuf,
    command_file: PathBuf,
    events_file: PathBuf,
    attach_command: String,
}

#[pyclass(name = "AgentStopResult")]
#[derive(Clone, Debug)]
struct PyAgentStopResult {
    tmux_session: Option<String>,
    agent_dir: PathBuf,
    events_file: PathBuf,
    log_file: PathBuf,
}

#[pyclass(name = "World")]
#[derive(Debug)]
struct PyWorld {
    dir: PathBuf,
    config: serde_json::Value,
    world_config: WorldConfig,
    port: Option<u16>,
    base_path: String,
    record: bool,
    record_dir: Option<PathBuf>,
    health_timeout: u64,
    port_wait: u64,
    command_file: Option<PathBuf>,
    log_file: Option<PathBuf>,
    process: Option<Child>,
    tmux_session: Option<String>,
    tmux_window: Option<String>,
    owns_tmux_session: bool,
    manifest_path: Option<PathBuf>,
    next_agent_index: u32,
    world_proxies: Vec<WorldCapabilityProxy>,
    connected_sessions: Vec<String>,
    shm_dir: Option<PathBuf>,
    resume: Option<PathBuf>,
    operator_token: Option<String>,
}

#[pyclass(name = "WorldStartResult")]
#[derive(Clone, Debug)]
struct PyWorldStartResult {
    command: Vec<String>,
    start_command: Vec<String>,
    delegated: bool,
    run_command: Option<Vec<String>>,
    env: Vec<(String, String)>,
    url: String,
    command_file: PathBuf,
    log_file: PathBuf,
    manifest_path: Option<PathBuf>,
}

impl PyAgent {
    fn effective_run_name(&self) -> &str {
        if self.run_name.is_empty() {
            &self.name
        } else {
            &self.run_name
        }
    }

    fn hidden_run_tmux(&self) -> PyResult<Option<AgentRunTmux>> {
        self.run_tmux
            .lock()
            .map(|guard| guard.clone())
            .map_err(|_| PyAgentError::new_err("agent tmux state lock was poisoned".to_string()))
    }

    fn merged_env(&self, mut env: Vec<(String, String)>) -> PyResult<Vec<(String, String)>> {
        let world_env = self
            .world_env
            .lock()
            .map_err(|_| PyAgentError::new_err("agent world env lock was poisoned".to_string()))?;
        let mut merged = world_env.clone();
        merged.extend(self.workspace_runtime_env()?);
        merged.append(&mut env);
        Ok(merged)
    }

    fn set_world_env(&self, env: Vec<(String, String)>) -> PyResult<()> {
        let mut world_env = self
            .world_env
            .lock()
            .map_err(|_| PyAgentError::new_err("agent world env lock was poisoned".to_string()))?;
        *world_env = env;
        Ok(())
    }

    fn set_sandbox_config(&self, config: SandboxConfig) -> PyResult<()> {
        let mut sandbox_config = self.sandbox_config.lock().map_err(|_| {
            PyAgentError::new_err("agent sandbox config lock was poisoned".to_string())
        })?;
        *sandbox_config = config;
        Ok(())
    }

    fn sandbox_config(&self) -> PyResult<SandboxConfig> {
        self.sandbox_config
            .lock()
            .map(|guard| guard.clone())
            .map_err(|_| {
                PyAgentError::new_err("agent sandbox config lock was poisoned".to_string())
            })
    }

    fn prepare_workspace_runtime(&self) -> PyResult<()> {
        let pyproject = self.workspace_dir().join("pyproject.toml");
        if !pyproject.is_file() {
            return Ok(());
        }
        if !command_exists("uv") {
            return Err(PyAgentError::new_err(format!(
                "workspace has pyproject.toml, but uv was not found in PATH: {}",
                pyproject.display()
            )));
        }
        if self.sandbox && command_exists("bwrap") {
            run_workspace_uv_sync_in_sandbox(
                &self.workspace_dir(),
                &self.runtime_dir(),
                &self.sandbox_config()?,
            )
        } else {
            run_workspace_uv_sync_on_host(&self.workspace_dir(), &self.runtime_dir())
        }
    }

    fn workspace_runtime_env(&self) -> PyResult<Vec<(String, String)>> {
        if !self.workspace_dir().join(".venv").is_dir() {
            return Ok(Vec::new());
        }
        if self.sandbox {
            return Ok(Vec::new());
        }
        let venv = self.workspace_dir().join(".venv");
        let path = std::env::var_os("PATH")
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default();
        Ok(vec![
            ("VIRTUAL_ENV".to_string(), venv.display().to_string()),
            (
                "PATH".to_string(),
                format!("{}:{path}", venv.join("bin").display()),
            ),
            ("PYTHONNOUSERSITE".to_string(), "1".to_string()),
        ])
    }
}

#[pymethods]
impl PyAgent {
    #[new]
    #[pyo3(signature = (*, agent, name, dir, model, run_name = None, binary = None, version = None, permission_mode = "bypassPermissions".to_string(), auto_continue_prompt = "Please don't stop. Just continue doing whatever you want to do.".to_string(), tmux = None, bare = false, extra_args = Vec::new(), sandbox = false, claude_native_sandbox = true))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        agent: String,
        name: String,
        dir: PathBuf,
        model: String,
        run_name: Option<String>,
        binary: Option<PathBuf>,
        version: Option<String>,
        permission_mode: String,
        auto_continue_prompt: String,
        tmux: Option<String>,
        bare: bool,
        extra_args: Vec<String>,
        sandbox: bool,
        claude_native_sandbox: bool,
    ) -> Self {
        Self {
            agent,
            name,
            run_name: run_name.unwrap_or_default(),
            binary,
            version,
            dir,
            model,
            permission_mode,
            auto_continue_prompt,
            tmux,
            bare,
            extra_args,
            sandbox,
            claude_native_sandbox,
            sandbox_config: Arc::new(Mutex::new(SandboxConfig::default())),
            run_tmux: Arc::new(Mutex::new(None)),
            world_env: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[getter]
    fn agent(&self) -> &str {
        &self.agent
    }

    #[getter]
    fn name(&self) -> &str {
        &self.name
    }

    #[getter]
    fn run_name(&self) -> &str {
        self.effective_run_name()
    }

    #[getter]
    fn dir(&self) -> PathBuf {
        self.dir.clone()
    }

    #[getter]
    fn workspace_dir(&self) -> PathBuf {
        self.dir.join("workspace")
    }

    #[getter]
    fn runtime_dir(&self) -> PathBuf {
        self.dir.join("runtime")
    }

    #[getter]
    fn log_dir(&self) -> PathBuf {
        self.dir.join("logs")
    }

    #[getter]
    fn events_file(&self) -> PathBuf {
        self.dir.join("events.jsonl")
    }

    #[getter]
    fn visible_workspace_dir(&self) -> String {
        if self.sandbox {
            "/workspace".to_string()
        } else {
            self.workspace_dir().display().to_string()
        }
    }

    #[pyo3(signature = (*, initial_prompt, system_prompt, env = None))]
    fn start(
        &self,
        initial_prompt: String,
        system_prompt: String,
        env: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyAgentStartResult> {
        self.prepare_workspace_runtime()?;
        let env = self.merged_env(dict_to_env(env)?)?;
        let manifest_path = manifest_path_from_env(&env);
        let sandbox_config = self.sandbox_config()?;
        let run_tmux = self.hidden_run_tmux()?;
        let tmux_session = run_tmux
            .as_ref()
            .map(|value| value.session.clone())
            .or_else(|| self.tmux.clone());
        let tmux_window = run_tmux.as_ref().map(|value| value.window.clone());
        let started = start_agent(AgentStartOptions {
            kind: self.agent.clone(),
            name: self.effective_run_name().to_string(),
            binary: self.binary.clone(),
            version: self.version.clone(),
            agent_dir: self.dir.clone(),
            model: self.model.clone(),
            permission_mode: self.permission_mode.clone(),
            initial_prompt,
            system_prompt,
            auto_continue_prompt: self.auto_continue_prompt.clone(),
            tmux_session,
            tmux_window,
            env,
            extra_args: self.extra_args.clone(),
            sandbox: self.sandbox,
            sandbox_config,
            bare: self.bare,
        })
        .map_err(PyAgentError::new_err)?;
        if let Some(manifest_path) = manifest_path {
            refresh_agent_from_dir(
                &manifest_path,
                &self.dir,
                self.effective_run_name(),
                &self.name,
                self.effective_run_name(),
                &self.agent,
                &self.model,
            )
            .map_err(PyAgentError::new_err)?;
        }

        Ok(PyAgentStartResult {
            tmux_session: started.tmux_session,
            tmux_window: started.tmux_window,
            tmux_pane_id: started.tmux_pane_id,
            agent_dir: started.agent_dir,
            command_file: started.command_file,
            events_file: started.events_file,
            attach_command: started.attach_command,
        })
    }

    #[pyo3(signature = (*, prompt, env = None))]
    fn send(
        &self,
        prompt: String,
        env: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyAgentStartResult> {
        let env = self.merged_env(dict_to_env(env)?)?;
        let manifest_path = manifest_path_from_env(&env);
        let sandbox_config = self.sandbox_config()?;
        let run_tmux = self.hidden_run_tmux()?;
        let tmux_session = run_tmux
            .as_ref()
            .map(|value| value.session.clone())
            .or_else(|| self.tmux.clone());
        let tmux_window = run_tmux.as_ref().map(|value| value.window.clone());
        let started = send_agent(AgentSendOptions {
            kind: self.agent.clone(),
            name: self.effective_run_name().to_string(),
            binary: self.binary.clone(),
            version: self.version.clone(),
            agent_dir: self.dir.clone(),
            model: self.model.clone(),
            permission_mode: self.permission_mode.clone(),
            prompt,
            auto_continue_prompt: self.auto_continue_prompt.clone(),
            tmux_session,
            tmux_window,
            env,
            extra_args: self.extra_args.clone(),
            sandbox: self.sandbox,
            sandbox_config,
            bare: self.bare,
        })
        .map_err(PyAgentError::new_err)?;
        if let Some(manifest_path) = manifest_path {
            refresh_agent_from_dir(
                &manifest_path,
                &self.dir,
                self.effective_run_name(),
                &self.name,
                self.effective_run_name(),
                &self.agent,
                &self.model,
            )
            .map_err(PyAgentError::new_err)?;
        }

        Ok(PyAgentStartResult {
            tmux_session: started.tmux_session,
            tmux_window: started.tmux_window,
            tmux_pane_id: started.tmux_pane_id,
            agent_dir: started.agent_dir,
            command_file: started.command_file,
            events_file: started.events_file,
            attach_command: started.attach_command,
        })
    }

    #[pyo3(signature = (*, grace_seconds = 10, env = None))]
    fn stop(
        &self,
        grace_seconds: u64,
        env: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyAgentStopResult> {
        let env = self.merged_env(dict_to_env(env)?)?;
        let manifest_path = manifest_path_from_env(&env);
        let sandbox_config = self.sandbox_config()?;
        let run_tmux = self.hidden_run_tmux()?;
        let tmux_session = run_tmux
            .as_ref()
            .map(|value| value.session.clone())
            .or_else(|| self.tmux.clone());
        let tmux_window = run_tmux.as_ref().map(|value| value.window.clone());
        let stopped = stop_agent(AgentStopOptions {
            kind: self.agent.clone(),
            name: self.effective_run_name().to_string(),
            binary: self.binary.clone(),
            agent_dir: self.dir.clone(),
            model: self.model.clone(),
            permission_mode: self.permission_mode.clone(),
            auto_continue_prompt: self.auto_continue_prompt.clone(),
            tmux_session,
            tmux_window,
            env,
            extra_args: self.extra_args.clone(),
            sandbox: self.sandbox,
            sandbox_config,
            bare: self.bare,
            grace_seconds,
        })
        .map_err(PyAgentError::new_err)?;
        if let Some(manifest_path) = manifest_path {
            refresh_agent_from_dir(
                &manifest_path,
                &self.dir,
                self.effective_run_name(),
                &self.name,
                self.effective_run_name(),
                &self.agent,
                &self.model,
            )
            .map_err(PyAgentError::new_err)?;
        }

        Ok(PyAgentStopResult {
            tmux_session: stopped.tmux_session,
            agent_dir: stopped.agent_dir,
            events_file: stopped.events_file,
            log_file: stopped.log_file,
        })
    }
}

#[pymethods]
impl PyWorld {
    #[new]
    #[pyo3(signature = (*, dir))]
    fn new(dir: PathBuf) -> PyResult<Self> {
        let source = dir.to_string_lossy().to_string();
        let resolved = world_source::resolve_world_source(&source).map_err(|err| {
            PyAgentError::new_err(format!("failed to resolve world source: {err}"))
        })?;
        let dir = resolved.dir;
        let world_toml = dir.join("world.toml");
        if !world_toml.is_file() {
            return Err(PyAgentError::new_err(format!(
                "world directory must contain world.toml: {}",
                dir.display()
            )));
        }
        let world_config = read_world_config(&world_toml)?;
        let config = read_world_config_value(&world_toml)?;
        Ok(Self {
            dir,
            config,
            world_config,
            port: None,
            base_path: "/".to_string(),
            record: false,
            record_dir: None,
            health_timeout: 30,
            port_wait: 120,
            command_file: None,
            log_file: None,
            process: None,
            tmux_session: None,
            tmux_window: None,
            owns_tmux_session: false,
            manifest_path: None,
            next_agent_index: 0,
            world_proxies: Vec::new(),
            connected_sessions: Vec::new(),
            shm_dir: None,
            resume: None,
            operator_token: None,
        })
    }

    #[getter]
    fn dir(&self) -> PathBuf {
        self.dir.clone()
    }

    #[getter]
    fn config<'py>(&self, py: Python<'py>) -> PyResult<PyObject> {
        json_to_py(py, &self.config)
    }

    #[getter]
    fn name(&self) -> Option<String> {
        self.config
            .get("name")
            .and_then(|value| value.as_str())
            .map(ToString::to_string)
    }

    #[getter]
    fn description(&self) -> Option<String> {
        self.config
            .get("description")
            .and_then(|value| value.as_str())
            .map(ToString::to_string)
    }

    #[getter]
    fn api_doc_path(&self) -> String {
        self.api_doc_path_value()
    }

    #[getter]
    fn api<'py>(&self, py: Python<'py>) -> PyResult<PyObject> {
        let api = serde_json::json!({
            "doc_path": self.api_doc_path_value(),
        });
        json_to_py(py, &api)
    }

    #[getter]
    fn scripts<'py>(&self, py: Python<'py>) -> PyResult<PyObject> {
        self.config_section(py, "scripts")
    }

    #[getter]
    fn renderer<'py>(&self, py: Python<'py>) -> PyResult<PyObject> {
        self.config_section(py, "renderer")
    }

    #[getter]
    fn runtime<'py>(&self, py: Python<'py>) -> PyResult<PyObject> {
        self.config_section(py, "runtime")
    }

    #[getter]
    fn run<'py>(&self, py: Python<'py>) -> PyResult<PyObject> {
        self.config_section(py, "run")
    }

    #[getter]
    fn run_command(&self) -> Option<Vec<String>> {
        self.config
            .get("run")
            .and_then(|run| run.get("command"))
            .and_then(|command| command.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(ToString::to_string))
                    .collect()
            })
    }

    #[getter]
    fn replay_command(&self) -> Option<Vec<String>> {
        self.config
            .get("replay")
            .and_then(|replay| replay.get("command"))
            .and_then(|command| command.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(ToString::to_string))
                    .collect()
            })
    }

    #[getter]
    fn uses_run_command(&self) -> bool {
        self.run_command().is_some()
    }

    #[getter]
    fn runtime_mode(&self) -> String {
        self.config
            .get("runtime")
            .and_then(|runtime| runtime.get("mode"))
            .and_then(|value| value.as_str())
            .unwrap_or("clawblox")
            .to_string()
    }

    #[getter]
    fn port(&self) -> Option<u16> {
        self.port
    }

    #[getter]
    fn url(&self) -> PyResult<String> {
        let port = self.running_port()?;
        Ok(format!("http://localhost:{}{}", port, self.base_path)
            .trim_end_matches('/')
            .to_string())
    }

    #[getter]
    fn command_file(&self) -> Option<PathBuf> {
        self.command_file.clone()
    }

    #[getter]
    fn log_file(&self) -> Option<PathBuf> {
        self.log_file.clone()
    }

    #[getter]
    fn start_command(&self) -> Vec<String> {
        self.run_command().unwrap_or_else(|| {
            vec![
                "clawblox".to_string(),
                "run".to_string(),
                "{world_dir}".to_string(),
                "--port".to_string(),
                "{port}".to_string(),
                "--base-path".to_string(),
                "{base_path}".to_string(),
            ]
        })
    }

    #[getter]
    fn start_env<'py>(&self, py: Python<'py>) -> PyResult<PyObject> {
        let env = if self.uses_run_command() {
            serde_json::json!({
                "WORLD_HOST": "127.0.0.1",
                "WORLD_PORT": "{port}",
                "WORLD_BASE_PATH": "{base_path}",
                "WORLD_RECORD": "{record}",
                "WORLD_RECORD_DIR": "{record_dir}",
            })
        } else {
            serde_json::json!({})
        };
        json_to_py(py, &env)
    }

    #[pyo3(signature = (*, port, base_path = "/".to_string(), record = false, record_dir = None, resume = None, health_timeout = 30, port_wait = 120, command_file = None, log_file = None, _tmux_session = None))]
    #[allow(clippy::too_many_arguments)]
    fn start(
        &mut self,
        port: u16,
        base_path: String,
        record: bool,
        record_dir: Option<PathBuf>,
        resume: Option<PathBuf>,
        health_timeout: u64,
        port_wait: u64,
        command_file: Option<PathBuf>,
        log_file: Option<PathBuf>,
        _tmux_session: Option<String>,
    ) -> PyResult<PyWorldStartResult> {
        if let Some(process) = self.process.as_mut() {
            if process.try_wait().map_err(io_error)?.is_none() {
                let command = if self.uses_run_command() {
                    self.run_command().unwrap_or_default()
                } else {
                    build_world_start_argv(
                        &resolve_clawblox_bin()?,
                        &self.dir,
                        self.running_port()?,
                        &self.base_path,
                        self.record,
                        self.record_dir.as_ref(),
                        self.resume.as_ref(),
                    )
                };
                return Ok(PyWorldStartResult {
                    command,
                    start_command: self.start_command(),
                    delegated: self.uses_run_command(),
                    run_command: self.run_command(),
                    env: self.resolved_start_env(self.running_port()?),
                    url: self.url()?,
                    command_file: self.running_command_file()?,
                    log_file: self.running_log_file()?,
                    manifest_path: self.manifest_path.clone(),
                });
            }
        }
        if let Some(session) = &self.tmux_session {
            if tmux_target_exists(session, self.tmux_window.as_deref()) {
                return Ok(PyWorldStartResult {
                    command: if self.uses_run_command() {
                        self.run_command().unwrap_or_default()
                    } else {
                        build_world_start_argv(
                            &resolve_clawblox_bin()?,
                            &self.dir,
                            self.running_port()?,
                            &self.base_path,
                            self.record,
                            self.record_dir.as_ref(),
                            self.resume.as_ref(),
                        )
                    },
                    start_command: self.start_command(),
                    delegated: self.uses_run_command(),
                    run_command: self.run_command(),
                    env: self.resolved_start_env(self.running_port()?),
                    url: self.url()?,
                    command_file: self.running_command_file()?,
                    log_file: self.running_log_file()?,
                    manifest_path: self.manifest_path.clone(),
                });
            }
        }
        let state_root = record_dir
            .as_ref()
            .and_then(|path| path.parent().map(|path| path.to_path_buf()))
            .unwrap_or_else(|| self.dir.join(".clawblox"));
        self.port = Some(port);
        self.base_path = normalize_base_path(&base_path);
        if self.uses_run_command() && self.base_path != "/" {
            return Err(PyAgentError::new_err(
                "world.toml [run].command is configured, so base_path must be '/'".to_string(),
            ));
        }
        self.record = record;
        self.record_dir = record_dir;
        self.resume = match resume {
            Some(path) if !path.is_file() => {
                return Err(PyAgentError::new_err(format!(
                    "resume snapshot not found: {}",
                    path.display()
                )));
            }
            other => other.map(|path| {
                if path.is_absolute() {
                    path
                } else {
                    std::env::current_dir().unwrap_or_default().join(path)
                }
            }),
        };
        self.operator_token = Some(generate_operator_token());
        self.health_timeout = health_timeout;
        self.port_wait = port_wait;
        self.command_file =
            Some(command_file.unwrap_or_else(|| state_root.join("runtime/world.sh")));
        self.log_file = Some(log_file.unwrap_or_else(|| state_root.join("logs/world.log")));
        self.manifest_path = Some(manifest_path_for_state_root(&state_root));
        if self.uses_run_command() {
            let shm_dir = state_root.join("shm");
            std::fs::create_dir_all(&shm_dir).map_err(io_error)?;
            self.shm_dir = Some(shm_dir);
        }

        wait_for_port_release(port, Duration::from_secs(self.port_wait))?;

        let command_file = self.running_command_file()?;
        let log_file = self.running_log_file()?;
        let tmux_session = Some(
            _tmux_session
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| default_tmux_session(&self.dir)),
        );
        let tmux_window = tmux_session.as_ref().map(|_| "world-0".to_string());
        let command = if self.uses_run_command() {
            let spec = self.external_world_command(port)?;
            write_external_world_command_file(
                &command_file,
                &spec,
                if tmux_session.is_some() {
                    None
                } else {
                    Some(&log_file)
                },
            )
            .map_err(io_error)?;
            if let Some(session) = &tmux_session {
                let created_tmux_session = start_tmux_command_file(
                    session,
                    tmux_window.as_deref().unwrap_or("world-0"),
                    &command_file,
                    &log_file,
                    &spec.env(),
                )?;
                self.tmux_session = Some(session.clone());
                self.tmux_window = tmux_window.clone();
                self.owns_tmux_session = created_tmux_session;
                self.process = None;
                start_tmux_parent_watchdog(
                    session,
                    tmux_window.as_deref(),
                    created_tmux_session,
                    Duration::from_secs(1),
                    &log_file,
                )?;
            } else {
                let child = spawn_external_world(&spec, Some(&log_file)).map_err(|err| {
                    PyAgentError::new_err(format!(
                        "{err}\ncommand file: {}\nlog file: {}",
                        command_file.display(),
                        log_file.display()
                    ))
                })?;
                self.process = Some(child);
            }
            spec.command
        } else {
            let bin = resolve_clawblox_bin()?;
            let command = build_world_start_argv(
                &bin,
                &self.dir,
                port,
                &self.base_path,
                self.record,
                self.record_dir.as_ref(),
                self.resume.as_ref(),
            );
            write_world_command_file(&command_file, &bin, self, tmux_session.is_none())?;
            if let Some(parent) = log_file.parent() {
                std::fs::create_dir_all(parent).map_err(io_error)?;
            }
            let log = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_file)
                .map_err(io_error)?;
            let err_log = log.try_clone().map_err(io_error)?;
            if let Some(session) = &tmux_session {
                drop(log);
                drop(err_log);
                let builtin_env: Vec<(String, String)> = self
                    .operator_token
                    .as_ref()
                    .map(|token| vec![("WORLD_OPERATOR_TOKEN".to_string(), token.clone())])
                    .unwrap_or_default();
                let created_tmux_session = start_tmux_command_file(
                    session,
                    tmux_window.as_deref().unwrap_or("world-0"),
                    &command_file,
                    &log_file,
                    &builtin_env,
                )?;
                self.tmux_session = Some(session.clone());
                self.tmux_window = tmux_window.clone();
                self.owns_tmux_session = created_tmux_session;
                self.process = None;
                start_tmux_parent_watchdog(
                    session,
                    tmux_window.as_deref(),
                    created_tmux_session,
                    Duration::from_secs(1),
                    &log_file,
                )?;
            } else {
                let child = Command::new(&bin)
                    .arg("run")
                    .arg(&self.dir)
                    .arg("--port")
                    .arg(port.to_string())
                    .arg("--base-path")
                    .arg(&self.base_path)
                    .args(if self.record {
                        vec!["--record"]
                    } else {
                        vec![]
                    })
                    .args(
                        self.record_dir
                            .as_ref()
                            .map(|path| {
                                vec!["--record-dir".to_string(), path.display().to_string()]
                            })
                            .unwrap_or_default(),
                    )
                    .args(
                        self.resume
                            .as_ref()
                            .map(|path| vec!["--resume".to_string(), path.display().to_string()])
                            .unwrap_or_default(),
                    )
                    .envs(
                        self.operator_token
                            .as_ref()
                            .map(|token| vec![("WORLD_OPERATOR_TOKEN".to_string(), token.clone())])
                            .unwrap_or_default(),
                    )
                    .stdout(Stdio::from(log))
                    .stderr(Stdio::from(err_log))
                    .spawn()
                    .map_err(|err| {
                        PyAgentError::new_err(format!(
                            "failed to start world command {}: {err}\ncommand file: {}\nlog file: {}",
                            bin.display(),
                            command_file.display(),
                            log_file.display()
                        ))
                    })?;
                self.process = Some(child);
            }
            command
        };

        let health_path = join_url_path(&self.base_path, "/api.md");
        let deadline = Instant::now() + Duration::from_secs(self.health_timeout);
        while Instant::now() < deadline {
            if let Some(process) = self.process.as_mut() {
                if let Some(status) = process.try_wait().map_err(io_error)? {
                    return Err(PyAgentError::new_err(format!(
                        "world process exited before becoming healthy with status {status}\ncommand file: {}\nlog file: {}\n\n{}",
                        command_file.display(),
                        log_file.display(),
                        format_log_tail(&log_file, 80)
                    )));
                }
            }
            if let Some(session) = &self.tmux_session {
                if !tmux_target_exists(session, self.tmux_window.as_deref()) {
                    return Err(PyAgentError::new_err(format!(
                        "world tmux target exited before becoming healthy\ncommand file: {}\nlog file: {}\n\n{}",
                        command_file.display(),
                        log_file.display(),
                        format_log_tail(&log_file, 80)
                    )));
                }
            }
            if http_request("GET", port, &health_path, &[], "")
                .is_ok_and(|response| response.status == 200)
            {
                let url = self.url()?;
                if let Some(manifest_path) = &self.manifest_path {
                    record_world_started(
                        manifest_path,
                        WorldStarted {
                            source_dir: &self.dir,
                            runtime_mode: &self.runtime_mode(),
                            url: &url,
                            port,
                            record_dir: self.record_dir.as_ref().map(PathBuf::as_path),
                            command_file: &command_file,
                            log_file: &log_file,
                            command: command.clone(),
                            replay_command: self.replay_command().unwrap_or_default(),
                        },
                    )
                    .map_err(PyAgentError::new_err)?;
                }
                return Ok(PyWorldStartResult {
                    command,
                    start_command: self.start_command(),
                    delegated: self.uses_run_command(),
                    run_command: self.run_command(),
                    env: self.resolved_start_env(port),
                    url,
                    command_file,
                    log_file,
                    manifest_path: self.manifest_path.clone(),
                });
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        Err(PyAgentError::new_err(format!(
            "world did not become healthy at {} within {}s\ncommand file: {}\nlog file: {}\n\n{}",
            self.url()?,
            self.health_timeout,
            command_file.display(),
            log_file.display(),
            format_log_tail(&log_file, 80)
        )))
    }

    #[pyo3(signature = (*, agent))]
    fn connect<'py>(&mut self, py: Python<'py>, agent: &PyAgent) -> PyResult<Bound<'py, PyDict>> {
        let port = self.running_port()?;
        let session_file = agent.dir.join("world_session.txt");
        let existing_session = std::fs::read_to_string(&session_file)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let join_headers = existing_session
            .as_ref()
            .map(|session| vec![("X-Session".to_string(), session.clone())])
            .unwrap_or_default();
        let path = format!(
            "{}?name={}",
            join_url_path(&self.base_path, "/join"),
            percent_encode(agent.name())
        );
        let response = http_request("POST", port, &path, &join_headers, "").map_err(|err| {
            PyAgentError::new_err(format!(
                "failed to join world at {}{}: {err}",
                self.url()
                    .unwrap_or_else(|_| format!("http://localhost:{port}")),
                path
            ))
        })?;
        if response.status != 200 {
            return Err(PyAgentError::new_err(format!(
                "failed to join world at {}{}: HTTP {}\nbody: {}",
                self.url()?,
                path,
                response.status,
                response.body
            )));
        }
        let value: serde_json::Value = serde_json::from_str(&response.body).map_err(|err| {
            PyAgentError::new_err(format!("failed to parse world join response: {err}"))
        })?;
        let session = value
            .get("session")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                PyAgentError::new_err("world join response missing session".to_string())
            })?;
        if let Some(expected) = &existing_session {
            if session != expected {
                return Err(PyAgentError::new_err(format!(
                    "resume error: expected world session {expected} but server returned {session}"
                )));
            }
        }
        let agent_id = value
            .get("agent_id")
            .and_then(|value| value.as_str())
            .unwrap_or("");

        if let Some(parent) = session_file.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                PyAgentError::new_err(format!(
                    "failed to create agent session directory {}: {err}",
                    parent.display()
                ))
            })?;
        }
        std::fs::write(&session_file, format!("{session}\n")).map_err(|err| {
            PyAgentError::new_err(format!(
                "failed to write world session file {}: {err}",
                session_file.display()
            ))
        })?;
        if !self.connected_sessions.iter().any(|value| value == session) {
            self.connected_sessions.push(session.to_string());
        }

        if let Some(tmux_session) = &self.tmux_session {
            let window = format!("agent-0-{}", self.next_agent_index);
            self.next_agent_index += 1;
            let mut guard = agent.run_tmux.lock().map_err(|_| {
                PyAgentError::new_err("agent tmux state lock was poisoned".to_string())
            })?;
            *guard = Some(AgentRunTmux {
                session: tmux_session.clone(),
                window,
            });
        }

        let internal_base_url = self.url()?;
        let mut base_url = internal_base_url.clone();
        let mut world_env = vec![
            ("WORLD_BASE_URL".to_string(), base_url.clone()),
            (
                "WORLD_INTERNAL_BASE_URL".to_string(),
                internal_base_url.clone(),
            ),
            ("WORLD_SESSION_ID".to_string(), session.to_string()),
            ("WORLD_AGENT_NAME".to_string(), agent.name().to_string()),
        ];
        if agent.sandbox {
            agent
                .set_sandbox_config(self.world_config.sandbox.clone())
                .map_err(|err| {
                    PyAgentError::new_err(format!(
                        "failed to configure sandbox for world connection: {err}"
                    ))
                })?;
            let proxy_port = self
                .start_capability_proxy(agent, &internal_base_url)
                .map_err(|err| {
                    PyAgentError::new_err(format!(
                        "failed to start world capability proxy for sandboxed agent: {err}"
                    ))
                })?;
            let agent_impl = build_agent(&agent.agent, None, false).map_err(|err| {
                PyAgentError::new_err(format!(
                    "failed to build agent implementation for world connection: {err}"
                ))
            })?;
            let uses_socat = agent_impl.needs_socat_sandbox();
            base_url = if agent_impl.name() == "claude" && !agent.claude_native_sandbox {
                // Claude can opt out of its native sandbox when Clawblox's
                // outer bubblewrap is the only sandbox boundary. In that mode
                // it must address the per-agent capability proxy directly.
                format!("http://world:{proxy_port}")
            } else if uses_socat {
                // Agents that keep a native sandbox network policy, currently
                // Claude, use the synthetic world host plus the policy's
                // httpProxyPort instead of putting the proxy port in the URL.
                "http://world".to_string()
            } else {
                // Codex/Kimi run inside Clawblox's outer bubblewrap. Their
                // shell commands reach only the per-agent capability proxy via
                // http://world:<proxy_port>; the proxy then enforces the world
                // API surface and session token.
                format!("http://world:{proxy_port}")
            };
            world_env[0] = ("WORLD_BASE_URL".to_string(), base_url.clone());
            world_env.push((
                "CLAWBLOX_WORLD_HTTP_PROXY_PORT".to_string(),
                proxy_port.to_string(),
            ));
            if agent_impl.name() == "claude" && !agent.claude_native_sandbox {
                world_env.push((
                    "CLAWBLOX_CLAUDE_NATIVE_SANDBOX".to_string(),
                    "0".to_string(),
                ));
            }
        }
        if let Some(shm_dir) = &self.shm_dir {
            world_env.push(("WORLD_SHM_DIR".to_string(), shm_dir.display().to_string()));
        }
        if self
            .config
            .get("sandbox")
            .and_then(|s| s.get("allow_loopback"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            world_env.push(("WORLD_ALLOW_LOOPBACK".to_string(), "1".to_string()));
        }
        if let Some(manifest_path) = &self.manifest_path {
            world_env.push((
                "CLAWBLOX_MANIFEST_PATH".to_string(),
                manifest_path.display().to_string(),
            ));
            record_agent_connected(
                manifest_path,
                AgentConnected {
                    id: if agent_id.is_empty() {
                        agent.effective_run_name()
                    } else {
                        agent_id
                    },
                    name: agent.name(),
                    run_name: agent.effective_run_name(),
                    backend: &agent.agent,
                    model: &agent.model,
                    agent_dir: &agent.dir,
                    world_session: session,
                    world_agent_id: agent_id,
                    base_url: &base_url,
                    internal_base_url: &internal_base_url,
                },
            )
            .map_err(PyAgentError::new_err)?;
        }
        agent.set_world_env(world_env)?;

        let headers = PyDict::new_bound(py);
        headers.set_item("X-Session", session)?;
        let access = PyDict::new_bound(py);
        access.set_item("headers", headers)?;
        access.set_item("session", session)?;
        access.set_item("agent_id", agent_id)?;
        access.set_item("base_url", base_url)?;
        access.set_item("internal_base_url", internal_base_url)?;
        Ok(access)
    }

    #[pyo3(signature = (*, path = "/api.md".to_string(), access = None))]
    fn api_path(&self, path: String, access: Option<&Bound<'_, PyDict>>) -> PyResult<String> {
        let port = self.running_port()?;
        let headers = access_headers(access)?;
        let request_path = join_url_path(&self.base_path, &path);
        match http_request("GET", port, &request_path, &headers, "") {
            Ok(response) if response.status == 200 => Ok(path),
            Ok(response) => Err(PyAgentError::new_err(format!(
                "world API path {} returned HTTP {}\nbody: {}",
                request_path, response.status, response.body
            ))),
            Err(err) => Err(err),
        }
    }

    #[pyo3(signature = (*, dir, name = None))]
    fn save<'py>(
        &mut self,
        py: Python<'py>,
        dir: PathBuf,
        name: Option<String>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let port = self.running_port()?;
        let mut headers = Vec::new();
        if let Some(token) = &self.operator_token {
            headers.push(("X-Operator-Token".to_string(), token.clone()));
        }
        let request_path = join_url_path(&self.base_path, "/snapshot");
        let response = http_request("GET", port, &request_path, &headers, "")?;
        if response.status != 200 {
            return Err(PyAgentError::new_err(format!(
                "GET /snapshot returned HTTP {}; this world may not implement the save/load protocol\nbody: {}",
                response.status,
                response.body.chars().take(400).collect::<String>()
            )));
        }
        let (format, checkpoint_tick, sim_time) =
            serde_json::from_str::<serde_json::Value>(&response.body)
                .map(|doc| {
                    (
                        doc.get("format")
                            .and_then(|value| value.as_str())
                            .unwrap_or("world-snapshot")
                            .to_string(),
                        doc.get("instance")
                            .and_then(|value| value.get("tick"))
                            .and_then(|value| value.as_u64()),
                        doc.get("time").and_then(|value| value.as_f64()),
                    )
                })
                .unwrap_or_else(|_| ("world-snapshot".to_string(), None, None));
        std::fs::create_dir_all(&dir).map_err(io_error)?;
        let file_name = name.unwrap_or_else(|| {
            format!(
                "{}.snapshot",
                chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ")
            )
        });
        let snapshot_path = dir.join(file_name);
        std::fs::write(&snapshot_path, &response.body).map_err(io_error)?;
        if let Some(manifest_path) = &self.manifest_path {
            record_checkpoint(
                manifest_path,
                &snapshot_path,
                &format,
                checkpoint_tick,
                sim_time,
            )
            .map_err(PyAgentError::new_err)?;
        }
        let result = PyDict::new_bound(py);
        result.set_item("path", snapshot_path)?;
        result.set_item("format", format)?;
        result.set_item("tick", checkpoint_tick)?;
        result.set_item("sim_time", sim_time)?;
        Ok(result)
    }

    // Manifest annotation for orchestration tools. World lifecycle remains
    // expressed through start/save/stop; this only records the run metadata
    // those tools want replay/app consumers to see.
    #[pyo3(signature = (*, id, index = None, status = "complete".to_string(), started_at = "".to_string(), ended_at = "".to_string(), resume_from = None))]
    #[allow(clippy::too_many_arguments)]
    fn record_run(
        &mut self,
        id: String,
        index: Option<u32>,
        status: String,
        started_at: String,
        ended_at: String,
        resume_from: Option<PathBuf>,
    ) -> PyResult<()> {
        let Some(manifest_path) = &self.manifest_path else {
            return Ok(());
        };
        record_manifest_run(
            manifest_path,
            RunRecorded {
                id: &id,
                index,
                status: &status,
                started_at: &started_at,
                ended_at: &ended_at,
                resume_from: resume_from.as_deref(),
            },
        )
        .map_err(PyAgentError::new_err)
    }

    #[pyo3(signature = (*, grace_seconds = 10))]
    fn stop(&mut self, grace_seconds: u64) -> PyResult<()> {
        // Ports sandboxed agents bridge to via daemonized socat processes;
        // collected before teardown so leftovers can be reaped afterwards.
        let mut bridge_ports: Vec<u16> = self
            .world_proxies
            .iter()
            .map(|proxy| proxy.port())
            .collect();
        if let Ok(port) = self.running_port() {
            bridge_ports.push(port);
        }
        let result = self.stop_teardown(grace_seconds);
        reap_socat_bridges(&bridge_ports);
        result
    }

    fn stop_teardown(&mut self, grace_seconds: u64) -> PyResult<()> {
        self.leave_connected_sessions();
        self.stop_world_proxies();
        if let Some(session) = self.tmux_session.take() {
            let target_window = self.tmux_window.take();
            if tmux_target_exists(&session, target_window.as_deref()) {
                send_tmux_ctrl_c(&session, target_window.as_deref());
                let deadline = Instant::now() + Duration::from_secs(grace_seconds);
                while Instant::now() < deadline {
                    if !tmux_target_exists(&session, target_window.as_deref()) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            if self.owns_tmux_session {
                kill_tmux_session(&session);
            } else if tmux_target_exists(&session, target_window.as_deref()) {
                kill_tmux_window(&session, target_window.as_deref());
            }
            self.owns_tmux_session = false;
            return Ok(());
        }
        let Some(mut process) = self.process.take() else {
            return Ok(());
        };
        terminate_process(process.id());
        let deadline = Instant::now() + Duration::from_secs(grace_seconds);
        while Instant::now() < deadline {
            if process.try_wait().map_err(io_error)?.is_some() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        process.kill().map_err(io_error)?;
        let _ = process.wait();
        Ok(())
    }
}

impl PyWorld {
    fn api_doc_path_value(&self) -> String {
        self.config
            .get("api")
            .and_then(|api| api.get("doc"))
            .and_then(|value| value.as_str())
            .or_else(|| {
                self.config
                    .get("scripts")
                    .and_then(|scripts| scripts.get("skill"))
                    .and_then(|value| value.as_str())
            })
            .unwrap_or("API.md")
            .to_string()
    }

    fn config_section(&self, py: Python<'_>, name: &str) -> PyResult<PyObject> {
        json_to_py(
            py,
            self.config
                .get(name)
                .unwrap_or(&serde_json::Value::Object(Default::default())),
        )
    }

    fn external_world_command(&self, port: u16) -> PyResult<ExternalWorldCommand> {
        Ok(ExternalWorldCommand {
            world_dir: self.dir.clone(),
            command: self.run_command().ok_or_else(|| {
                PyAgentError::new_err("world.toml [run].command is not configured".to_string())
            })?,
            port,
            base_path: self.base_path.clone(),
            record: self.record,
            record_dir: self.record_dir.clone(),
            manifest_path: self.manifest_path.clone(),
            resume: self.resume.clone(),
            operator_token: self.operator_token.clone(),
            clawblox_bin: None,
            shm_dir: self.shm_dir.clone(),
        })
    }

    fn resolved_start_env(&self, port: u16) -> Vec<(String, String)> {
        if self.uses_run_command() {
            let mut env = vec![
                ("WORLD_HOST".to_string(), "127.0.0.1".to_string()),
                ("WORLD_PORT".to_string(), port.to_string()),
                ("WORLD_BASE_PATH".to_string(), self.base_path.clone()),
                (
                    "WORLD_RECORD".to_string(),
                    if self.record { "true" } else { "false" }.to_string(),
                ),
            ];
            if let Some(record_dir) = &self.record_dir {
                env.push((
                    "WORLD_RECORD_DIR".to_string(),
                    record_dir.display().to_string(),
                ));
            }
            if let Some(manifest_path) = &self.manifest_path {
                env.push((
                    "CLAWBLOX_MANIFEST_PATH".to_string(),
                    manifest_path.display().to_string(),
                ));
            }
            if let Some(resume) = &self.resume {
                env.push((
                    "WORLD_RESUME_PATH".to_string(),
                    resume.display().to_string(),
                ));
            }
            if let Some(operator_token) = &self.operator_token {
                env.push(("WORLD_OPERATOR_TOKEN".to_string(), operator_token.clone()));
            }
            env
        } else {
            Vec::new()
        }
    }

    fn running_port(&self) -> PyResult<u16> {
        self.port.ok_or_else(|| {
            PyAgentError::new_err(
                "world has not been started; call world.start(port=...) first".to_string(),
            )
        })
    }

    fn running_command_file(&self) -> PyResult<PathBuf> {
        self.command_file.clone().ok_or_else(|| {
            PyAgentError::new_err(
                "world has not been started; no command file exists yet".to_string(),
            )
        })
    }

    fn running_log_file(&self) -> PyResult<PathBuf> {
        self.log_file.clone().ok_or_else(|| {
            PyAgentError::new_err("world has not been started; no log file exists yet".to_string())
        })
    }

    fn start_capability_proxy(&mut self, agent: &PyAgent, target_base_url: &str) -> PyResult<u16> {
        let session_token_file = agent.dir.join("world_session.txt");
        let proxy = start_world_capability_proxy(WorldCapabilityProxyConfig {
            target_base_url: target_base_url.to_string(),
            public_host: "world".to_string(),
            session_token_file,
            session_header: "X-Session".to_string(),
        })
        .map_err(PyAgentError::new_err)?;
        let port = proxy.port();
        self.world_proxies.push(proxy);
        Ok(port)
    }

    fn stop_world_proxies(&mut self) {
        for proxy in &mut self.world_proxies {
            proxy.stop();
        }
        self.world_proxies.clear();
    }

    fn leave_connected_sessions(&mut self) {
        let leave_path = std::env::var("CLAWBLOX_WORLD_LEAVE_PATH")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let Some(leave_path) = leave_path else {
            return;
        };
        let Ok(port) = self.running_port() else {
            return;
        };
        let request_path = join_url_path(&self.base_path, &leave_path);
        for session in std::mem::take(&mut self.connected_sessions) {
            let headers = vec![("X-Session".to_string(), session)];
            let _ = http_request("POST", port, &request_path, &headers, "");
        }
    }
}

/// Claude Code's native sandbox spawns daemonized `socat` bridges
/// (`UNIX-LISTEN:/tmp/claude-http-*.sock -> TCP:localhost:<port>`) that
/// outlive the agent when its tmux window is torn down. Left alone they
/// accumulate across runs until new agent launches fail with EAGAIN
/// (hundreds were observed on a shared host), so on world stop any bridge
/// still targeting one of this world's ports is killed.
fn reap_socat_bridges(ports: &[u16]) {
    if ports.is_empty() {
        return;
    }
    let needles: Vec<String> = ports
        .iter()
        .map(|port| format!("TCP:localhost:{port}"))
        .collect();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(raw) = std::fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let cmdline = String::from_utf8_lossy(&raw);
        let argv0 = cmdline.split('\0').next().unwrap_or("");
        if !argv0.ends_with("socat") {
            continue;
        }
        if needles
            .iter()
            .any(|needle| cmdline.contains(needle.as_str()))
        {
            let _ = std::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .status();
        }
    }
}

impl Drop for PyWorld {
    fn drop(&mut self) {
        let _ = self.stop(2);
    }
}

#[pymethods]
impl PyWorldStartResult {
    #[getter]
    fn command(&self) -> Vec<String> {
        self.command.clone()
    }

    #[getter]
    fn start_command(&self) -> Vec<String> {
        self.start_command.clone()
    }

    #[getter]
    fn delegated(&self) -> bool {
        self.delegated
    }

    #[getter]
    fn run_command(&self) -> Option<Vec<String>> {
        self.run_command.clone()
    }

    #[getter]
    fn env<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new_bound(py);
        for (key, value) in &self.env {
            dict.set_item(key, value)?;
        }
        Ok(dict)
    }

    #[getter]
    fn url(&self) -> &str {
        &self.url
    }

    #[getter]
    fn command_file(&self) -> PathBuf {
        self.command_file.clone()
    }

    #[getter]
    fn log_file(&self) -> PathBuf {
        self.log_file.clone()
    }

    #[getter]
    fn manifest_path(&self) -> Option<PathBuf> {
        self.manifest_path.clone()
    }
}

#[pymethods]
impl PyAgentStartResult {
    #[getter]
    fn tmux_session(&self) -> &str {
        &self.tmux_session
    }

    #[getter]
    fn tmux_window(&self) -> Option<&str> {
        self.tmux_window.as_deref()
    }

    #[getter]
    fn tmux_pane_id(&self) -> Option<&str> {
        self.tmux_pane_id.as_deref()
    }

    #[getter]
    fn agent_dir(&self) -> PathBuf {
        self.agent_dir.clone()
    }

    #[getter]
    fn command_file(&self) -> PathBuf {
        self.command_file.clone()
    }

    #[getter]
    fn events_file(&self) -> PathBuf {
        self.events_file.clone()
    }

    #[getter]
    fn attach_command(&self) -> &str {
        &self.attach_command
    }
}

#[pymethods]
impl PyAgentStopResult {
    #[getter]
    fn tmux_session(&self) -> Option<&str> {
        self.tmux_session.as_deref()
    }

    #[getter]
    fn agent_dir(&self) -> PathBuf {
        self.agent_dir.clone()
    }

    #[getter]
    fn events_file(&self) -> PathBuf {
        self.events_file.clone()
    }

    #[getter]
    fn log_file(&self) -> PathBuf {
        self.log_file.clone()
    }
}

#[derive(Debug)]
struct PyAgentError;

impl PyAgentError {
    fn new_err(message: String) -> PyErr {
        pyo3::exceptions::PyRuntimeError::new_err(message)
    }
}

fn dict_to_env(env: Option<&Bound<'_, PyDict>>) -> PyResult<Vec<(String, String)>> {
    let Some(env) = env else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for (key, value) in env.iter() {
        out.push((key.extract()?, value.extract()?));
    }
    Ok(out)
}

fn manifest_path_from_env(env: &[(String, String)]) -> Option<PathBuf> {
    env.iter()
        .rev()
        .find(|(key, value)| key == "CLAWBLOX_MANIFEST_PATH" && !value.trim().is_empty())
        .map(|(_, value)| PathBuf::from(value))
}

#[derive(Debug)]
struct HttpResponse {
    status: u16,
    body: String,
}

fn resolve_clawblox_bin() -> PyResult<PathBuf> {
    if let Ok(value) = std::env::var("CLAWBLOX_BIN") {
        let path = PathBuf::from(value);
        if path.exists() {
            return Ok(path);
        }
    }
    // Prefer the most recently built binary. Debug and release builds can
    // both exist; blindly preferring debug runs worlds on a build that can
    // be an order of magnitude slower per tick, which starves the world's
    // HTTP responses and surfaces as EAGAIN timeouts in agent setup.
    let newest_existing = |candidates: &[PathBuf]| -> Option<PathBuf> {
        candidates
            .iter()
            .filter(|path| path.exists())
            .max_by_key(|path| {
                std::fs::metadata(path)
                    .and_then(|meta| meta.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            })
            .cloned()
    };
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .map(Path::to_path_buf);
    if let Some(root) = workspace_root {
        let candidates = [
            root.join("target/debug/clawblox"),
            root.join("target/release/clawblox"),
        ];
        if let Some(path) = newest_existing(&candidates) {
            return Ok(path);
        }
    }
    let candidates = [
        PathBuf::from("target/debug/clawblox"),
        PathBuf::from("target/release/clawblox"),
    ];
    if let Some(path) = newest_existing(&candidates) {
        return Ok(path);
    }
    if command_exists("clawblox") {
        return Ok(PathBuf::from("clawblox"));
    }
    Err(PyAgentError::new_err(
        "could not find clawblox binary; set CLAWBLOX_BIN or build target/debug/clawblox"
            .to_string(),
    ))
}

fn read_world_config(path: &Path) -> PyResult<WorldConfig> {
    WorldConfig::from_file(path).map_err(|err| PyAgentError::new_err(err.to_string()))
}

fn read_world_config_value(path: &PathBuf) -> PyResult<serde_json::Value> {
    read_world_config(path)?;
    let content = std::fs::read_to_string(path).map_err(io_error)?;
    let value: toml::Value = toml::from_str(&content).map_err(|err| {
        PyAgentError::new_err(format!("failed to parse {}: {err}", path.display()))
    })?;
    serde_json::to_value(value).map_err(|err| {
        PyAgentError::new_err(format!(
            "failed to convert {} to JSON: {err}",
            path.display()
        ))
    })
}

fn json_to_py(py: Python<'_>, value: &serde_json::Value) -> PyResult<PyObject> {
    match value {
        serde_json::Value::Null => Ok(py.None()),
        serde_json::Value::Bool(value) => Ok(value.into_py(py)),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(value.into_py(py))
            } else if let Some(value) = value.as_u64() {
                Ok(value.into_py(py))
            } else if let Some(value) = value.as_f64() {
                Ok(value.into_py(py))
            } else {
                Ok(py.None())
            }
        }
        serde_json::Value::String(value) => Ok(value.into_py(py)),
        serde_json::Value::Array(items) => {
            let list = PyList::empty_bound(py);
            for item in items {
                list.append(json_to_py(py, item)?)?;
            }
            Ok(list.into_py(py))
        }
        serde_json::Value::Object(items) => {
            let dict = PyDict::new_bound(py);
            for (key, value) in items {
                dict.set_item(key, json_to_py(py, value)?)?;
            }
            Ok(dict.into_py(py))
        }
    }
}

fn command_exists(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).exists()))
        .unwrap_or(false)
}

fn run_workspace_uv_sync_on_host(workspace_dir: &Path, runtime_dir: &Path) -> PyResult<()> {
    std::fs::create_dir_all(runtime_dir).map_err(io_error)?;
    let mut command = Command::new("uv");
    command
        .args([
            "sync",
            "--no-install-project",
            "--no-dev",
            "--link-mode",
            "copy",
        ])
        .current_dir(workspace_dir)
        .env_remove("UV_NO_SYNC")
        .env_remove("VIRTUAL_ENV")
        .env("UV_PYTHON_INSTALL_DIR", runtime_dir.join("python"));
    run_checked(command, "failed to prepare workspace Python environment")
}

fn run_workspace_uv_sync_in_sandbox(
    workspace_dir: &Path,
    runtime_dir: &Path,
    sandbox_config: &SandboxConfig,
) -> PyResult<()> {
    std::fs::create_dir_all(workspace_dir).map_err(io_error)?;
    std::fs::create_dir_all(runtime_dir).map_err(io_error)?;
    let uv = resolve_program("uv").ok_or_else(|| {
        PyAgentError::new_err(
            "workspace has pyproject.toml, but uv was not found in PATH".to_string(),
        )
    })?;
    let mut args = Vec::new();
    // no process started by `uv sync` (a build script) may outlive
    // it: a private pid namespace dies with its init, and bwrap dies with its parent.
    args.extend(["--unshare-pid".to_string(), "--die-with-parent".to_string()]);
    if sandbox_config.enable_weaker_nested_sandbox {
        // Match the main agent sandbox: nested Docker mode should not make the
        // guest process root, because Claude bypass mode refuses root.
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
        "--ro-bind".to_string(),
        uv.display().to_string(),
        "/sandbox-bin/uv".to_string(),
        "--bind".to_string(),
        workspace_dir.display().to_string(),
        "/workspace".to_string(),
        "--bind".to_string(),
        runtime_dir.display().to_string(),
        "/runtime".to_string(),
    ]);
    // the same read-only control files as the agent's sandbox,
    // since an agent-written pyproject.toml decides what runs here.
    for name in clawblox::agent_sandbox::RUNTIME_CONTROL_FILES {
        args.extend([
            "--ro-bind-try".to_string(),
            runtime_dir.join(name).display().to_string(),
            format!("/runtime/{name}"),
        ]);
    }
    args.extend([
        "--chdir".to_string(),
        "/workspace".to_string(),
        "--setenv".to_string(),
        "PATH".to_string(),
        if sandbox_config.deps_root.is_some() {
            "/sandbox-bin:/sandbox-deps/bin:/usr/local/bin:/usr/bin:/bin".to_string()
        } else {
            "/sandbox-bin:/usr/local/bin:/usr/bin:/bin".to_string()
        },
        "--setenv".to_string(),
        "UV_PYTHON_INSTALL_DIR".to_string(),
        "/runtime/python".to_string(),
    ]);
    if let Some(root) = sandbox_config.deps_root.as_ref() {
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
        if sandbox_config.add_deps_lib_to_library_path && root.join("lib").is_dir() {
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
    if Path::new("/mnt/wsl/resolv.conf").is_file() {
        args.extend([
            "--ro-bind".to_string(),
            "/mnt/wsl/resolv.conf".to_string(),
            "/mnt/wsl/resolv.conf".to_string(),
        ]);
    }
    args.extend([
        "--".to_string(),
        "/sandbox-bin/uv".to_string(),
        "sync".to_string(),
        "--no-install-project".to_string(),
        "--no-dev".to_string(),
        "--link-mode".to_string(),
        "copy".to_string(),
    ]);

    let mut command = Command::new("bwrap");
    command
        .args(args)
        .env_remove("UV_NO_SYNC")
        .env_remove("VIRTUAL_ENV");
    run_checked(
        command,
        "failed to prepare sandboxed workspace Python environment",
    )
}

fn resolve_program(program: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let path = dir.join(program);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

fn run_checked(mut command: Command, label: &str) -> PyResult<()> {
    let output = command
        .output()
        .map_err(|err| PyAgentError::new_err(format!("{label}: failed to run command: {err}")))?;
    if output.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(PyAgentError::new_err(format!(
        "{label}: command exited with status {}\nstdout:\n{}\nstderr:\n{}",
        output.status, stdout, stderr
    )))
}

fn build_world_start_argv(
    bin: &PathBuf,
    dir: &PathBuf,
    port: u16,
    base_path: &str,
    record: bool,
    record_dir: Option<&PathBuf>,
    resume: Option<&PathBuf>,
) -> Vec<String> {
    let mut argv = vec![
        bin.display().to_string(),
        "run".to_string(),
        dir.display().to_string(),
        "--port".to_string(),
        port.to_string(),
        "--base-path".to_string(),
        base_path.to_string(),
    ];
    if record {
        argv.push("--record".to_string());
    }
    if let Some(record_dir) = record_dir {
        argv.push("--record-dir".to_string());
        argv.push(record_dir.display().to_string());
    }
    if let Some(resume) = resume {
        argv.push("--resume".to_string());
        argv.push(resume.display().to_string());
    }
    argv
}

fn generate_operator_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut token = String::with_capacity(32);
    for _ in 0..2 {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(std::process::id() as u64);
        token.push_str(&format!("{:016x}", hasher.finish()));
    }
    token
}

fn write_world_command_file(
    path: &PathBuf,
    bin: &PathBuf,
    world: &PyWorld,
    redirect_to_log: bool,
) -> PyResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io_error)?;
    }
    let mut parts = vec![
        shell_quote(&bin.display().to_string()),
        "run".to_string(),
        shell_quote(&world.dir.display().to_string()),
        "--port".to_string(),
        world.running_port()?.to_string(),
        "--base-path".to_string(),
        shell_quote(&world.base_path),
    ];
    if world.record {
        parts.push("--record".to_string());
    }
    if let Some(record_dir) = &world.record_dir {
        parts.push("--record-dir".to_string());
        parts.push(shell_quote(&record_dir.display().to_string()));
    }
    if let Some(resume) = &world.resume {
        parts.push("--resume".to_string());
        parts.push(shell_quote(&resume.display().to_string()));
    }
    if redirect_to_log {
        parts.push(">>".to_string());
        parts.push(shell_quote(
            &world.running_log_file()?.display().to_string(),
        ));
        parts.push("2>&1".to_string());
    }
    std::fs::write(
        path,
        format!(
            "#!/usr/bin/env bash\nset -euo pipefail\n{}\n",
            parts.join(" ")
        ),
    )
    .map_err(io_error)?;
    make_command_file_executable(path).map_err(io_error)
}

#[cfg(unix)]
fn make_command_file_executable(path: &PathBuf) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
fn make_command_file_executable(_path: &PathBuf) -> std::io::Result<()> {
    Ok(())
}

fn wait_for_port_release(port: u16, timeout: Duration) -> PyResult<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_err() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(PyAgentError::new_err(format!(
        "port {port} is still in use after {}s",
        timeout.as_secs()
    )))
}

fn http_request(
    method: &str,
    port: u16,
    path: &str,
    headers: &[(String, String)],
    body: &str,
) -> PyResult<HttpResponse> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(io_error)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .map_err(io_error)?;
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(name);
        request.push_str(": ");
        request.push_str(value);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    request.push_str(body);
    stream.write_all(request.as_bytes()).map_err(io_error)?;
    let raw = read_http_response(&mut stream)?;
    let header_end = response_header_end(&raw).unwrap_or(raw.len());
    let head = String::from_utf8_lossy(&raw[..header_end]);
    let body_start = (header_end + 4).min(raw.len());
    let body = String::from_utf8_lossy(&raw[body_start..]).to_string();
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(0);
    Ok(HttpResponse { status, body })
}

fn read_http_response(stream: &mut TcpStream) -> PyResult<Vec<u8>> {
    let mut raw = Vec::new();
    let mut buffer = [0u8; 4096];
    let mut expected_len = None;
    loop {
        let count = stream.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            return Ok(raw);
        }
        raw.extend_from_slice(&buffer[..count]);
        if expected_len.is_none() {
            if let Some(header_end) = response_header_end(&raw) {
                expected_len = response_expected_len(&raw, header_end);
            }
        }
        if let Some(expected_len) = expected_len {
            if raw.len() >= expected_len {
                raw.truncate(expected_len);
                return Ok(raw);
            }
        }
    }
}

fn response_header_end(raw: &[u8]) -> Option<usize> {
    raw.windows(4).position(|window| window == b"\r\n\r\n")
}

fn response_expected_len(raw: &[u8], header_end: usize) -> Option<usize> {
    let head = String::from_utf8_lossy(&raw[..header_end]);
    for line in head.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            let content_len = value.trim().parse::<usize>().unwrap_or(0);
            return Some(header_end + 4 + content_len);
        }
    }
    None
}

fn access_headers(access: Option<&Bound<'_, PyDict>>) -> PyResult<Vec<(String, String)>> {
    let Some(access) = access else {
        return Ok(Vec::new());
    };
    let Some(headers) = access.get_item("headers")? else {
        return Ok(Vec::new());
    };
    let headers = headers.downcast::<PyDict>()?;
    dict_to_env(Some(headers))
}

fn normalize_base_path(base_path: &str) -> String {
    let trimmed = base_path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return "/".to_string();
    }
    format!("/{}", trimmed.trim_matches('/'))
}

fn join_url_path(base_path: &str, route: &str) -> String {
    let base_path = normalize_base_path(base_path);
    let route = format!("/{}", route.trim_start_matches('/'));
    if base_path == "/" {
        route
    } else {
        format!("{base_path}{route}")
    }
}

fn percent_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn format_log_tail(path: &PathBuf, max_lines: usize) -> String {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) => return format!("failed to read log file: {err}"),
    };
    let lines: Vec<&str> = contents.lines().collect();
    if lines.is_empty() {
        return "log file is empty".to_string();
    }
    let start = lines.len().saturating_sub(max_lines);
    format!("last {max_lines} log lines:\n{}", lines[start..].join("\n"))
}

fn tmux_target(session: &str, window: Option<&str>) -> String {
    match window {
        Some(window) => format!("{session}:{window}"),
        None => session.to_string(),
    }
}

fn tmux_session_exists(session: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", session])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn tmux_target_exists(session: &str, window: Option<&str>) -> bool {
    match window {
        Some(window) => Command::new("tmux")
            .args(["list-windows", "-t", session, "-F", "#{window_name}"])
            .stderr(Stdio::null())
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

fn tmux_session_created(session: &str) -> Option<String> {
    let output = Command::new("tmux")
        .args(["display-message", "-p", "-t", session, "#{session_created}"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn process_start_time(pid: u32) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(") ")?
        .1
        .split_whitespace()
        .nth(19)
        .map(ToString::to_string)
}

fn start_tmux_parent_watchdog(
    session: &str,
    window: Option<&str>,
    owns_session: bool,
    grace: Duration,
    log_file: &PathBuf,
) -> PyResult<()> {
    let Some(session_created) = tmux_session_created(session) else {
        return Ok(());
    };
    let parent_pid = std::process::id();
    let parent_start = process_start_time(parent_pid).unwrap_or_default();
    let watchdog_log = log_file.with_extension("watchdog.log");
    let target = tmux_target(session, window);
    let stop_target = if owns_session {
        format!(
            r#"
tmux list-panes -a -t "$session" -F '##{{pane_id}}' 2>/dev/null | while IFS= read -r pane; do
  [ -n "$pane" ] && tmux send-keys -t "$pane" C-c 2>/dev/null || true
done
sleep "$grace"
tmux has-session -t "$session" 2>/dev/null || exit 0
current_created="$(tmux display-message -p -t "$session" '##{{session_created}}' 2>/dev/null || true)"
[ "$current_created" = "$session_created" ] || exit 0
tmux kill-session -t "$session" 2>/dev/null || true
"#
        )
    } else {
        format!(
            r#"
tmux has-session -t "$session" 2>/dev/null || exit 0
current_created="$(tmux display-message -p -t "$session" '##{{session_created}}' 2>/dev/null || true)"
[ "$current_created" = "$session_created" ] || exit 0
tmux list-windows -t "$session" -F '##{{window_name}}' 2>/dev/null | grep -Fx -- "$window" >/dev/null || exit 0
tmux send-keys -t "$target" C-c 2>/dev/null || true
sleep "$grace"
tmux has-session -t "$session" 2>/dev/null || exit 0
current_created="$(tmux display-message -p -t "$session" '##{{session_created}}' 2>/dev/null || true)"
[ "$current_created" = "$session_created" ] || exit 0
tmux list-windows -t "$session" -F '##{{window_name}}' 2>/dev/null | grep -Fx -- "$window" >/dev/null || exit 0
tmux kill-window -t "$target" 2>/dev/null || true
"#
        )
    };
    let command = format!(
        r#"
parent_pid={parent_pid}
parent_start={parent_start}
session={session}
window={window}
target={target}
session_created={session_created}
grace={grace}
log_file={log_file}
same_parent() {{
  if [ -n "$parent_start" ] && [ -r "/proc/$parent_pid/stat" ]; then
    current="$(awk '{{print $22}}' "/proc/$parent_pid/stat" 2>/dev/null || true)"
    [ "$current" = "$parent_start" ]
  else
    kill -0 "$parent_pid" 2>/dev/null
  fi
}}
while same_parent; do
  sleep 0.2
done
tmux has-session -t "$session" 2>/dev/null || exit 0
current_created="$(tmux display-message -p -t "$session" '##{{session_created}}' 2>/dev/null || true)"
[ "$current_created" = "$session_created" ] || exit 0
printf '%s parent process exited; stopping tmux target %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$target" >> "$log_file"
{stop_target}
"#,
        parent_pid = parent_pid,
        parent_start = shell_quote(&parent_start),
        session = shell_quote(session),
        window = shell_quote(window.unwrap_or("")),
        target = shell_quote(&target),
        session_created = shell_quote(&session_created),
        grace = grace.as_secs(),
        log_file = shell_quote(&watchdog_log.display().to_string()),
        stop_target = stop_target,
    );
    let status = Command::new("tmux")
        .args(["run-shell", "-b", &command])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|err| PyAgentError::new_err(format!("failed to start tmux watchdog: {err}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(PyAgentError::new_err(format!(
            "tmux failed to start parent watchdog for session '{session}'"
        )))
    }
}

fn start_tmux_command_file(
    session: &str,
    window: &str,
    command_file: &PathBuf,
    log_file: &PathBuf,
    env: &[(String, String)],
) -> PyResult<bool> {
    let tmux_command = format!("exec {}", shell_quote(&command_file.display().to_string()));
    let mut command = Command::new("tmux");
    let created_session = !tmux_session_exists(session);
    if tmux_session_exists(session) {
        command.args(["new-window", "-d", "-t", session, "-n", window]);
    } else {
        command.args(["new-session", "-d", "-s", session, "-n", window]);
    }
    for (key, value) in env {
        command.args(["-e", &format!("{key}={value}")]);
        command.env(key, value);
    }
    let status = command
        .arg(&tmux_command)
        .status()
        .map_err(|err| PyAgentError::new_err(format!("failed to start tmux: {err}")))?;
    if !status.success() {
        return Err(PyAgentError::new_err(format!(
            "tmux failed to start world target '{}'",
            tmux_target(session, Some(window))
        )));
    }
    if let Some(parent) = log_file.parent() {
        std::fs::create_dir_all(parent).map_err(io_error)?;
    }
    let pipe_command = format!("cat >> {}", shell_quote(&log_file.display().to_string()));
    let status = Command::new("tmux")
        .args([
            "pipe-pane",
            "-o",
            "-t",
            &tmux_target(session, Some(window)),
            &pipe_command,
        ])
        .status()
        .map_err(|err| PyAgentError::new_err(format!("failed to attach tmux log pipe: {err}")))?;
    if !status.success() {
        kill_tmux_window(session, Some(window));
        return Err(PyAgentError::new_err(format!(
            "tmux failed to attach log pipe for world target '{}'",
            tmux_target(session, Some(window))
        )));
    }
    Ok(created_session)
}

fn send_tmux_ctrl_c(session: &str, window: Option<&str>) {
    let _ = Command::new("tmux")
        .args(["send-keys", "-t", &tmux_target(session, window), "C-c"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn kill_tmux_window(session: &str, window: Option<&str>) {
    if let Some(window) = window {
        let _ = Command::new("tmux")
            .args(["kill-window", "-t", &tmux_target(session, Some(window))])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn kill_tmux_session(session: &str) {
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", session])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn terminate_process(pid: u32) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status();
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string()])
            .status();
    }
}

fn io_error(err: std::io::Error) -> PyErr {
    PyAgentError::new_err(err.to_string())
}

/// Generate a default tmux session name from the world directory name and a
/// compact UTC timestamp:
///   {sanitized_dir_name}-{YYYYMMDDTHHmmssZ}
/// e.g. "mesa-island-20260527T194512Z"
fn default_tmux_session(world_dir: &Path) -> String {
    let dir_name = world_dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "world".to_string());
    // Keep [A-Za-z0-9._-], replace rest with '_'
    let safe: String = dir_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    format!("{safe}-{ts}")
}

#[cfg(test)]
mod tests {
    use super::{response_expected_len, response_header_end};

    #[test]
    fn response_expected_len_uses_content_length() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhelloextra";
        let header_end = response_header_end(raw).unwrap();
        assert_eq!(response_expected_len(raw, header_end), Some(43));
    }

    #[test]
    fn response_expected_len_reads_to_close_without_content_length() {
        let raw = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nhello";
        let header_end = response_header_end(raw).unwrap();
        assert_eq!(response_expected_len(raw, header_end), None);
    }
}

#[pymodule]
fn _clawblox(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyAgent>()?;
    m.add_class::<PyWorld>()?;
    m.add_class::<PyWorldStartResult>()?;
    m.add_class::<PyAgentStartResult>()?;
    m.add_class::<PyAgentStopResult>()?;
    m.add_function(wrap_pyfunction!(default_system_prompt, m)?)?;
    Ok(())
}

#[pyfunction]
fn default_system_prompt() -> &'static str {
    rust_default_system_prompt()
}
