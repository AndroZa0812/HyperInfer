//! RTK-style deterministic command output filtering, `tee` raw-log persistence,
//! and session-level token-savings tracking for `BashTool`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

pub mod rules;

#[cfg(test)]
mod tests;

/// Configuration for command output filtering and `tee` log persistence.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputFilterConfig {
    pub max_output_bytes: usize,
    pub max_log_files: usize,
    pub tee_compression_threshold: f64,
    pub use_rtk: bool,
}

impl Default for OutputFilterConfig {
    fn default() -> Self {
        Self {
            max_output_bytes: 32 * 1024,
            max_log_files: 50,
            tee_compression_threshold: 0.20,
            use_rtk: false,
        }
    }
}

/// Statistics for a single filtered command execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterStats {
    pub raw_bytes: usize,
    pub filtered_bytes: usize,
    pub estimated_tokens_saved: usize,
    pub rule_name: String,
}

/// Result returned by `OutputFilter::filter_and_tee`.
#[derive(Debug, Clone)]
pub struct FilterOutcome {
    pub output: String,
    pub stats: FilterStats,
    pub tee_log_path: Option<PathBuf>,
}

/// Thread-safe accumulator of session-wide output filter token savings.
#[derive(Default, Debug)]
pub struct SessionFilterStats {
    inner: Mutex<SessionFilterStatsInner>,
}

#[derive(Debug, Default)]
struct SessionFilterStatsInner {
    commands_filtered: u64,
    total_raw_bytes: u64,
    total_filtered_bytes: u64,
    total_tokens_saved: u64,
}

/// Immutable snapshot of session-wide output filter statistics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFilterSnapshot {
    pub commands_filtered: u64,
    pub total_raw_bytes: u64,
    pub total_filtered_bytes: u64,
    pub total_tokens_saved: u64,
}

impl SessionFilterStats {
    pub fn record(&self, stats: &FilterStats) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.commands_filtered += 1;
        inner.total_raw_bytes += stats.raw_bytes as u64;
        inner.total_filtered_bytes += stats.filtered_bytes as u64;
        inner.total_tokens_saved += stats.estimated_tokens_saved as u64;
    }

    pub fn snapshot(&self) -> SessionFilterSnapshot {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        SessionFilterSnapshot {
            commands_filtered: inner.commands_filtered,
            total_raw_bytes: inner.total_raw_bytes,
            total_filtered_bytes: inner.total_filtered_bytes,
            total_tokens_saved: inner.total_tokens_saved,
        }
    }
}

static LOG_SEQ: AtomicU64 = AtomicU64::new(1);

fn sanitize_cmd(cmd: &str) -> String {
    let cleaned: String = cmd
        .split_whitespace()
        .take(3)
        .collect::<Vec<_>>()
        .join("-")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    if cleaned.is_empty() {
        "cmd".to_string()
    } else {
        cleaned
    }
}

fn combine_streams(stdout: &str, stderr: &str) -> String {
    match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => String::new(),
        (false, true) => stdout.to_string(),
        (true, false) => stderr.to_string(),
        (false, false) => {
            if stdout.ends_with('\n') {
                format!("{}{}", stdout, stderr)
            } else {
                format!("{}\n{}", stdout, stderr)
            }
        }
    }
}

fn prune_logs(log_dir: &Path, max_files: usize) {
    let Ok(entries) = fs::read_dir(log_dir) else {
        return;
    };
    let mut logs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "log"))
        .collect();
    // Sequence prefixes are zero-padded (`000001-...`), so lexicographical order
    // matches chronological creation order.
    logs.sort();
    while logs.len() > max_files {
        let oldest = logs.remove(0);
        let _ = fs::remove_file(&oldest);
    }
}

/// Deterministic command-aware output filter with `tee` safety net.
pub struct OutputFilter {
    config: OutputFilterConfig,
}

impl OutputFilter {
    pub fn new(config: OutputFilterConfig) -> Self {
        Self { config }
    }

