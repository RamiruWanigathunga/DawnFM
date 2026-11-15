//! Tier 2: F01 - Git Feature Branch Boundary & Extreme Cases
//!
//! Validates detached HEAD detection, merge commit discipline, non-ASCII commit message parsing,
//! shallow clone compatibility, and submodule cleanliness per ORIGINAL_REQUEST §R7.

use std::process::Command;

/// Parses a raw git HEAD ref or symbolic ref output into an enum representing the HEAD state.
#[derive(Debug, PartialEq)]
pub enum HeadState {
    Branch(String),
    Detached(String),
    Corrupt,
}

pub fn parse_head_reference(raw_ref: &str, raw_commit: &str) -> HeadState {
    let trimmed_ref = raw_ref.trim();
    let trimmed_commit = raw_commit.trim();

    if trimmed_commit.is_empty() && trimmed_ref.is_empty() {
        return HeadState::Corrupt;
    }

    if trimmed_ref.starts_with("refs/heads/") {
        HeadState::Branch(trimmed_ref.trim_start_matches("refs/heads/").to_string())
    } else if trimmed_ref == "HEAD" || trimmed_ref.is_empty() {
        if trimmed_commit.len() >= 7 && trimmed_commit.chars().all(|c| c.is_ascii_hexdigit()) {
            HeadState::Detached(trimmed_commit.to_string())
        } else {
            HeadState::Corrupt
        }
    } else {
        HeadState::Branch(trimmed_ref.to_string())
    }
}

/// Validates whether a commit message contains forbidden merge conflict markers or corrupt headers.
pub fn validate_commit_hygiene(msg: &str) -> Result<(), &'static str> {
    if msg.is_empty() {
        return Err("Empty commit message");
    }
    if msg.contains("<<<<<<<") || msg.contains("=======") || msg.contains(">>>>>>>") {
        return Err("Commit contains merge conflict markers");
    }
    if msg.starts_with("Merge branch '") && msg.contains("into") {
        return Err("Raw merge commits without descriptive context are forbidden on feature branch");
    }
    Ok(())
}

#[test]
fn test_git_detached_head_detection() {
    // 1. Test simulated state transitions
    assert_eq!(
        parse_head_reference("refs/heads/feat/production-v2.3", "a1b2c3d"),
        HeadState::Branch("feat/production-v2.3".into())
    );
    assert_eq!(
        parse_head_reference("", "a1b2c3d4e5f67890"),
        HeadState::Detached("a1b2c3d4e5f67890".into())
    );
    assert_eq!(
        parse_head_reference("HEAD", "a1b2c3d4e5f6"),
        HeadState::Detached("a1b2c3d4e5f6".into())
    );
    assert_eq!(
        parse_head_reference("", "not_a_hex_hash"),
        HeadState::Corrupt
    );
    assert_eq!(parse_head_reference("", ""), HeadState::Corrupt);

    // 2. Query live repository HEAD state
    let sym_output = Command::new("git")
        .args(["symbolic-ref", "-q", "HEAD"])
        .output()
        .expect("git symbolic-ref must execute");

    let rev_output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git rev-parse must execute");

    let raw_ref = String::from_utf8_lossy(&sym_output.stdout);
    let raw_commit = String::from_utf8_lossy(&rev_output.stdout);

    let state = parse_head_reference(&raw_ref, &raw_commit);
    // On a valid checkout (branch or CI detached HEAD), state must be valid
    assert!(
        matches!(state, HeadState::Branch(_) | HeadState::Detached(_)),
        "HEAD must be attached to a valid branch or detached commit, got: {:?}",
        state
    );
}

#[test]
fn test_git_merge_commit_tag_discipline() {
    // Test validation helper against malformed messages
    assert!(validate_commit_hygiene("<<<<<<< HEAD\nfoo\n=======\nbar\n>>>>>>>").is_err());
    assert!(validate_commit_hygiene("Merge branch 'main' into feat/production-v2.3").is_err());
    assert!(validate_commit_hygiene("").is_err());
    assert!(validate_commit_hygiene("feat(audio): tune window buffer to 12MB").is_ok());

    // Inspect recent commits in repository
    let output = Command::new("git")
        .args(["log", "-n", "20", "--format=%s"])
        .output()
        .expect("git log must execute");

    assert!(output.status.success());
    let log = String::from_utf8_lossy(&output.stdout);
    for line in log.lines() {
        if !line.is_empty() {
            assert!(
                validate_commit_hygiene(line).is_ok(),
                "Commit '{}' violates git hygiene conventions",
                line
            );
        }
    }
}

#[test]
fn test_git_non_ascii_commit_messages() {
    let non_ascii_samples = [
        "feat(i18n): add Español and Français dictionaries 🌍",
        "fix(ui): handle RTL text direction for العربية and עברית",
        "perf(mem): optimize karaoke blur pooling 🚀 • 64px buckets",
        "chore(deps): update Japanese (日本語) translation tokens",
        "refactor(stream): buffer sliding window: 12MB boundary ±1B",
    ];

    for sample in non_ascii_samples {
        // Ensure byte length is greater than char count (multi-byte UTF-8)
        assert!(sample.len() > sample.chars().count());
        // Ensure character-safe slicing
        let truncated: String = sample.chars().take(20).collect();
        assert!(!truncated.is_empty());
        assert!(validate_commit_hygiene(sample).is_ok());
    }
}

#[test]
fn test_git_shallow_clone_compatibility() {
    let output = Command::new("git")
        .args(["rev-parse", "--is-shallow-repository"])
        .output()
        .expect("git rev-parse must execute");

    assert!(output.status.success());
    let is_shallow_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert!(
        is_shallow_str == "true" || is_shallow_str == "false",
        "--is-shallow-repository must output boolean string"
    );

    // Verify git can traverse at least 1 commit even in shallow clones
    let log_output = Command::new("git")
        .args(["rev-list", "--max-count=1", "HEAD"])
        .output()
        .expect("rev-list max-count 1 must execute");

    assert!(log_output.status.success());
    let commit_hash = String::from_utf8_lossy(&log_output.stdout).trim().to_string();
    assert_eq!(commit_hash.len(), 40, "Commit hash must be 40 hex chars");
}

#[test]
fn test_git_submodule_cleanliness() {
    // If .gitmodules exists, ensure it is non-empty and well-formed
    let gitmodules_path = std::path::Path::new(".gitmodules");
    if gitmodules_path.exists() {
        let content = std::fs::read_to_string(gitmodules_path).expect(".gitmodules must be readable");
        assert!(
            content.contains("[submodule") && content.contains("path ="),
            ".gitmodules must be well-formed"
        );
    }

    // Git status should report no uncommitted submodule changes or conflicts
    let status_output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .expect("git status must execute");

    assert!(status_output.status.success());
    let status_text = String::from_utf8_lossy(&status_output.stdout);
    for line in status_text.lines() {
        assert!(
            !line.starts_with("UU ") && !line.starts_with("AA "),
            "Repository working tree contains unmerged conflict path: {}",
            line
        );
    }
}
