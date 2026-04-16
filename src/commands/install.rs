use anyhow::{Context, Result};
use clap::{Args, CommandFactory};
use clap_complete::Shell;
use std::path::PathBuf;

use super::style::{dim, err, green, ok_mark};

/// Embedded SKILL.md content. Bundled into the binary at compile time
/// so `pixa install --skills` works without any external files.
const SKILL_MD: &str = include_str!("../../assets/skills/pixa/SKILL.md");

#[derive(Args)]
pub struct InstallArgs {
    /// Install the Claude Code skill into ~/.claude/skills/pixa/SKILL.md
    #[arg(long)]
    pub skills: bool,
    /// Install shell completions (auto-detects shell and install path)
    #[arg(long)]
    pub completions: bool,
    /// Overwrite the destination if it already exists
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: InstallArgs) -> Result<()> {
    if !args.skills && !args.completions {
        anyhow::bail!("nothing to install — pass --skills and/or --completions");
    }

    if args.skills {
        install_skills(args.force)?;
    }
    if args.completions {
        install_completions(args.force)?;
    }
    Ok(())
}

fn install_skills(force: bool) -> Result<()> {
    let target = skill_target_path()?;
    if target.exists() && !force {
        eprintln!(
            "{} skill already installed at {}",
            err::yellow("!"),
            err::green(&target.display().to_string())
        );
        eprintln!("  re-run with --force to overwrite");
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    std::fs::write(&target, SKILL_MD)
        .with_context(|| format!("Failed to write {}", target.display()))?;

    println!(
        "{} skill installed to {}",
        ok_mark(),
        green(&target.display().to_string())
    );
    println!(
        "  {}",
        dim("Claude Code and other coding agents can now use pixa automatically.")
    );
    Ok(())
}

fn install_completions(force: bool) -> Result<()> {
    let (shell, target) = detect_shell_and_path()?;

    if target.exists() && !force {
        eprintln!(
            "{} completions already installed at {}",
            err::yellow("!"),
            err::green(&target.display().to_string())
        );
        eprintln!("  re-run with --force to overwrite");
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }

    let mut cmd = crate::Cli::command();
    let mut buf = Vec::new();
    clap_complete::generate(shell, &mut cmd, "pixa", &mut buf);
    std::fs::write(&target, &buf).with_context(|| {
        format!(
            "Failed to write {}. \
             If the path is not writable, generate manually with: \
             pixa completions {} > <your-path>",
            target.display(),
            shell
        )
    })?;

    println!(
        "{} {} completions installed to {}",
        ok_mark(),
        shell,
        green(&target.display().to_string())
    );
    println!(
        "  {}",
        dim("Restart your shell or run `exec $SHELL` to activate.")
    );
    Ok(())
}

/// Auto-detect the current shell and the standard completion file path.
fn detect_shell_and_path() -> Result<(Shell, PathBuf)> {
    let shell_env = std::env::var("SHELL").unwrap_or_default();
    let shell = if shell_env.contains("zsh") {
        Shell::Zsh
    } else if shell_env.contains("bash") {
        Shell::Bash
    } else if shell_env.contains("fish") {
        Shell::Fish
    } else {
        anyhow::bail!(
            "Could not detect shell from $SHELL={shell_env}. \
             Use `pixa completions <shell>` to generate manually."
        );
    };

    let path = match shell {
        Shell::Zsh => {
            // Prefer Homebrew's site-functions (already in fpath)
            let brew = PathBuf::from("/opt/homebrew/share/zsh/site-functions/_pixa");
            let usr = PathBuf::from("/usr/local/share/zsh/site-functions/_pixa");
            if brew.parent().is_some_and(|p| p.exists()) {
                brew
            } else if usr.parent().is_some_and(|p| p.exists()) {
                usr
            } else {
                let home = home_dir()?;
                home.join(".zfunc/_pixa")
            }
        }
        Shell::Bash => {
            let home = home_dir()?;
            home.join(".local/share/bash-completion/completions/pixa")
        }
        Shell::Fish => {
            let home = home_dir()?;
            home.join(".config/fish/completions/pixa.fish")
        }
        _ => anyhow::bail!("Unsupported shell for auto-install"),
    };

    Ok((shell, path))
}

fn skill_target_path() -> Result<PathBuf> {
    let home = home_dir()?;
    Ok(home
        .join(".claude")
        .join("skills")
        .join("pixa")
        .join("SKILL.md"))
}

fn home_dir() -> Result<PathBuf> {
    if let Ok(h) = std::env::var("HOME")
        && !h.is_empty()
    {
        return Ok(PathBuf::from(h));
    }
    if let Ok(h) = std::env::var("USERPROFILE")
        && !h.is_empty()
    {
        return Ok(PathBuf::from(h));
    }
    eprintln!(
        "{} could not determine home directory (HOME / USERPROFILE not set)",
        err::fail_mark()
    );
    anyhow::bail!("home directory not found")
}
