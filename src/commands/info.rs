use anyhow::Result;
use clap::Args;
use pixa::info::get_image_info;
use std::path::PathBuf;

use super::style::{bold, cyan, dim, green, red};

#[derive(Args)]
pub struct InfoArgs {
    /// Input image file
    pub input: PathBuf,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

pub fn run(args: InfoArgs) -> Result<()> {
    let info = get_image_info(&args.input)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&info)?);
        return Ok(());
    }

    let label = |s: &str| dim(&format!("{s:<11}"));
    println!("{}{}", label("File"), green(&info.file_name));
    println!("{}{}", label("Format"), cyan(&info.format));
    println!(
        "{}{} {}",
        label("Size"),
        red(&info.file_size_human),
        dim(&format!("({} bytes)", info.file_size))
    );
    println!(
        "{}{}",
        label("Dimensions"),
        cyan(&format!("{}×{}", info.width, info.height))
    );
    println!("{}{}", label("Pixels"), red(&info.pixel_count.to_string()));
    println!("{}{}", label("Color"), info.color_type);
    println!("{}{}-bit", label("Depth"), info.bit_depth);
    println!(
        "{}{}",
        label("Alpha"),
        if info.has_alpha { "yes" } else { "no" }
    );
    println!("{}{}", label("SHA-256"), dim(&info.sha256));

    if let Some(exif) = &info.exif {
        println!("\n{} {}", bold("EXIF"), dim(&format!("({} fields)", exif.len())));
        let mut entries: Vec<_> = exif.iter().collect();
        entries.sort_by_key(|(k, _)| k.to_string());
        for (key, value) in entries {
            println!("  {} {value}", dim(&format!("{key}:")));
        }
    }
    Ok(())
}
