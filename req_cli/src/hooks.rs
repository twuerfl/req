//! Git hooks management for CI/Pre-commit integration.

use req_engine::Error;
use std::fs;
use std::io::Write;
use std::path::Path;

const PRE_COMMIT_HOOK: &str = r#"#!/bin/sh
# req pre-commit hook (advisory mode)
echo "Checking traceability..."
req check
exit 0
"#;

const PRE_COMMIT_HOOK_STRICT: &str = r#"#!/bin/sh
# req pre-commit hook (strict mode)
echo "Checking traceability (strict mode)..."
req check --strict
exit $?
"#;

/// Install pre-commit hook, locating `.git` by walking up from the current directory.
pub fn install_hook(strict: bool) -> Result<(), Error> {
    let git_dir = find_git_dir()?;
    install_hook_in(git_dir.parent().unwrap_or(&git_dir), strict)
}

// REQ: LLR-0010, LLR-0011
/// Install pre-commit hook inside `base/.git/hooks/`.
///
/// Accepts an explicit base path for testability.
pub fn install_hook_in(base: &Path, strict: bool) -> Result<(), Error> {
    let hooks_dir = base.join(".git").join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    let hook_path = hooks_dir.join("pre-commit");

    if hook_path.exists() {
        return Err(Error::Config(
            "pre-commit hook already exists. Remove it first.".to_string(),
        ));
    }

    let content = if strict {
        PRE_COMMIT_HOOK_STRICT
    } else {
        PRE_COMMIT_HOOK
    };

    let mut file = fs::File::create(&hook_path)?;
    file.write_all(content.as_bytes())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755))?;
    }

    println!("✓ Installed pre-commit hook at {}", hook_path.display());
    if strict {
        println!("  Mode: strict (will block commits with issues)");
    } else {
        println!("  Mode: advisory (warnings only, commits allowed)");
    }

    Ok(())
}

/// Remove pre-commit hook, locating `.git` by walking up from the current directory.
pub fn uninstall_hook() -> Result<(), Error> {
    let git_dir = find_git_dir()?;
    uninstall_hook_in(git_dir.parent().unwrap_or(&git_dir))
}

/// Remove pre-commit hook from `base/.git/hooks/pre-commit`.
///
/// Accepts an explicit base path for testability.
pub fn uninstall_hook_in(base: &Path) -> Result<(), Error> {
    let hook_path = base.join(".git").join("hooks").join("pre-commit");

    if !hook_path.exists() {
        println!("No pre-commit hook found");
        return Ok(());
    }

    let content = fs::read_to_string(&hook_path)?;
    if !content.contains("req check") {
        return Err(Error::Config(
            "pre-commit hook is not managed by req".to_string(),
        ));
    }

    fs::remove_file(&hook_path)?;
    println!("✓ Removed pre-commit hook");

    Ok(())
}

fn find_git_dir() -> Result<std::path::PathBuf, Error> {
    let mut current = std::env::current_dir()?;

    loop {
        let git_dir = current.join(".git");
        if git_dir.exists() {
            return Ok(git_dir);
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => return Err(Error::Config("Not a git repository".to_string())),
        }
    }
}

/// Generate CI workflow files
pub fn generate_ci_workflow(ci_type: &str, output_dir: &Path) -> Result<(), Error> {
    match ci_type {
        "github" | "gh" => generate_github_actions(output_dir),
        "gitlab" => generate_gitlab_ci(output_dir),
        _ => Err(Error::Config(format!(
            "Unknown CI type: {}. Supported: github, gitlab",
            ci_type
        ))),
    }
}

fn generate_github_actions(output_dir: &Path) -> Result<(), Error> {
    let workflow_dir = output_dir.join(".github").join("workflows");
    fs::create_dir_all(&workflow_dir)?;

    let workflow = r#"name: Traceability Check

on:
  pull_request:
    branches: [main, release/*]
  push:
    branches: [main, release/*]

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install req
        run: |
          curl -sSL https://github.com/twuerfl/req/releases/latest/download/req-linux-x86_64 -o req
          chmod +x req
          sudo mv req /usr/local/bin/

      - name: Check traceability
        run: req check --strict
"#;

    let workflow_path = workflow_dir.join("traceability.yml");
    fs::write(&workflow_path, workflow)?;
    println!(
        "✓ Generated GitHub Actions workflow at {}",
        workflow_path.display()
    );
    Ok(())
}

fn generate_gitlab_ci(output_dir: &Path) -> Result<(), Error> {
    let workflow = r#"# Traceability Check — add to your .gitlab-ci.yml

traceability:
  stage: test
  script:
    - curl -sSL https://github.com/twuerfl/req/releases/latest/download/req-linux-x86_64 -o req
    - chmod +x req
    - ./req check --strict
  rules:
    - if: $CI_COMMIT_BRANCH == "main"
    - if: $CI_COMMIT_BRANCH =~ /^release\//
"#;

    let workflow_path = output_dir.join("traceability.gitlab-ci.yml");
    fs::write(&workflow_path, workflow)?;
    println!(
        "✓ Generated GitLab CI snippet at {}",
        workflow_path.display()
    );
    println!("  Include this in your .gitlab-ci.yml");
    Ok(())
}
