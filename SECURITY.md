# Security Policy

## Supported Versions

The following versions of HyperInfer are currently receiving security updates:

| Version | Supported |
|---------|-----------|
| 0.2.x   | ✅ Yes    |
| 0.1.x   | ⚠️ Patch only for critical issues |
| < 0.1.0 | ❌ No     |

## Reporting a Vulnerability

We take the security of HyperInfer seriously. If you discover a security vulnerability, please report it privately rather than opening a public issue.

### How to Report

1. Navigate to [GitHub Security Advisories](https://github.com/AndroZa0812/HyperInfer/security/advisories/new)
2. Click "Draft a new security advisory"
3. Fill in the details of the vulnerability
4. Submit the report

Alternatively, you can open a [GitHub Security Advisory](https://github.com/AndroZa0812/HyperInfer/security/advisories/new) directly.

### What to Include

Please include the following information in your report:

- **Description**: A clear description of the vulnerability
- **Affected component**: Which crate or module is affected (e.g., `hyperinfer-client`, `hyperinfer-server`, `hyperinfer-python`)
- **Severity**: Your assessment of the severity (low, medium, high, critical)
- **Steps to reproduce**: Detailed steps to reproduce the issue
- **Proof of concept**: If available, a minimal example demonstrating the vulnerability
- **Environment**: Version of HyperInfer, Rust version, OS, and any relevant configuration
- **Suggested fix**: If you have ideas for a fix, please include them

### What to Expect

- **Acknowledgment**: We aim to acknowledge receipt of your report within **48 hours**
- **Initial assessment**: We will provide an initial assessment of the vulnerability within **5 business days**
- **Status updates**: We will keep you informed of progress throughout the remediation process
- **Credit**: We will credit you in the security advisory unless you request anonymity

### Scope

This security policy covers the following components:

- `hyperinfer-core` — Core types and shared logic
- `hyperinfer-client` — SDK client library
- `hyperinfer-server` — Control plane server
- `hyperinfer-router` — LLM request router
- `hyperinfer-providers` — LLM provider implementations
- `hyperinfer-python` — Python bindings (PyO3)
- `hypercode` — Agent framework
- `hyperinfer-langchain` — LangChain integration
- `hyperinfer-llamaindex` — LlamaIndex integration

### Out of Scope

- Vulnerabilities in third-party dependencies (please report those upstream)
- Issues related to misconfiguration by users
- Denial of service attacks requiring physical access

## Security Best Practices

When deploying HyperInfer in production:

- Keep your installation up to date with the latest security patches
- Use environment variables or a secrets manager for API keys
- Configure appropriate rate limits and quotas
- Enable TLS for server deployments
- Regularly review access logs and usage patterns
