# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| Latest  | Yes       |

## Reporting a Vulnerability

If you discover a security vulnerability, please report it responsibly:

1. **Do not** open a public issue
2. Email the maintainer or use [GitHub Security Advisories](https://github.com/tyql688/cc-session/security/advisories/new)
3. Include a description of the vulnerability and steps to reproduce

You can expect an initial response within 72 hours.

## Scope

CC Session is a desktop app that reads local AI coding session files. Security concerns include:

- Path traversal when reading session files or images
- XSS in rendered markdown or HTML exports
- Command injection in terminal resume commands
- Unauthorized file access outside expected directories
