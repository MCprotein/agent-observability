# @mcprotein/agent-observability

GitHub Packages transport for the native macOS `agent-observability` CLI.
The package contains one universal Rust binary for Apple Silicon and Intel Macs;
package installation has no postinstall side effects and does not install a
JavaScript runtime wrapper or network collector. Running `agentobs setup` is a
separate, explicit action that installs the local-only Codex collector as a
macOS LaunchAgent; `agentobs disconnect codex` removes that integration.

See the [repository README](https://github.com/MCprotein/agent-observability#readme)
for authentication, installation, input requirements, and commands.
The package also includes a content-free example handoff for installation smoke tests.
