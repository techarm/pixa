use anyhow::Result;
use clap::Args;
use pixa::favicon::{generate_favicon_set, FaviconOptions};
use std::path::PathBuf;

use super::format_size;
use super::style::{bold, dim, green, ok_mark};

#[derive(Args)]
pub struct FaviconArgs {
    /// Input image file
    pub input: PathBuf,
    /// Output directory for the icon set
    #[arg(short, long, default_value = "favicon-output")]
    pub output_dir: PathBuf,
    /// PNG optimization level (0-6)
    #[arg(long, default_value = "4")]
    pub png_level: u8,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

pub fn run(args: FaviconArgs) -> Result<()> {
    let opts = FaviconOptions {
        png_level: args.png_level,
    };
    let result = generate_favicon_set(&args.input, &args.output_dir, &opts)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    println!(
        "{} Favicon set generated in {} {}",
        ok_mark(),
        green(&args.output_dir.display().to_string()),
        dim(&format!(
            "({} files, {})",
            result.generated_files.len(),
            format_size(result.total_size)
        )),
    );
    for file in &result.generated_files {
        println!("  {} {}", dim("•"), green(&file.display().to_string()));
    }
    println!("\n{}", bold("HTML snippet"));
    for line in result.html_snippet.lines() {
        println!("  {line}");
    }
    Ok(())
}
