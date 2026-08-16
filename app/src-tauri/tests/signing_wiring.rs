//! Guard tests for the Windows signing wiring (11-02-PLAN.md Task 3): the
//! committed Tauri config never carries a live signCommand, the signing
//! script/metadata/README exist where the release workflow expects them,
//! the workflow's injected path string agrees with the script's real
//! on-disk location, and every step that actually performs signing tooling
//! actions -- plus the public-Release-publish step -- is gated on the
//! repository enable variable.
//!
//! These are file-and-parse assertions, not behavioural tests: they never
//! require a genuinely signed artifact or live Azure credentials, neither
//! of which exist in this environment (must_haves.prohibitions). The
//! workflow inspection uses plain string handling, never a YAML dependency
//! (design_resolutions / must_haves.prohibitions -- no new Cargo crate).

#![allow(clippy::unwrap_used, clippy::expect_used)] // test-only code, not the archive-data path

use std::fs;
use std::path::{Path, PathBuf};

/// The Tauri project root -- where tauri.conf.json lives, and the working
/// directory Tauri's bundler invokes bundle.windows.signCommand from.
fn tauri_project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// The repository root, two levels above the Tauri project root
/// (<repo>/app/src-tauri -> <repo>).
fn repo_root() -> PathBuf {
    tauri_project_root()
        .parent()
        .expect("app dir")
        .parent()
        .expect("repo root")
        .to_path_buf()
}

/// The committed Tauri config never carries a live signCommand. A
/// locally-injected hook accidentally committed turns this test red --
/// this is the primary guard against T-11-08 (tampering: a live
/// sign-command entry accidentally committed).
#[test]
fn committed_config_has_no_windows_sign_hook() {
    let conf_path = tauri_project_root().join("tauri.conf.json");
    let raw = fs::read_to_string(&conf_path).expect("read tauri.conf.json");
    let json: serde_json::Value = serde_json::from_str(&raw).expect("parse tauri.conf.json");

    let windows = json.get("bundle").and_then(|b| b.get("windows"));

    if let Some(windows) = windows {
        assert!(
            windows.get("signCommand").is_none(),
            "committed tauri.conf.json must not carry a bundle.windows.signCommand entry -- \
             injection happens only in CI, into the workspace copy, never into the tracked file"
        );
    }
    // No `bundle.windows` object at all is also fine -- that is today's
    // committed state.
}

/// The signing script, its metadata, and its README all exist at the
/// expected location inside the Tauri project root.
#[test]
fn signing_script_and_metadata_present() {
    let dir = tauri_project_root().join("signing");
    for name in ["sign.ps1", "trusted-signing-metadata.json", "README.md"] {
        let path = dir.join(name);
        assert!(
            path.is_file(),
            "expected {name} to exist at {}",
            path.display()
        );
    }
}

/// The release workflow's injected sign-command path, read out of the
/// workflow file itself, resolves to the script's actual on-disk location
/// when interpreted relative to the Tauri project root. This is the
/// automated half of RESEARCH Pitfall 1: a path-depth mismatch between the
/// injected string and the script's real location would either fail loud
/// (acceptable) or silently resolve a different, stale script (dangerous).
#[test]
fn release_workflow_references_the_script_where_it_lives() {
    let workflow_path = repo_root().join(".github/workflows/release-app.yml");
    let workflow = fs::read_to_string(&workflow_path).expect("read release-app.yml");

    // Extract the injected signCommand string out of the workflow's own
    // `$signCmd = '...'` PowerShell assignment -- a targeted string match,
    // not a loose "signing" substring search (the workflow legitimately
    // mentions "signing" many times in comments and step names).
    let marker = "$signCmd = '";
    let start = workflow
        .find(marker)
        .expect("release-app.yml must set $signCmd")
        + marker.len();
    let rest = &workflow[start..];
    let end = rest
        .find('\'')
        .expect("terminating quote for $signCmd value");
    let sign_cmd = &rest[..end];

    // sign_cmd looks like: "powershell -ExecutionPolicy Bypass -File signing/sign.ps1 %1"
    let file_marker = "-File ";
    let file_start = sign_cmd
        .find(file_marker)
        .expect("signCommand must invoke the script via -File")
        + file_marker.len();
    let after_file = &sign_cmd[file_start..];
    let script_rel_path = after_file
        .split_whitespace()
        .next()
        .expect("a script path token must follow -File");

    assert_ne!(
        script_rel_path, "%1",
        "the -File argument must be the script's own path, not Tauri's artifact placeholder"
    );

    // Resolve relative to the Tauri project root -- exactly the working
    // directory Tauri's bundler invokes signCommand from.
    let resolved = tauri_project_root().join(script_rel_path);
    assert!(
        resolved.is_file(),
        "release workflow's injected script path '{script_rel_path}' does not resolve to a \
         real file relative to the Tauri project root ({}); resolved to {}",
        tauri_project_root().display(),
        resolved.display()
    );

    // Compare against the REAL script location, not a second hardcoded
    // constant that could independently be wrong in the same way (the
    // path-agreement acceptance criterion).
    let expected = tauri_project_root().join("signing").join("sign.ps1");
    assert_eq!(
        fs::canonicalize(&resolved).expect("canonicalize resolved path"),
        fs::canonicalize(&expected).expect("canonicalize expected path"),
        "the workflow's injected script path does not point at signing/sign.ps1"
    );
}