    pub fn filter_and_tee(
        &self,
        command: &str,
        exit_code: i32,
        stdout: &str,
        stderr: &str,
        workspace_root: &Path,
    ) -> FilterOutcome {
        let raw = combine_streams(stdout, stderr);
        let raw_bytes = raw.len();
        let cleaned = rules::strip_noise(&raw);

        let (rule_name, filtered) = if rules::should_bypass_aggressive_filter(command) {
            (
                "bypass".to_string(),
                rules::truncate_head_tail(&cleaned, self.config.max_output_bytes),
            )
        } else {
            let rule = rules::detect_rule(command);
            let result = rules::apply_rule(rule, &cleaned, self.config.max_output_bytes);
            (rule.to_string(), result)
        };

        let filtered_bytes = filtered.len();
        let saved = raw_bytes.saturating_sub(filtered_bytes);
        let estimated_tokens_saved = saved.div_ceil(4);

        let stats = FilterStats {
            raw_bytes,
            filtered_bytes,
            estimated_tokens_saved,
            rule_name,
        };

        let should_tee = exit_code != 0
            || (raw_bytes > 0
                && (saved as f64 / raw_bytes as f64) > self.config.tee_compression_threshold);

        let tee_log_path = if should_tee {
            let log_dir = workspace_root.join(".hypercode/logs");
            if fs::create_dir_all(&log_dir).is_ok() {
                let sanitized = sanitize_cmd(command);
                let mut written_path = None;
                for _ in 0..1000 {
                    let seq = LOG_SEQ.fetch_add(1, Ordering::Relaxed);
                    let log_path = log_dir.join(format!("{:06}-{}.log", seq, sanitized));
                    match fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&log_path)
                    {
                        Ok(mut file) => {
                            if std::io::Write::write_all(&mut file, raw.as_bytes()).is_ok() {
                                written_path = Some(log_path);
                            } else {
                                let _ = fs::remove_file(&log_path);
                            }
                            break;
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                        Err(_) => break,
                    }
                }
                if written_path.is_some() {
                    prune_logs(&log_dir, self.config.max_log_files);
                }
                written_path
            } else {
                None
            }
        } else {
            None
        };

        FilterOutcome {
            output: filtered,
            stats,
            tee_log_path,
        }
    }
}

/// Split leading `KEY=VALUE` environment variable assignments from the rest of a command.
fn split_leading_env_assignments(trimmed: &str) -> (&str, &str) {
    let mut offset = 0;
    let bytes = trimmed.as_bytes();
    while offset < bytes.len() {
        while offset < bytes.len() && bytes[offset].is_ascii_whitespace() {
            offset += 1;
        }
        let token_start = offset;
        while offset < bytes.len() && !bytes[offset].is_ascii_whitespace() {
            offset += 1;
        }
        let token = &trimmed[token_start..offset];
        if token.contains('=') && !token.starts_with('-') && !token.starts_with('=') {
            continue;
        }
        return (trimmed[..token_start].trim_end(), &trimmed[token_start..]);
    }
    (trimmed, "")
}

/// Optionally wrap `command` with an external `rtk` binary if `--use-rtk` is enabled,
/// the command is a simple single command without bypass flags, and `rtk` is on `PATH`.
pub fn maybe_wrap_with_rtk(command: &str, config: &OutputFilterConfig) -> Option<String> {
    if !config.use_rtk {
        return None;
    }
    if rules::should_bypass_aggressive_filter(command) {
        return None;
    }
    let trimmed = command.trim_start();
    if trimmed.is_empty()
        || trimmed.contains("&&")
        || trimmed.contains("||")
        || trimmed.contains(';')
    {
        return None;
    }
    let (env_prefix, cmd_rest) = split_leading_env_assignments(trimmed);
    if cmd_rest.is_empty() || cmd_rest.starts_with("rtk ") || cmd_rest == "rtk" {
        return None;
    }
    let path_os = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_os) {
        let candidate = dir.join("rtk");
        if candidate.is_file() {
            return if env_prefix.is_empty() {
                Some(format!("rtk {}", cmd_rest))
            } else {
                Some(format!("{} rtk {}", env_prefix, cmd_rest))
            };
        }
    }
    None
}
