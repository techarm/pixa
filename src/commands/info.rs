use anyhow::Result;
use clap::Args;
use image::DynamicImage;
use pixa::info::{get_image_info, get_image_info_from_image};
use std::io::Cursor;
use std::path::PathBuf;

use super::ImageSource;
use super::style::{bold, cyan, dim, green, red};

#[derive(Args)]
pub struct InfoArgs {
    /// Input image file, or @clipboard to read from the OS clipboard
    pub input: PathBuf,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

pub fn run(args: InfoArgs) -> Result<()> {
    let source = ImageSource::parse(&args.input);
    let info = if source.is_clipboard() {
        // If Finder put a file URL on the clipboard, report real file
        // metadata (size, SHA of source bytes, EXIF) — same detail as
        // `pixa info <path>`.
        if let Some(path) = pixa::clipboard::read_file_url()? {
            get_image_info(&path)?
        } else {
            let img = source.load_image()?;
            // Otherwise re-encode the decoded RGBA to PNG so file_size
            // and SHA-256 are deterministic for raw-pixel clipboard input.
            let png_bytes = encode_to_png(&img)?;
            get_image_info_from_image(&img, "@clipboard", Some(&png_bytes))
        }
    } else {
        get_image_info(&args.input)?
    };
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
        println!(
            "\n{} {}",
            bold("EXIF"),
            dim(&format!("({} fields)", exif.len()))
        );
        let mut entries: Vec<_> = exif.iter().collect();
        entries.sort_by_key(|(k, _)| k.to_string());
        for (key, value) in entries {
            println!("  {} {value}", dim(&format!("{key}:")));
        }
    }
    Ok(())
}

fn encode_to_png(img: &DynamicImage) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)?;
    Ok(buf)
}
