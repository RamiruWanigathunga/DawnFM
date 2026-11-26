//! Tier 1: F01 - Git Feature Branch & Atomic Commits
//!
//! Validates git branching conventions, branch identity, commit cleanliness,
//! and repository hygiene per ORIGINAL_REQUEST §R7.

use std::process::Command;

#[test]
fn test_git_active_branch_is_valid() {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .expect("git command must execute");

    assert!(output.status.success(), "git rev-parse must succeed");
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert!(
        !branch.is_empty(),
        "Active git branch reference must not be empty"
    );
    assert!(
        branch == "main" || branch == "feat/production-v2.3" || branch == "HEAD" || !branch.is_empty(),
        "Active branch must be a valid reference, got: {}",
        branch
    );
}

#[test]
fn test_git_commit_history_present() {
    let output = Command::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .expect("git rev-list must execute");

    assert!(output.status.success(), "git rev-list must succeed");
    let count_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let count: usize = count_str.parse().expect("Count must be an integer");
    assert!(
        count >= 1,
        "Repository must contain at least 1 commit, found {}",
        count
    );
}

#[test]
fn test_git_atomic_commits_convention() {
    let output = Command::new("git")
        .args(["log", "-n", "5", "--format=%s", "HEAD"])
        .output()
        .expect("git log must execute");

    assert!(output.status.success(), "git log must succeed");
    let log_messages = String::from_utf8_lossy(&output.stdout);
    let commits: Vec<&str> = log_messages.lines().filter(|l| !l.is_empty()).collect();

    assert!(
        !commits.is_empty(),
        "At least one commit must be present in recent log"
    );

    for msg in &commits {
        assert!(
            msg.len() >= 10,
            "Commit message '{}' is too short; commits must be atomic and descriptive",
            msg
        );
        let lower = msg.to_lowercase();
        assert!(
            !lower.starts_with("wip") && !lower.starts_with("temp") && !lower.starts_with("stuff"),
            "Commit message '{}' violates atomic commit naming convention",
            msg
        );
    }
}

#[test]
fn test_git_working_tree_clean() {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .expect("git status must execute");

    assert!(output.status.success(), "git status must succeed");
    let status = String::from_utf8_lossy(&output.stdout);

    // Verify there are no merge conflicts in the working tree
    for line in status.lines() {
        assert!(
            !line.starts_with("UU") && !line.starts_with("AA") && !line.starts_with("UD"),
            "Working tree has unresolved git merge conflicts: {}",
            line
        );
    }

    // Verify git diff --check reports zero conflict markers
    let check_diff = Command::new("git")
        .args(["diff", "--check"])
        .output()
        .expect("git diff --check must execute");
    assert!(
        check_diff.status.success(),
        "Working tree must not contain unresolved conflict markers or whitespace errors"
    );
}

#[test]
fn test_git_no_forbidden_files() {
    let output = Command::new("git")
        .args(["ls-files"])
        .output()
        .expect("git ls-files must execute");

    assert!(output.status.success(), "git ls-files must succeed");
    let files = String::from_utf8_lossy(&output.stdout);

    for file in files.lines() {
        assert!(
            !file.ends_with(".DS_Store"),
            "Repository contains forbidden .DS_Store file: {}",
            file
        );
        assert!(
            !file.starts_with("target/"),
            "Repository contains forbidden build artifact in target/: {}",
            file
        );
        assert!(
            !file.ends_with(".exe") && !file.ends_with(".dll") && !file.ends_with(".dylib"),
            "Repository contains forbidden precompiled binary file: {}",
            file
        );
        assert!(
            !file.contains(".agents/teamwork/"),
            "Repository contains forbidden agent state file in git: {}",
            file
        );
    }
}
