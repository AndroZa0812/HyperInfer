//! Sandbox policies and pluggable `CommandRunner` backends (including NVIDIA OpenShell).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

/// Environment variable names that commonly hold sensitive credentials and are
/// stripped from child processes when `strip_env_secrets` is enabled.
pub const SENSITIVE_ENV_VARS: &[&str] = &[
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "ANTIGRAVITY_API_KEY",
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "DATABASE_URL",
    "REDIS_URL",
];

/// Declarative security policy for command execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxPolicy {
    /// Directories where the sandboxed process is permitted to write files.
    pub writable_paths: Vec<PathBuf>,
    /// Whether arbitrary outbound network access is allowed.
    pub allow_network: bool,
    /// Specific domains allowed via egress proxy when `allow_network` is false.
    pub allowed_domains: Vec<String>,
    /// Whether to strip known secret/credential environment variables from subprocesses.
    pub strip_env_secrets: bool,
}

impl SandboxPolicy {
    /// Default restrictive policy for a workspace:
    /// - Writes restricted to `workspace_root` and `/tmp`
    /// - Outbound network disabled by default
    /// - Sensitive credential environment variables stripped
    pub fn restricted_for_workspace(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            writable_paths: vec![workspace_root.into(), PathBuf::from("/tmp")],
            allow_network: false,
            allowed_domains: Vec::new(),
            strip_env_secrets: true,
        }
    }

    pub fn with_allowed_domains(mut self, domains: Vec<String>) -> Self {
        self.allowed_domains = domains;
        self
    }

    pub fn with_network(mut self, allow_network: bool) -> Self {
        self.allow_network = allow_network;
        self
    }

    /// Render a declarative NVIDIA OpenShell policy YAML document matching this policy.
    pub fn to_openshell_policy_yaml(&self) -> String {
        let mut yaml = String::new();
        yaml.push_str("version: \"1\"\n");
        yaml.push_str("filesystem:\n");
        yaml.push_str("  read_only:\n");
        yaml.push_str("    - \"/\"\n");
        yaml.push_str("  read_write:\n");
        for path in &self.writable_paths {
            yaml.push_str(&format!("    - \"{}\"\n", path.display()));
        }
        yaml.push_str("network:\n");
        if self.allow_network {
            yaml.push_str("  mode: \"allow_all\"\n");
        } else if self.allowed_domains.is_empty() {
            yaml.push_str("  mode: \"deny_all\"\n");
        } else {
            yaml.push_str("  mode: \"egress_allowlist\"\n");
            yaml.push_str("  allowed_domains:\n");
            for domain in &self.allowed_domains {
                yaml.push_str(&format!("    - \"{}\"\n", domain));
            }
        }
        yaml
    }
}

/// Result of running a shell command via a `CommandRunner`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandExecution {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    /// True if executed inside an isolated sandbox (e.g. OpenShell).
    pub sandboxed: bool,
}

/// Pluggable execution backend for `BashTool`.
#[async_trait]
pub trait CommandRunner: Send + Sync {
    /// Identifier of the runner backend (e.g. `"host"`, `"openshell"`).
    fn name(&self) -> &str;

    /// Whether this runner enforces kernel-level sandbox isolation.
    fn is_sandboxed(&self) -> bool;

    /// Execute `command` in `cwd` within `timeout` under `policy`.
    async fn run(
        &self,
        command: &str,
        cwd: &Path,
        timeout: Duration,
        policy: &SandboxPolicy,
    ) -> Result<CommandExecution, String>;
}

/// Apply environment sanitization to a `tokio::process::Command`.
pub(crate) fn apply_env_policy(cmd: &mut Command, policy: &SandboxPolicy) {
    if policy.strip_env_secrets {
        for var in SENSITIVE_ENV_VARS {
            cmd.env_remove(var);
        }
        for (k, _) in std::env::vars() {
            let upper = k.to_uppercase();
            if upper.ends_with("_API_KEY")
                || upper.ends_with("_SECRET")
                || upper.ends_with("_TOKEN")
            {
                cmd.env_remove(&k);
            }
        }
    }
}

