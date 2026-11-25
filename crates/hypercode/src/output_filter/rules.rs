use regex::Regex;
use std::sync::OnceLock;

fn ansi_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(\x1b\[[0-9;]*[A-Za-z]|\x1b\][0-9;]*[^\x07]*\x07|\x1b[()][0-9A-Za-z])")
            .unwrap()
    })
}

fn spinner_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\s*[⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏]\s*$").unwrap())
}

/// Detect explicit verbose flags or shell pipes/redirects where aggressive
/// command-specific filtering should be skipped to respect agent intent.
pub fn should_bypass_aggressive_filter(command: &str) -> bool {
    if command.contains('|') || command.contains('>') {
        return true;
    }
    command
        .split_whitespace()
        .any(|tok| matches!(tok, "--nocapture" | "--verbose" | "-vv" | "-vvv"))
}

/// Strip ANSI escape sequences, resolve `\r` progress-bar overwrites, drop
/// spinner-only lines, and collapse runs of 3+ blank lines into a single blank line.
pub fn strip_noise(input: &str) -> String {
    let cleaned = ansi_re().replace_all(input, "");
    let mut lines: Vec<&str> = Vec::new();
    for line in cleaned.lines() {
        let effective = line.rsplit('\r').next().unwrap_or(line);
        if spinner_re().is_match(effective) {
            continue;
        }
        lines.push(effective);
    }

    let mut out = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim().is_empty() {
            let mut count = 0;
            while i + count < lines.len() && lines[i + count].trim().is_empty() {
                count += 1;
            }
            if count >= 3 {
                out.push("");
            } else {
                out.extend(std::iter::repeat_n("", count));
            }
            i += count;
        } else {
            out.push(lines[i]);
            i += 1;
        }
    }
    out.join("\n")
}

/// UTF-8 char-boundary-safe head/tail truncation in O(n) time.
pub fn truncate_head_tail(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_string();
    }
    let marker = format!("\n... [truncated {} bytes] ...\n", input.len() - max_bytes);
    if max_bytes <= marker.len() {
        let mut end = max_bytes;
        while end > 0 && !input.is_char_boundary(end) {
            end -= 1;
        }
        return input[..end].to_string();
    }

    let budget = max_bytes - marker.len();
    let head_budget = budget / 2;
    let tail_budget = budget - head_budget;

    let mut head_end = head_budget.min(input.len());
    while head_end > 0 && !input.is_char_boundary(head_end) {
        head_end -= 1;
    }

    let mut tail_start = input.len().saturating_sub(tail_budget).max(head_end);
    while tail_start < input.len() && !input.is_char_boundary(tail_start) {
        tail_start += 1;
    }

    format!("{}{}{}", &input[..head_end], marker, &input[tail_start..])
}

/// Extract command tokens after skipping leading `KEY=VALUE` env assignments
/// and common runner wrappers (`uv run`, `npx`, `pnpm`, `npm run`).
fn normalized_command_tokens(command: &str) -> Vec<&str> {
    let mut tokens = command.split_whitespace().peekable();
    while let Some(&tok) = tokens.peek() {
        if tok.contains('=') && !tok.starts_with('-') && !tok.starts_with('=') {
            tokens.next();
        } else {
            break;
        }
    }
    let mut rest: Vec<&str> = tokens.collect();
    if rest.len() >= 2 && rest[0] == "uv" && rest[1] == "run" {
        rest.drain(0..2);
    } else if rest.len() >= 2 && matches!(rest[0], "npx" | "pnpm") {
        rest.drain(0..1);
    }
    rest
}

/// Determine the filter rule to apply based on the normalized command tokens.
pub fn detect_rule(command: &str) -> &'static str {
    let tokens = normalized_command_tokens(command);
    let base = tokens.first().copied().unwrap_or("");
    let sub = tokens.get(1).copied().unwrap_or("");

    match base {
        "cargo" => match sub {
            "test" | "nextest" => return "cargo-test",
            "check" | "clippy" | "build" => return "cargo-check",
            _ => {}
        },
        "git" => match sub {
            "status" => return "git-status",
            "diff" => return "git-diff",
            _ => {}
        },
        "pytest" | "jest" | "vitest" => return "test-runner",
        "bun" if sub == "test" => return "test-runner",
        _ => {}
    }

    "generic"
}

/// Apply the selected deterministic rule, collapse consecutive identical lines,
/// and enforce `max_bytes` via head/tail truncation.
pub fn apply_rule(rule: &str, cleaned: &str, max_bytes: usize) -> String {
    let lines: Vec<&str> = cleaned.lines().collect();
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());

    match rule {
        "cargo-test" => {
            let mut in_failure = false;
            for &line in &lines {
                let trimmed = line.trim_start();
                if trimmed.starts_with("failures:") || trimmed.starts_with("---- ") {
                    in_failure = true;
                }
                if in_failure {
                    out.push(line);
                    if trimmed.starts_with("test result:") {
                        in_failure = false;
                    }
                    continue;
                }
                if trimmed.starts_with("Compiling ")
                    || trimmed.starts_with("Checking ")
                    || trimmed.starts_with("Blocking ")
                    || trimmed.starts_with("Finished ")
                    || trimmed.starts_with("Fresh ")
                {
                    continue;
                }
                if trimmed.starts_with("test ") && line.trim_end().ends_with("... ok") {
                    continue;
                }
                if trimmed.starts_with("PASS ") {
                    continue;
                }
                out.push(line);
            }
        }
        "cargo-check" => {
            let mut last_note = "";
            for &line in &lines {
                let trimmed = line.trim_start();
                if trimmed.starts_with("Compiling ")
                    || trimmed.starts_with("Checking ")
                    || trimmed.starts_with("Finished ")
                {
                    continue;
                }
                if trimmed.starts_with("note:") || trimmed.starts_with("help:") {
                    if trimmed == last_note {
                        continue;
                    }
                    last_note = trimmed;
                } else {
                    last_note = "";
                }
                out.push(line);
            }
        }
        "git-status" => {
            for &line in &lines {
                if line.contains("(use \"git ") {
                    continue;
                }
                out.push(line);
            }
        }
        "git-diff" => {
            for &line in &lines {
                if line.starts_with("index ") && line.contains("..") {
                    continue;
                }
                out.push(line);
            }
        }
        "test-runner" => {
            for &line in &lines {
                let trimmed = line.trim_start();
                if line.contains("PASSED") || line.contains('✓') || trimmed.starts_with("PASS ") {
                    continue;
                }
                out.push(line);
            }
        }
        _ => {
            out.extend_from_slice(&lines);
        }
    }

    let joined = out.join("\n");
    let deduped = deduplicate_repeated_lines(&joined);
    truncate_head_tail(&deduped, max_bytes)
}

fn deduplicate_repeated_lines(input: &str) -> String {
    let lines: Vec<&str> = input.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let mut count = 1;
        while i + count < lines.len() && lines[i + count] == lines[i] {
            count += 1;
        }
        if count >= 3 {
            out.push(lines[i].to_string());
            out.push(format!("[repeated {}x]", count));
        } else {
            for _ in 0..count {
                out.push(lines[i].to_string());
            }
        }
        i += count;
    }
    out.join("\n")
}
