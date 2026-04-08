use anyhow::Result;
use clap::Args;
use pixa::convert::convert_image;
use std::path::PathBuf;

use super::{collect_inputs, ensure_parent, mirror_path};

#[derive(Args)]
pub struct ConvertArgs {
    /// Input image file or directory
    pub input: PathBuf,
    /// Output file (single input) or directory (recursive)
    pub output: PathBuf,
    /// Recursively process directories
    #[arg(short, long)]
    pub recursive: bool,
    /// Target format extension when processing a directory (e.g. webp, png)
    #[arg(long)]
    pub format: Option<String>,
}

pub fn run(args: ConvertArgs) -> Result<()> {
    let inputs = collect_inputs(&args.input, args.recursive)?;
    if inputs.is_empty() {
        println!("No images found.");
        return Ok(());
    }

    let single_file = inputs.len() == 1 && !args.input.is_dir();

    if single_file {
        convert_image(&inputs[0], &args.output)?;
        println!(
            "Converted: {} -> {}",
            inputs[0].display(),
            args.output.display()
        );
        return Ok(());
    }

    let format = args
        .format
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--format is required when converting a directory"))?;

    let input_root = args.input.as_path();
    let mut success = 0;
    let mut failed = 0;

    for input in &inputs {
        let mut out_path = mirror_path(input, input_root, Some(&args.output));
        out_path.set_extension(format);
        if let Err(e) = ensure_parent(&out_path) {
            eprintln!("FAIL: {}: {e}", input.display());
            failed += 1;
            continue;
        }
        match convert_image(input, &out_path) {
            Ok(_) => {
                success += 1;
                println!("OK: {} -> {}", input.display(), out_path.display());
            }
            Err(e) => {
                failed += 1;
                eprintln!("FAIL: {}: {e}", input.display());
            }
        }
    }

    println!("\nDone: {success} succeeded, {failed} failed");
    Ok(())
}
