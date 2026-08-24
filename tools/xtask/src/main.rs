use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const REQUIRED_FILES: &[&str] = &[
    "README.md",
    "AGENTS.md",
    "ARCHITECTURE.md",
    "TESTING.md",
    "LICENSE",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    ".cargo/config.toml",
    ".github/workflows/validate.yml",
    "tools/xtask/Cargo.toml",
    "tools/xtask/src/main.rs",
];

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (Some("validate"), None) => match validate() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("validation failed: {error}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("usage: cargo validate");
            ExitCode::FAILURE
        }
    }
}

fn validate() -> Result<(), String> {
    let root = repository_root()?;
    let before = git_status(&root)?;

    check_required_files(&root)?;
    check_markdown_links(&root)?;
    check_projects(&root)?;

    run(
        &root,
        "cargo",
        &["metadata", "--locked", "--format-version=1", "--no-deps"],
    )?;
    run(&root, "cargo", &["fmt", "--all", "--", "--check"])?;
    run(
        &root,
        "cargo",
        &["test", "--workspace", "--all-targets", "--locked"],
    )?;
    run(
        &root,
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    run(&root, "git", &["diff", "--check"])?;

    let after = git_status(&root)?;
    if before != after {
        return Err("validation changed the checkout state".into());
    }

    println!("Runen Lab validation passed");
    Ok(())
}

fn repository_root() -> Result<PathBuf, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|error| format!("failed to invoke git: {error}"))?;
    if !output.status.success() {
        return Err("not inside a Git repository".into());
    }
    let root = String::from_utf8(output.stdout)
        .map_err(|_| "git returned a non-UTF-8 repository path".to_string())?;
    Ok(PathBuf::from(root.trim()))
}

fn git_status(root: &Path) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .output()
        .map_err(|error| format!("failed to inspect checkout state: {error}"))?;
    if !output.status.success() {
        return Err("git status failed".into());
    }
    Ok(output.stdout)
}

fn check_required_files(root: &Path) -> Result<(), String> {
    for path in REQUIRED_FILES {
        if !root.join(path).is_file() {
            return Err(format!("required repository file is missing: {path}"));
        }
    }
    Ok(())
}

fn check_markdown_links(root: &Path) -> Result<(), String> {
    let mut markdown = Vec::new();
    collect_markdown(root, &mut markdown)?;
    for file in markdown {
        let text = fs::read_to_string(&file)
            .map_err(|error| format!("failed to read {}: {error}", file.display()))?;
        for target in inline_link_targets(&text) {
            if target.is_empty()
                || target.starts_with('#')
                || target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
            {
                continue;
            }
            let path = target.split('#').next().unwrap_or(target);
            if path.is_empty() {
                continue;
            }
            let resolved = file.parent().unwrap_or(root).join(path);
            if !resolved.exists() {
                return Err(format!(
                    "broken repository-relative Markdown link in {}: {target}",
                    file.strip_prefix(root).unwrap_or(&file).display()
                ));
            }
        }
    }
    Ok(())
}

fn collect_markdown(dir: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir)
        .map_err(|error| format!("failed to read {}: {error}", dir.display()))?
    {
        let entry = entry.map_err(|error| format!("failed to read directory entry: {error}"))?;
        let path = entry.path();
        if path
            .file_name()
            .is_some_and(|name| name == ".git" || name == "target")
        {
            continue;
        }
        if path.is_dir() {
            collect_markdown(&path, output)?;
        } else if path.extension().is_some_and(|extension| extension == "md") {
            output.push(path);
        }
    }
    Ok(())
}

fn inline_link_targets(text: &str) -> Vec<&str> {
    let mut targets = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find("](") {
        rest = &rest[open + 2..];
        let Some(close) = rest.find(')') else {
            break;
        };
        targets.push(rest[..close].trim());
        rest = &rest[close + 1..];
    }
    targets
}

fn check_projects(root: &Path) -> Result<(), String> {
    let projects = root.join("projects");
    if !projects.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(&projects)
        .map_err(|error| format!("failed to read projects/: {error}"))?
    {
        let entry = entry.map_err(|error| format!("failed to read project entry: {error}"))?;
        let project = entry.path();
        if !project.is_dir() {
            continue;
        }

        let manifest = project.join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        if !project.join("Cargo.lock").is_file() {
            return Err(format!(
                "maintained Rust project has no committed Cargo.lock: {}",
                project.strip_prefix(root).unwrap_or(&project).display()
            ));
        }

        let manifest_text = fs::read_to_string(&manifest)
            .map_err(|error| format!("failed to read {}: {error}", manifest.display()))?;
        check_dependency_policy(root, &manifest, &manifest_text)?;
        validate_rust_project(&project)?;
    }

    Ok(())
}

fn check_dependency_policy(root: &Path, manifest: &Path, text: &str) -> Result<(), String> {
    let display = manifest.strip_prefix(root).unwrap_or(manifest).display();
    let mut table_git_without_rev = false;

    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') {
            if table_git_without_rev {
                return Err(format!(
                    "maintained Lab project uses an unpinned Git dependency: {display}"
                ));
            }
            table_git_without_rev = false;
            continue;
        }

        if line.contains("path =") {
            return Err(format!(
                "maintained Lab project uses a path dependency: {display}"
            ));
        }
        if line.contains("branch =") {
            return Err(format!(
                "maintained Lab project uses a moving branch dependency: {display}"
            ));
        }

        if line.starts_with("git =") {
            table_git_without_rev = true;
        } else if line.contains("git =") && !line.contains("rev =") {
            return Err(format!(
                "maintained Lab project uses an unpinned Git dependency: {display}"
            ));
        }

        if line.starts_with("rev =") {
            table_git_without_rev = false;
        }
    }

    if table_git_without_rev {
        return Err(format!(
            "maintained Lab project uses an unpinned Git dependency: {display}"
        ));
    }

    Ok(())
}

fn validate_rust_project(project: &Path) -> Result<(), String> {
    run(
        project,
        "cargo",
        &["metadata", "--locked", "--format-version=1", "--no-deps"],
    )?;
    run(project, "cargo", &["fmt", "--all", "--", "--check"])?;
    run(
        project,
        "cargo",
        &["test", "--workspace", "--all-targets", "--locked"],
    )?;
    run(
        project,
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    Ok(())
}

fn run(root: &Path, program: &str, args: &[&str]) -> Result<(), String> {
    println!("+ {program} {}", args.join(" "));
    let status = Command::new(program)
        .current_dir(root)
        .args(args)
        .status()
        .map_err(|error| format!("failed to invoke {program}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("command failed: {program} {}", args.join(" ")))
    }
}
