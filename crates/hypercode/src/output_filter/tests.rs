use super::*;
use crate::cli::CliOptions;
use crate::tool::{Tool, ToolContext};
use crate::tools::bash::BashTool;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_strip_noise_ansi_escapes() {
    let input = "\x1b[31merror\x1b[0m: something failed\n\x1b[1mbold\x1b[0m text";
    let cleaned = rules::strip_noise(input);
    assert!(!cleaned.contains('\x1b'));
    assert!(cleaned.contains("error: something failed"));
    assert!(cleaned.contains("bold text"));
}

#[test]
fn test_strip_noise_progress_bar_overwrite() {
    let input = "Loading... 10%\rLoading... 50%\rLoading... 100%\r\nDone";
    let cleaned = rules::strip_noise(input);
    assert!(cleaned.contains("Loading... 100%"));
    assert!(!cleaned.contains("Loading... 10%"));
}

#[test]
fn test_strip_noise_spinner_lines() {
    let input = "Starting\n⠋\n⠙\n⠹\nComplete\n";
    let cleaned = rules::strip_noise(input);
    assert!(!cleaned.contains('⠋'));
    assert!(!cleaned.contains('⠙'));
    assert!(cleaned.contains("Starting"));
    assert!(cleaned.contains("Complete"));
}

#[test]
fn test_strip_noise_collapse_blank_lines() {
    let input = "line1\n\n\n\n\nline2\n\nline3";
    let cleaned = rules::strip_noise(input);
    let lines: Vec<&str> = cleaned.lines().collect();
    let blank_count = lines.iter().filter(|l| l.trim().is_empty()).count();
    assert!(
        blank_count <= 2,
        "Expected at most 2 blank lines, got {}",
        blank_count
    );
}

#[test]
fn test_should_bypass_aggressive_filter() {
    assert!(rules::should_bypass_aggressive_filter(
        "cargo test --nocapture"
    ));
    assert!(rules::should_bypass_aggressive_filter(
        "cargo test --verbose"
    ));
    assert!(rules::should_bypass_aggressive_filter("cargo test -vv"));
    assert!(rules::should_bypass_aggressive_filter("ls | grep foo"));
    assert!(rules::should_bypass_aggressive_filter("echo hi > out.txt"));
    assert!(rules::should_bypass_aggressive_filter("echo hi >> out.txt"));
    assert!(!rules::should_bypass_aggressive_filter("cargo test"));
    assert!(!rules::should_bypass_aggressive_filter("ls -la"));
}

#[test]
fn test_detect_rule() {
    assert_eq!(rules::detect_rule("cargo test"), "cargo-test");
    assert_eq!(rules::detect_rule("cargo nextest run"), "cargo-test");
    assert_eq!(
        rules::detect_rule("CARGO_TARGET_DIR=/tmp/x cargo nextest run"),
        "cargo-test"
    );
    assert_eq!(
        rules::detect_rule("CARGO_TARGET_DIR=/tmp/x cargo clippy"),
        "cargo-check"
    );
    assert_eq!(rules::detect_rule("cargo clippy"), "cargo-check");
    assert_eq!(rules::detect_rule("cargo check"), "cargo-check");
    assert_eq!(rules::detect_rule("git status"), "git-status");
    assert_eq!(rules::detect_rule("GIT_PAGER=cat git diff"), "git-diff");
    assert_eq!(rules::detect_rule("uv run pytest"), "test-runner");
    assert_eq!(rules::detect_rule("bun test"), "test-runner");
    assert_eq!(rules::detect_rule("bun install"), "generic");
    assert_eq!(rules::detect_rule("jest"), "test-runner");
    assert_eq!(rules::detect_rule("ls -la"), "generic");
}

#[test]
fn test_cargo_test_filter() {
    let input = "Compiling foo v0.1.0\nChecking bar v0.1.0\nFinished test [unoptimized]\ntest foo::test_ok ... ok\ntest foo::test_fail ... FAILED\nfailures:\n---- foo::test_fail stdout ----\nthread 'foo::test_fail' panicked at 'assertion failed'\nfailures:\n    foo::test_fail\ntest result: FAILED. 1 passed; 1 failed\n";
    let filtered = rules::apply_rule("cargo-test", input, 1024);
    assert!(!filtered.contains("Compiling"));
    assert!(!filtered.contains("Checking"));
    assert!(!filtered.contains("Finished"));
    assert!(!filtered.contains("test foo::test_ok ... ok"));
    assert!(filtered.contains("failures:"));
    assert!(filtered.contains("---- foo::test_fail stdout ----"));
    assert!(filtered.contains("panicked"));
    assert!(filtered.contains("test result: FAILED"));
}

#[test]
fn test_cargo_check_filter() {
    let input = "Compiling foo v0.1.0\nFinished dev [unoptimized]\nerror[E0425]: cannot find value `x`\n --> src/main.rs:2:5\n  |\n2 |     let y = x;\n  |             ^ not found\nhelp: consider importing\nnote: this is a note\nnote: this is a note\n";
    let filtered = rules::apply_rule("cargo-check", input, 1024);
    assert!(!filtered.contains("Compiling"));
    assert!(!filtered.contains("Finished"));
    assert!(filtered.contains("error[E0425]"));
    assert!(filtered.contains("--> src/main.rs:2:5"));
    let note_count = filtered.matches("note: this is a note").count();
    assert_eq!(
        note_count, 1,
        "Expected deduplicated note, got {}",
        note_count
    );
}

