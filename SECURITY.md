# Security Policy

## Supported Versions

DCFC is preparing its first public release. The current `0.1.0` builds are
development builds and do not yet have a public security-support promise.
After an official release, supported versions and update instructions will be
listed here.

## Reporting a Vulnerability

Do not publish credentials, private project files, logs, or exploit details in
a public issue. Use the repository's **Security > Report a vulnerability**
private-reporting channel if it is available. If it is not enabled, open a
minimal public issue requesting a private contact channel without including
technical details or sensitive data. Maintainers must enable or publish a
working private reporting channel before the first public release.

Include the DCFC version, Windows version, affected component, reproduction
steps and expected impact. Redact Runtime API Keys, Tunnel credentials,
personal paths and project contents. Please allow maintainers time to
acknowledge and coordinate a fix before public disclosure.

## Local Security Boundary

Runtime API Keys are stored through Windows Credential Manager, not in
`settings.json`. The fixed Router resolves explicit `project_id` values and
text-editing tools are confined to each configured output directory. Report
any path traversal, cross-project access, credential disclosure, or
unauthorized network exposure through the private channel above.
