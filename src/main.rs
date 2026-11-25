//! RINEX-Cli: command line application of the GNSS-Qc framework.
//! Homepage: <https://github.com/nav-solutions/rinex-cli>
mod cli;
mod engine;
mod error;
mod fops;
mod index;
mod input;
mod report;
mod sink;
mod snap;
mod summary;
mod workspace;

#[cfg(feature = "navigation")]
mod nav;

use clap::Parser;
use env_logger::{Builder, Env, Target};

use cli::{Cli, Command};
use error::Error;
use input::Inputs;
use workspace::Workspace;

fn init_logger(cli: &Cli) {
    let level = if cli.outputs.quiet { "warn" } else { "info" };

    // the navigation solver reports every rejected candidate (info): warnings only
    let filter = format!("{},gnss_rtk=warn", level);

    Builder::from_env(Env::default().default_filter_or(filter))
        .target(Target::Stderr)
        .format_timestamp_secs()
        .format_module_path(false)
        .init();
}

fn run(cli: Cli) -> Result<(), Error> {
    let inputs = Inputs::load(&cli.inputs, &cli.preprocessing)?;

    if let Command::Index(args) = &cli.command {
        return index::run(&inputs, args);
    }

    let session = cli
        .outputs
        .session
        .clone()
        .unwrap_or_else(|| inputs.session.clone());

    let workspace = Workspace::new(&cli.outputs.output, &session)?;

    if cli.command.is_file_operation() {
        return fops::run(&cli, &inputs, &workspace);
    }

    let plan = engine::plan(&cli, inputs, &workspace)?;
    engine::run(plan)?;

    if cli.outputs.open {
        for name in ["report.html", "report.pdf"] {
            if workspace.path(name).exists() {
                workspace.open(name);
                break;
            }
        }
    }

    Ok(())
}

fn main() {
    let cli = Cli::parse();
    init_logger(&cli);

    if let Err(e) = run(cli) {
        // stdout closed by the consumer (| head): not an error
        if e.to_string().contains("Broken pipe") {
            return;
        }

        log::error!("{}", e);
        std::process::exit(1);
    }
}
