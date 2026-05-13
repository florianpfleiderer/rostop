//! Capture the ROS distro and RMW implementation that rostop is being built
//! against so the runtime peer-probe error message can name the real target
//! instead of hard-coding a single distro.
//!
//! These values are sourced from the build-time environment (the same one
//! that exposes the `rcl` / `rmw` headers via `r2r`'s build), so the values
//! land in the binary exactly when the build is also linking against that
//! distro's libraries. When the env vars are unset (e.g. someone running
//! `cargo build` without sourcing `setup.bash` and without the `live`
//! feature), we fall back to "unknown" — the strings are only consumed by
//! the live backend's error path.

fn main() {
    println!("cargo:rerun-if-env-changed=ROS_DISTRO");
    println!("cargo:rerun-if-env-changed=RMW_IMPLEMENTATION");

    let distro = std::env::var("ROS_DISTRO").unwrap_or_else(|_| "unknown".into());
    let rmw = std::env::var("RMW_IMPLEMENTATION").unwrap_or_else(|_| "unknown".into());

    println!("cargo:rustc-env=ROSTOP_TARGET_DISTRO={distro}");
    println!("cargo:rustc-env=ROSTOP_TARGET_RMW={rmw}");
}
