use anyhow::Result;
use clap::Parser;

use rostop_cli::app::{run, App};
use rostop_cli::backend::demo::DemoBackend;
use rostop_cli::backend::RosBackend;
use rostop_cli::domain::{resolve_domain, DomainId};

/// Interactive TUI for inspecting and debugging ROS 2 topics.
#[derive(Debug, Parser)]
#[command(name = "rostop", version, about, long_about = None)]
struct Cli {
    /// Run with a fabricated ROS 2 system — no ROS install needed.
    /// Default when built without the `live` feature.
    #[arg(long)]
    demo: bool,

    /// ROS domain to inspect (0-232). Overrides ROS_DOMAIN_ID.
    #[arg(long, value_name = "ID")]
    domain: Option<DomainId>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let domain = resolve_domain(
        cli.domain,
        std::env::var("ROS_DOMAIN_ID").ok().as_deref(),
    )?;
    let backend: Box<dyn RosBackend> = pick_backend(cli.demo, domain)?;
    let app = App::new(backend);
    run(app)
}

#[cfg(feature = "live")]
fn pick_backend(demo: bool, domain: DomainId) -> Result<Box<dyn RosBackend>> {
    use rostop_cli::backend::live::LiveBackend;
    if demo {
        Ok(Box::new(DemoBackend::new()))
    } else {
        Ok(Box::new(LiveBackend::new_for_domain(domain)?))
    }
}

#[cfg(not(feature = "live"))]
fn pick_backend(_demo: bool, _domain: DomainId) -> Result<Box<dyn RosBackend>> {
    // Without the `live` feature the live backend isn't compiled in, so
    // there's only one choice. We honour `--demo` silently (it's a no-op).
    Ok(Box::new(DemoBackend::new()))
}
