//! Git and revision metadata.
//!
//! Git is used only for *metadata*: recording the current revision, and
//! matching stored results to revisions when selecting a baseline.
//! PyPerfGuard never checks out code on the user's behalf, so it cannot
//! clobber a working tree. All git invocations handle the absence of git, a
//! missing repository, and shallow clones gracefully.

use std::path::Path;
use std::process::Command;

use crate::error::{Error, Result};
use crate::results::RevisionInfo;

/// Run a git subcommand in `dir`, returning trimmed stdout on success.
fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| Error::io(format!("running git {}", args.join(" ")), e))?;
    if !output.status.success() {
        return Err(Error::NotFound(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Whether `dir` is inside a git work tree.
pub fn is_repo(dir: &Path) -> bool {
    git(dir, &["rev-parse", "--is-inside-work-tree"])
        .map(|s| s == "true")
        .unwrap_or(false)
}

/// Collect the current revision metadata. Returns default (all `None`) when
/// git is unavailable or `dir` is not a repository, rather than failing.
pub fn current_revision(dir: &Path) -> RevisionInfo {
    if !is_repo(dir) {
        return RevisionInfo::default();
    }
    let commit = git(dir, &["rev-parse", "HEAD"]).ok();
    // Detached HEAD is reported as "HEAD", which is not a branch name.
    let branch = git(dir, &["rev-parse", "--abbrev-ref", "HEAD"])
        .ok()
        .filter(|b| b != "HEAD");
    let dirty = git(dir, &["status", "--porcelain"])
        .ok()
        .map(|s| !s.is_empty());
    RevisionInfo {
        commit,
        branch,
        dirty,
    }
}

/// Abbreviate a commit SHA to seven characters.
pub fn short_sha(sha: &str) -> String {
    sha.chars().take(7).collect()
}

/// Resolve a git revision selector to a full SHA.
pub fn resolve(dir: &Path, rev: &str) -> Result<String> {
    git(dir, &["rev-parse", rev])
}

/// Whether `ancestor` is an ancestor of `descendant`. Returns `Ok(false)`
/// when the relationship cannot be determined (for example in a shallow
/// clone), so callers can degrade gracefully.
pub fn is_ancestor(dir: &Path, ancestor: &str, descendant: &str) -> Result<bool> {
    let result = Command::new("git")
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .current_dir(dir)
        .output()
        .map_err(|e| Error::io("running git merge-base", e))?;
    match result.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Ok(false),
    }
}

/// Whether the repository appears to be a shallow clone.
pub fn is_shallow(dir: &Path) -> bool {
    git(dir, &["rev-parse", "--is-shallow-repository"])
        .map(|s| s == "true")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_repo_returns_default_revision() {
        let tmp = tempfile::tempdir().unwrap();
        let rev = current_revision(tmp.path());
        assert_eq!(rev, RevisionInfo::default());
    }

    #[test]
    fn short_sha_truncates() {
        assert_eq!(short_sha("0123456789abcdef"), "0123456");
        assert_eq!(short_sha("abc"), "abc");
    }
}
