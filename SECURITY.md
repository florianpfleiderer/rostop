# Security policy

## Supported versions

rostop is at 0.x; only the most recent release receives fixes. If you are running an older tagged version, please upgrade before reporting an issue.

| Version    | Supported          |
| ---------- | ------------------ |
| 0.1.x      | ✅                 |
| < 0.1      | ❌ (pre-release)   |

## Reporting a vulnerability

Please **do not** open a public issue for security problems.

Use GitHub's [private vulnerability reporting](https://github.com/florianpfleiderer/rostop/security/advisories/new) (Security → Report a vulnerability on the repo page). If that is not available for you, email **florian.pfleiderer@agile-robots.com** with subject prefix `[rostop security]`.

Please include:

- rostop version and which build (Humble / Jazzy / local cargo).
- A minimal reproduction or proof of concept.
- Impact assessment (what an attacker can do, what they need).
- Suggested fix if you have one.

## Response expectations

This is a solo-maintained project. Best-effort response time:

- Acknowledgement within **7 days**.
- Triage decision (accept / clarify / decline) within **14 days**.
- Fix or mitigation timeline communicated once triaged.

## Scope

In scope:

- Memory-safety bugs reachable from untrusted ROS topic data (rostop subscribes to whatever the graph publishes).
- Code execution or privilege escalation triggered by malformed messages, malformed CLI input, or malformed config files.
- Crashes triggered by messages a real ROS 2 publisher could legitimately send (we should not panic the TUI on valid graph traffic).

Out of scope:

- Vulnerabilities in upstream dependencies (`r2r`, `ratatui`, `tokio`, etc.) — report those to the upstream project; we will pick up patched releases.
- Issues that require local code execution or filesystem access already at rostop's privilege level (rostop is a passive observer, not a security boundary).
- DoS by simply flooding the ROS graph with high-rate publishers; we mitigate (array eliding, bounded sample buffers) but a robot's own publishers can always saturate a viewer.

## Disclosure

Coordinated disclosure preferred. Once a fix is available and released, we will publish a GitHub Security Advisory crediting the reporter (unless anonymity is requested).
