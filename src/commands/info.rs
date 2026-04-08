use anyhow::Result;
use clap::Args;
use pixa::info::get_image_info;
use std::path::PathBuf;

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

    println!("File:       {}", info.file_name);
    println!("Format:     {}", info.format);
    println!("Size:       {} ({})", info.file_size_human, info.file_size);
    println!("Dimensions: {}x{}", info.width, info.height);
    println!("Pixels:     {}", info.pixel_count);
    println!("Color:      {}", info.color_type);
    println!("Bit depth:  {}", info.bit_depth);
    println!("Alpha:      {}", info.has_alpha);
    println!("SHA-256:    {}", info.sha256);
    if let Some(exif) = &info.exif {
        println!("\nEXIF ({} fields):", exif.len());
        let mut entries: Vec<_> = exif.iter().collect();
        entries.sort_by_key(|(k, _)| k.to_string());
        for (key, value) in entries {
            println!("  {key}: {value}");
        }
    }
    Ok(())
}
