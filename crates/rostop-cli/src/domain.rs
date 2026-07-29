//! ROS domain selection and probe protocol shared by the CLI and scanner.

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

pub const MAX_DOMAIN_ID: u16 = 232;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DomainId(u16);

impl DomainId {
    pub const DEFAULT: Self = Self(0);

    pub fn new(value: u16) -> Result<Self, DomainIdError> {
        if value <= MAX_DOMAIN_ID {
            Ok(Self(value))
        } else {
            Err(DomainIdError(value.to_string()))
        }
    }

    pub fn get(self) -> u16 {
        self.0
    }
}

impl fmt::Display for DomainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for DomainId {
    type Err = DomainIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parsed = value
            .parse::<u16>()
            .map_err(|_| DomainIdError(value.to_string()))?;
        Self::new(parsed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainIdError(String);

impl fmt::Display for DomainIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid ROS domain ID {:?}; expected an integer from 0 to {MAX_DOMAIN_ID}",
            self.0
        )
    }
}

impl std::error::Error for DomainIdError {}

pub fn resolve_domain(
    cli: Option<DomainId>,
    environment: Option<&str>,
) -> Result<DomainId, DomainIdError> {
    cli.map(Ok)
        .unwrap_or_else(|| environment.map(str::parse).unwrap_or(Ok(DomainId::DEFAULT)))
}

#[cfg_attr(feature = "live", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainProbeResult {
    pub protocol_version: u8,
    pub domain_id: u16,
    pub visible_topics: usize,
    pub visible_nodes: usize,
    pub discovery_ms: u64,
}

impl DomainProbeResult {
    pub const PROTOCOL_VERSION: u8 = 1;

    pub fn is_visible(&self) -> bool {
        self.visible_topics > 0 || self.visible_nodes > 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeConfig {
    pub discovery_window: Duration,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            discovery_window: Duration::from_millis(800),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_protocol_range_boundaries() {
        assert_eq!("0".parse::<DomainId>().unwrap().get(), 0);
        assert_eq!("232".parse::<DomainId>().unwrap().get(), 232);
    }

    #[test]
    fn rejects_out_of_range_or_non_numeric_values() {
        assert!("233".parse::<DomainId>().is_err());
        assert!("-1".parse::<DomainId>().is_err());
        assert!("robot".parse::<DomainId>().is_err());
    }

    #[test]
    fn cli_overrides_environment_and_environment_overrides_default() {
        assert_eq!(
            resolve_domain(Some(DomainId::new(7).unwrap()), Some("42"))
                .unwrap()
                .get(),
            7
        );
        assert_eq!(resolve_domain(None, Some("42")).unwrap().get(), 42);
        assert_eq!(resolve_domain(None, None).unwrap().get(), 0);
    }

    #[test]
    fn probe_visibility_requires_an_external_graph_signal() {
        let empty = DomainProbeResult {
            protocol_version: DomainProbeResult::PROTOCOL_VERSION,
            domain_id: 7,
            visible_topics: 0,
            visible_nodes: 0,
            discovery_ms: 800,
        };
        assert!(!empty.is_visible());
        assert!(DomainProbeResult {
            visible_nodes: 1,
            ..empty
        }
        .is_visible());
    }
}
