//! Makes `--version` name the commit a binary was built from when it is
//! not the tagged release, so a build from `main` (`cargo install --git`)
//! can be told from the release it follows, and a stale binary cannot pass
//! for a current one. No dirty-tree marker: nothing re-runs this script on
//! an uncommitted edit, so it would be wrong as often as right.

#[path = "build/build_id.rs"]
mod build_id;

use std::path::Path;
use std::process::{Command, Stdio};

fn main() {
    let version = env!("CARGO_PKG_VERSION");
    // With no rerun-if-changed at all, cargo re-runs this script whenever
    // any file in the package changes.
    println!("cargo:rerun-if-changed=build.rs");

    // Only this crate's own checkout (`.git` is a file in a worktree).
    // Without the check, git walks up to any repository around the crate,
    // a home directory under git or a project that vendors it, and reports
    // that one's HEAD.
    if !Path::new(".git").exists() {
        emit(version, None, &[]);
        return;
    }

    // Ahead of everything that can fail: a build that could not read git
    // has to run again once it can, or its bare version passes for the
    // release from then on.
    watch_refs(version);

    let Some(sha) = git(&["rev-parse", "--short", "HEAD"]).filter(|sha| !sha.is_empty()) else {
        println!(
            "cargo:warning=git could not read HEAD in this checkout; --version prints the bare version"
        );
        emit(version, None, &[]);
        return;
    };

    // `for-each-ref`, not `git tag`: with `column.ui=always`, `git tag`
    // prints several tags to a line even into a pipe. `strip=2`, not
    // `short`, which prints `tags/v1` when a branch has the same name.
    //
    // A failed query is not "no tags": on the release commit it would print
    // a sha. Keep the sha, which cannot pass for a release, and say so.
    let tags = git(&[
        "for-each-ref",
        "--points-at",
        "HEAD",
        "--format=%(refname:strip=2)",
        "refs/tags",
    ])
    .unwrap_or_else(|| {
        println!("cargo:warning=git could not list the tags at HEAD; --version names the commit");
        String::new()
    });
    let tags: Vec<&str> = tags.lines().collect();
    emit(version, Some(&sha), &tags);
}

fn emit(version: &str, sha: Option<&str>, tags_at_head: &[&str]) {
    println!(
        "cargo:rustc-env=CRATE_VERSION_WITH_BUILD={}",
        build_id::build_id(version, sha, tags_at_head)
    );
}

/// Re-run when the answer can have changed: a checkout (HEAD), a commit on
/// the checked-out branch, the release tag arriving or going, and a repack
/// (`packed-refs`, or `reftable` in a repo that uses that format).
///
/// The branch and the release tag are each watched as their own loose ref
/// file, so a commit on another branch or an unrelated tag, in this
/// worktree or any other, rebuilds nothing. While that file does not exist
/// (the ref is packed, or the tag is not made yet), the directory it will
/// be written under is watched instead. A path that does not exist is
/// never emitted, since cargo re-runs on every build for one.
fn watch_refs(version: &str) {
    let release_tag = format!("refs/tags/v{version}");
    // Empty on a detached HEAD, where a commit rewrites HEAD itself.
    let branch = git(&["symbolic-ref", "-q", "HEAD"]).unwrap_or_default();

    // `--git-path` resolves each path in a worktree too. `reftable` there
    // is the worktree's own stack; branches and tags go to the common
    // directory's, hence `--git-common-dir`.
    let mut query = vec!["rev-parse", "--git-common-dir"];
    let fixed = [
        "HEAD",
        "packed-refs",
        "reftable",
        "refs/heads",
        "refs/tags",
        release_tag.as_str(),
    ];
    for path in fixed.into_iter().chain([branch.as_str()]) {
        if !path.is_empty() {
            query.extend(["--git-path", path]);
        }
    }
    let resolved = git(&query).unwrap_or_default();
    let resolved: Vec<&str> = resolved.lines().collect();
    let [
        common,
        head,
        packed,
        reftable,
        heads_dir,
        tags_dir,
        tag_file,
        branch_file @ ..,
    ] = resolved.as_slice()
    else {
        // git cannot run here at all (not on PATH, or it refuses the
        // checkout). Nothing can be watched, so name a path that never
        // exists: cargo then runs this script on every build, and the
        // first one where git works gets the commit.
        println!("cargo:rerun-if-changed=.git/build-rs-could-not-run-git");
        return;
    };

    let loose_or = |file: &str, dir: &str| {
        if Path::new(file).exists() {
            file.to_string()
        } else {
            dir.to_string()
        }
    };
    let mut watch = vec![
        head.to_string(),
        packed.to_string(),
        reftable.to_string(),
        format!("{common}/reftable"),
        loose_or(tag_file, tags_dir),
    ];
    if let [branch_file] = branch_file {
        watch.push(loose_or(branch_file, heads_dir));
    }
    watch.sort();
    watch.dedup();
    for path in watch.iter().filter(|path| Path::new(path).exists()) {
        println!("cargo:rerun-if-changed={path}");
    }
}

/// Trimmed stdout of a git command that succeeded, empty when it printed
/// nothing; `None` when it failed.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(String::from_utf8(out.stdout).ok()?.trim().to_string())
}