/// Returns the text of a workflow step's block: from its own `- name: ...`
/// line up to (but not including) the next step's `- name:` line, or the
/// end of the file. Every job's step list in release-app.yml is indented
/// at the same six-space depth, so this next-step-boundary search is valid
/// across job boundaries too.
fn step_block<'a>(workflow: &'a str, step_name_substr: &str) -> &'a str {
    let name_marker = format!("name: {step_name_substr}");
    let name_pos = workflow
        .find(&name_marker)
        .unwrap_or_else(|| panic!("step '{step_name_substr}' not found in release-app.yml"));

    let line_start = workflow[..name_pos].rfind('\n').map_or(0, |i| i + 1);
    let block_start = &workflow[line_start..];

    let after_own_line = block_start.find('\n').map_or(block_start.len(), |i| i + 1);
    let rest = &block_start[after_own_line..];

    let next_step_boundary = rest.find("\n      - name:").map_or(rest.len(), |i| i);
    &block_start[..after_own_line + next_step_boundary]
}

/// Every step that actually performs signing tooling actions -- the
/// Trusted Signing NuGet install step and the sign-command injection step
/// -- carries the enable-variable condition, so neither can ever run
/// ungated; the public-Release-publish job also carries that same
/// condition, so an unsigned build can never be published publicly.
///
/// Scoped by step NAME/purpose, not by substring-matching any mention of
/// "signing" in the workflow -- the Windows build step legitimately
/// carries the (inert-when-off) Azure env vars and is deliberately NOT
/// checked here; asserting it must be gated would wrongly fail a correct
/// workflow (per this task's <behavior> spec).
#[test]
fn release_workflow_gates_signing_steps() {
    let gate = "if: vars.ENABLE_MSI_SIGNING == 'true'";
    let workflow_path = repo_root().join(".github/workflows/release-app.yml");
    let workflow = fs::read_to_string(&workflow_path).expect("read release-app.yml");

    let install_block = step_block(&workflow, "Install Trusted Signing dlib");
    assert!(
        install_block.contains(gate),
        "the Trusted Signing dlib install step must carry `{gate}`"
    );

    let inject_block = step_block(&workflow, "Inject bundle.windows.signCommand");
    assert!(
        inject_block.contains(gate),
        "the sign-command injection step must carry `{gate}`"
    );

    // The publish job gates at the JOB level (`if:` directly under the job
    // id, before its steps), not per-step -- so locate the job block
    // itself rather than an individual step block.
    let publish_job_marker = "publish-release:";
    let publish_job_pos = workflow
        .find(publish_job_marker)
        .expect("publish-release job must exist in release-app.yml");
    let publish_step_marker = "name: Create GitHub Release";
    let publish_step_pos = workflow
        .find(publish_step_marker)
        .expect("publish-release job must have a Create GitHub Release step");
    assert!(
        publish_job_pos < publish_step_pos,
        "publish-release job header must precede its Create GitHub Release step"
    );
    let publish_job_block = &workflow[publish_job_pos..publish_step_pos];
    assert!(
        publish_job_block.contains(gate),
        "the publish-release job must carry `{gate}` gating public Release publishing on the \
         same condition as the signing steps"
    );
}