#[test]
fn test_git_status_filter() {
    let input = "On branch main\n(use \"git add <file>\" to include)\n(use \"git commit\" to commit)\nChanges not staged:\n  modified: file.txt\n";
    let filtered = rules::apply_rule("git-status", input, 1024);
    assert!(!filtered.contains("(use \"git"));
    assert!(filtered.contains("On branch main"));
    assert!(filtered.contains("modified: file.txt"));
}

#[test]
fn test_git_diff_filter() {
    let input = "diff --git a/file.txt b/file.txt\nindex 123..456 100644\n--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,4 @@\n line1\n-old line\n+new line\n line3\n";
    let filtered = rules::apply_rule("git-diff", input, 1024);
    assert!(filtered.contains("diff --git"));
    assert!(filtered.contains("--- a/file.txt"));
    assert!(filtered.contains("+++ b/file.txt"));
    assert!(filtered.contains("@@ -1,3 +1,4 @@"));
    assert!(filtered.contains("-old line"));
    assert!(filtered.contains("+new line"));
}

#[test]
fn test_test_runner_filter() {
    let input = "test_foo PASSED\ntest_bar ✓\ntest_baz FAILED\nAssertionError: 1 != 2\n  at test_baz (test.js:5)\n\n3 tests, 2 passed, 1 failed\n";
    let filtered = rules::apply_rule("test-runner", input, 1024);
    assert!(!filtered.contains("test_foo PASSED"));
    assert!(!filtered.contains("test_bar ✓"));
    assert!(filtered.contains("test_baz FAILED"));
    assert!(filtered.contains("AssertionError"));
    assert!(filtered.contains("3 tests, 2 passed, 1 failed"));
}

#[test]
fn test_generic_filter_repeated_lines() {
    let input = "same line\nsame line\nsame line\nsame line\ndifferent\n";
    let filtered = rules::apply_rule("generic", input, 1024);
    assert!(filtered.contains("[repeated 4x]"));
    assert!(filtered.contains("different"));
}

#[test]
fn test_truncate_head_tail_utf8() {
    let input = "Hello 世界 🌍\n".repeat(100);
    let truncated = rules::truncate_head_tail(&input, 50);
    assert!(truncated.len() <= 50 || truncated.contains("[truncated"));
    assert!(truncated.starts_with("Hello"));
    assert!(truncated.ends_with("🌍\n"));
}

#[test]
fn test_tee_on_failure() {
    let tmp = TempDir::new().unwrap();
    let filter = OutputFilter::new(OutputFilterConfig::default());
    let outcome = filter.filter_and_tee(
        "failing_cmd",
        1,
        "stdout content",
        "stderr content",
        tmp.path(),
    );
    assert!(outcome.tee_log_path.is_some());
    let log_path = outcome.tee_log_path.unwrap();
    assert!(log_path.exists());
    let log_content = fs::read_to_string(&log_path).unwrap();
    assert!(log_content.contains("stdout content"));
    assert!(log_content.contains("stderr content"));
}

#[test]
fn test_tee_on_high_compression() {
    let tmp = TempDir::new().unwrap();
    let filter = OutputFilter::new(OutputFilterConfig::default());
    let verbose = "Compiling foo\nChecking bar\nFinished\n".repeat(100);
    let outcome = filter.filter_and_tee("cargo test", 0, &verbose, "", tmp.path());
    assert!(outcome.tee_log_path.is_some());
    assert!(outcome.stats.estimated_tokens_saved > 0);
}

#[test]
fn test_log_rotation() {
    let tmp = TempDir::new().unwrap();
    let config = OutputFilterConfig {
        max_log_files: 3,
        ..Default::default()
    };
    let filter = OutputFilter::new(config);
    for i in 0..5 {
        let _ = filter.filter_and_tee(&format!("cmd{}", i), 1, "output", "", tmp.path());
    }
    let log_dir = tmp.path().join(".hypercode/logs");
    let log_files: Vec<_> = fs::read_dir(&log_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "log"))
        .collect();
    assert!(
        log_files.len() <= 3,
        "Expected at most 3 log files, got {}",
        log_files.len()
    );
}

#[test]
fn test_session_filter_stats() {
    let stats = SessionFilterStats::default();
    stats.record(&FilterStats {
        raw_bytes: 1000,
        filtered_bytes: 500,
        estimated_tokens_saved: 125,
        rule_name: "cargo-test".to_string(),
    });
    stats.record(&FilterStats {
        raw_bytes: 2000,
        filtered_bytes: 1000,
        estimated_tokens_saved: 250,
        rule_name: "generic".to_string(),
    });
    let snapshot = stats.snapshot();
    assert_eq!(snapshot.commands_filtered, 2);
    assert_eq!(snapshot.total_raw_bytes, 3000);
    assert_eq!(snapshot.total_filtered_bytes, 1500);
    assert_eq!(snapshot.total_tokens_saved, 375);
}

#[tokio::test]
async fn test_bash_tool_filtering_and_tee() {
    let tmp = TempDir::new().unwrap();
    let ctx = ToolContext::new(tmp.path());
    let tool = BashTool;
    let args = serde_json::json!({
        "command": "echo error output; exit 1",
        "timeout_secs": 5
    });
    let output = tool.execute(args, &ctx).await;
    assert!(output.is_error);
    let snapshot = ctx.filter_stats.snapshot();
    assert_eq!(snapshot.commands_filtered, 1);
    assert!(output.content.contains("Exit code: 1"));
    assert!(output.content.contains(".hypercode/logs/"));
}

#[test]
fn test_cli_use_rtk_flag() {
    let opts = CliOptions::parse_from(vec!["--use-rtk"]).unwrap();
    assert!(opts.use_rtk);
    assert!(opts.prompt.is_none());
}
