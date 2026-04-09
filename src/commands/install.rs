use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

use super::style::{dim, fail_mark, green, ok_mark, yellow};

/// Embedded SKILL.md content. Bundled into the binary at compile time
/// so `pixa install --skills` works without any external files.
const SKILL_MD: &str = include_str!("../../assets/skills/pixa/SKILL.md");

#[derive(Args)]
pub struct InstallArgs {
    /// Install the Claude Code skill into ~/.claude/skills/pixa/SKILL.md
    /// so coding agents (Claude Code, etc.) can use pixa automatically.
    #[arg(long)]
    pub skills: bool,
    /// Overwrite the destination if it already exists
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: InstallArgs) -> Result<()> {
    if !args.skills {
        anyhow::bail!("nothing to install — pass --skills");
    }

    let target = skill_target_path()?;
    if target.exists() && !args.force {
        eprintln!(
            "{} already installed at {}",
            yellow("!"),
            green(&target.display().to_string())
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

fn skill_target_path() -> Result<PathBuf> {
    let home = home_dir()?;
    Ok(home.join(".claude").join("skills").join("pixa").join("SKILL.md"))
}

fn home_dir() -> Result<PathBuf> {
    if let Ok(h) = std::env::var("HOME") {
        if !h.is_empty() {
            return Ok(PathBuf::from(h));
        }
    }
    if let Ok(h) = std::env::var("USERPROFILE") {
        if !h.is_empty() {
            return Ok(PathBuf::from(h));
        }
    }
    eprintln!(
        "{} could not determine home directory (HOME / USERPROFILE not set)",
        fail_mark()
    );
    anyhow::bail!("home directory not found")
}