/// Host command runner (`sh -c <command>`) with environment secret stripping.
#[derive(Debug, Clone, Default)]
pub struct HostRunner;

#[async_trait]
impl CommandRunner for HostRunner {
    fn name(&self) -> &str {
        "host"
    }

    fn is_sandboxed(&self) -> bool {
        false
    }

    async fn run(
        &self,
        command: &str,
        cwd: &Path,
        timeout: Duration,
        policy: &SandboxPolicy,
    ) -> Result<CommandExecution, String> {
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(command)
            .current_dir(cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        apply_env_policy(&mut cmd, policy);

        let output = tokio::time::timeout(timeout, cmd.output())
            .await
            .map_err(|_| {
                format!(
                    "Command timed out after {}s: `{}`",
                    timeout.as_secs(),
                    command
                )
            })?
            .map_err(|e| format!("Failed to spawn command `{}`: {}", command, e))?;

        Ok(CommandExecution {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            sandboxed: false,
        })
    }
}

/// NVIDIA OpenShell sandbox runner (`openshell`).
///
/// Wraps shell commands inside an OpenShell sandbox with a declarative Landlock
/// filesystem and network egress policy.
#[derive(Debug, Clone)]
pub struct OpenShellRunner {
    pub binary_path: String,
    pub sandbox_name: String,
    pub policy_file: Option<PathBuf>,
}

impl OpenShellRunner {
    pub fn new(sandbox_name: impl Into<String>) -> Self {
        Self {
            binary_path: "openshell".to_string(),
            sandbox_name: sandbox_name.into(),
            policy_file: None,
        }
    }

    pub fn with_policy_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.policy_file = Some(path.into());
        self
    }

    pub fn with_binary_path(mut self, binary_path: impl Into<String>) -> Self {
        self.binary_path = binary_path.into();
        self
    }

    /// Build the CLI argument list passed to the `openshell` binary.
    pub fn build_args(&self, command: &str, policy_path: &Path) -> Vec<String> {
        vec![
            "sandbox".to_string(),
            "exec".to_string(),
            "--name".to_string(),
            self.sandbox_name.clone(),
            "--policy".to_string(),
            policy_path.display().to_string(),
            "--".to_string(),
            "sh".to_string(),
            "-c".to_string(),
            command.to_string(),
        ]
    }
}

#[async_trait]
impl CommandRunner for OpenShellRunner {
    fn name(&self) -> &str {
        "openshell"
    }

    fn is_sandboxed(&self) -> bool {
        true
    }

    async fn run(
        &self,
        command: &str,
        cwd: &Path,
        timeout: Duration,
        policy: &SandboxPolicy,
    ) -> Result<CommandExecution, String> {
        // Materialize policy YAML if an explicit policy_file was not provided.
        let policy_path = match &self.policy_file {
            Some(p) => p.clone(),
            None => {
                let generated_path = std::env::temp_dir()
                    .join(format!("hypercode-openshell-{}.yaml", self.sandbox_name));
                let yaml = policy.to_openshell_policy_yaml();
                tokio::fs::write(&generated_path, yaml)
                    .await
                    .map_err(|e| format!("Failed to write OpenShell policy YAML: {}", e))?;
                generated_path
            }
        };

        let args = self.build_args(command, &policy_path);
        let mut cmd = Command::new(&self.binary_path);
        cmd.args(&args)
            .current_dir(cwd)
            .env("OPENSHELL_SANDBOX_POLICY", &policy_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        apply_env_policy(&mut cmd, policy);

        let output = tokio::time::timeout(timeout, cmd.output())
            .await
            .map_err(|_| {
                format!(
                    "OpenShell command timed out after {}s: `{}`",
                    timeout.as_secs(),
                    command
                )
            })?
            .map_err(|e| {
                format!(
                    "Failed to invoke OpenShell binary '{}' for `{}`: {}",
                    self.binary_path, command, e
                )
            })?;

        Ok(CommandExecution {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            sandboxed: true,
        })
    }
}
