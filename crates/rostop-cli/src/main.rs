use anyhow::Result;
use clap::Parser;

use rostop_cli::app::{run, App};
use rostop_cli::backend::demo::DemoBackend;
use rostop_cli::backend::RosBackend;

/// Interactive TUI for inspecting and debugging ROS 2 topics.
#[derive(Debug, Parser)]
#[command(name = "rostop", version, about, long_about = None)]
struct Cli {
    /// Run with a fake ROS 2 system — no ROS install needed.
    #[arg(long, default_value_t = true)]
    demo: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let backend: Box<dyn RosBackend> = if cli.demo {
        Box::new(DemoBackend::new())
    } else {
        // Live backend coming in a follow-up — for now we always run demo.
        Box::new(DemoBackend::new())
    };
    let app = App::new(backend);
    run(app)
}
