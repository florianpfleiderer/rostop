use anyhow::Result;
use clap::Parser;

use rostop_cli::app::{run, App};
use rostop_cli::backend::demo::DemoBackend;
use rostop_cli::backend::RosBackend;
use rostop_cli::domain::{resolve_domain, DomainId, ProbeConfig};

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

    /// Internal process-isolated domain probe.
    #[arg(long, hide = true, value_name = "ID")]
    probe_domain: Option<DomainId>,

    /// Discovery window for the internal domain probe.
    #[arg(long, hide = true, default_value_t = 800)]
    probe_ms: u64,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(domain) = cli.probe_domain {
        return run_probe(domain, cli.probe_ms);
    }
    let domain = resolve_domain(
        cli.domain,
        std::env::var("ROS_DOMAIN_ID").ok().as_deref(),
    )?;
    let backend: Box<dyn RosBackend> = pick_backend(cli.demo, domain)?;
    let app = App::new(backend);
    run(app)
}

#[cfg(feature = "live")]
fn run_probe(domain: DomainId, probe_ms: u64) -> Result<()> {
    let result = rostop_cli::backend::live::probe_domain(
        domain,
        ProbeConfig {
            discovery_window: std::time::Duration::from_millis(probe_ms),
        },
    )?;
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}

#[cfg(not(feature = "live"))]
fn run_probe(_domain: DomainId, _probe_ms: u64) -> Result<()> {
    anyhow::bail!("domain probing requires a rostop build with the live feature")
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
