use anyhow::Result;
use clap::Args;
use pixa::convert::{convert_image, convert_image_from_dynamic};
use std::path::PathBuf;

use super::style::{arrow, bold, dim, err, green, ok_mark, red, yellow};
use super::{
    ImageSource, collect_inputs, ensure_parent, guard_clipboard_not_directory, mirror_path,
};

#[derive(Args)]
pub struct ConvertArgs {
    /// Input image file or directory. Use @clipboard (aliases: @clip, @c)
    /// to read the image from the OS clipboard.
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
    let source = ImageSource::parse(&args.input);
    guard_clipboard_not_directory(&source, args.recursive)?;

    if source.is_clipboard() {
        ensure_parent(&args.output)?;
        let img = source.load_image()?;
        convert_image_from_dynamic(&img, &args.output)?;
        println!(
            "{} {} {} {}",
            ok_mark(),
            green("@clipboard"),
            arrow(),
            args.output.display(),
        );
        return Ok(());
    }

    let inputs = collect_inputs(&args.input, args.recursive)?;
    if inputs.is_empty() {
        println!("{} No images found.", yellow("!"));
        return Ok(());
    }

    let single_file = inputs.len() == 1 && !args.input.is_dir();

    if single_file {
        convert_image(&inputs[0], &args.output)?;
        println!(
            "{} {} {} {}",
            ok_mark(),
            green(&inputs[0].display().to_string()),
            arrow(),
            args.output.display(),
        );
        return Ok(());
    }

    let format = args
        .format
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--format is required when converting a directory"))?;

    let input_root = args.input.as_path();
    let mut success = 0u32;
    let mut failed = 0u32;

    for input in &inputs {
        let mut out_path = mirror_path(input, input_root, Some(&args.output));
        out_path.set_extension(format);
        if let Err(e) = ensure_parent(&out_path) {
            eprintln!(
                "{} {}: {}",
                err::fail_mark(),
                input.display(),
                err::red(&e.to_string())
            );
            failed += 1;
            continue;
        }
        match convert_image(input, &out_path) {
            Ok(_) => {
                success += 1;
                println!(
                    "{} {} {} {}",
                    ok_mark(),
                    green(&input.display().to_string()),
                    arrow(),
                    dim(&out_path.display().to_string())
                );
            }
            Err(e) => {
                failed += 1;
                eprintln!(
                    "{} {}: {}",
                    err::fail_mark(),
                    input.display(),
                    err::red(&e.to_string())
                );
            }
        }
    }

    println!(
        "\n{}  {}",
        bold("Summary"),
        if failed == 0 {
            green(&format!("{success} ok"))
        } else {
            format!(
                "{}, {}",
                green(&format!("{success} ok")),
                red(&format!("{failed} failed"))
            )
        }
    );
    Ok(())
}
