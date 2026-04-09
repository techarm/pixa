use anyhow::Result;
use clap::Args;
use clap_complete::Shell;

#[derive(Args)]
pub struct CompletionsArgs {
    /// Shell to generate completions for
    #[arg(value_enum)]
    pub shell: Shell,
}

pub fn run(args: CompletionsArgs, cmd: &mut clap::Command) -> Result<()> {
    let mut out = std::io::stdout().lock();
    clap_complete::generate(args.shell, cmd, "pixa", &mut out);
    Ok(())
}
