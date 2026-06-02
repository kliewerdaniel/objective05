# Security Model

## Purpose

Define Objective's security model — threat model, trust boundaries, supply chain security, and operational security for a local-first intelligence system.

## Scope

This document covers threat modeling, trust model, supply chain security, local operation security, credential management, and security incident response.

## Responsibilities

- Define the threat model for Objective
- Document trust boundaries and assumptions
- Specify supply chain security measures
- Define credential management
- Document security incident procedures

## Assumptions

- Objective runs on a single-user machine (multi-user is Phase 3+)
- The user trusts their local machine (OS-level security)
- Network communication is only for ingestion (source fetching) and model downloads
- No external services are required for core operation
- The attacker model is: remote adversary attempting to exploit the system

## Design

### Threat Model

**Assets to protect:**
- Knowledge graph data (entities, claims, narratives)
- Source credentials (API keys, tokens, passwords)
- User configuration (source lists, preferences)
- Model files (integrity, not confidentiality)
- Generated broadcasts (confidential until published)

**Trust boundaries:**
```
┌────────────────────────────────────────────┐
│           Trusted (Local Machine)          │
│  ┌──────────────────────────────────────┐  │
│  │  Objective Process                   │  │
│  │  - All data at rest                  │  │
│  │  - All processing                    │  │
│  │  - Credentials in memory             │  │
│  └──────────┬───────────────────────────┘  │
│             │                               │
│             ▼                               │
│  ┌──────────────────────────────────────┐  │
│  │  OS Security                         │  │
│  │  - File permissions                  │  │
│  │  - Process isolation                 │  │
│  │  - Memory protection                 │  │
│  └──────────────────────────────────────┘  │
└────────────────┬───────────────────────────┘
                 │
                 ▼
┌────────────────────────────────────────────┐
│          Semi-Trusted (Network)            │
│  - Source servers (RSS, APIs)              │
│  - Model repositories (HuggingFace)        │
│  - Update server                           │
└────────────────────────────────────────────┘
                 │
                 ▼
┌────────────────────────────────────────────┐
│           Untrusted (External)             │
│  - Network adversaries                     │
│  - Malicious source content                │
│  - Compromised update server               │
└────────────────────────────────────────────┘
```

**Threat scenarios:**

| Threat | Impact | Likelihood | Mitigation |
|--------|--------|------------|------------|
| Remote exploit via network service | Full system compromise | Low | Do not expose unnecessary ports; input validation |
| Malicious source content (injection) | Data corruption | Medium | Input sanitization; output validation |
| Credential theft from disk | Source access compromise | Low | Encrypted credential storage |
| Man-in-the-middle on model download | Malicious model loaded | Low | HTTPS + checksum verification |
| Supply chain attack on dependency | Compromise via dependency | Low | Dependency scanning; minimal dependencies |
| Local privilege escalation | Full system access | Low | Run as user, not root |
| Denial of service via source | Resource exhaustion | Medium | Rate limiting; backpressure; timeouts |
| Data exfiltration via plugin | Data leak | Medium (if plugins used) | Plugin sandboxing; permission system |

### Credential Management

```yaml
credentials:
  storage: "os_keyring"       # OS-level keyring (macOS Keychain, Linux Secret Service, Windows Credential Manager)
  fallback: "encrypted_file"  # Encrypted JSON file if keyring unavailable
  encryption: "aes-256-gcm"
  key_derivation: "argon2id"  # For encrypted file fallback
```

**Supported storage backends:**

| Backend | Platform | Security |
|---------|----------|----------|
| macOS Keychain | macOS | Hardware-backed on T2/M-series |
| Secret Service (D-Bus) | Linux | GNOME Keyring / KDE Wallet |
| Windows Credential Manager | Windows | DPAPI-encrypted |
| Encrypted file (fallback) | All | AES-256-GCM with Argon2id KDF |

**What is stored:**
- Source API keys (Reddit, YouTube, etc.)
- Email credentials (IMAP password)
- Custom API keys for plugins
- Model repository tokens (if any)

**What is NOT stored:**
- OS login credentials
- SSH keys (not used by Objective)
- Browser cookies (not used by Objective)

### Supply Chain Security

```yaml
supply_chain:
  binary_signing:
    enabled: true
    method: "gpg"             # GPG-signed binaries
    key_id: "..."             # Objective signing key
  
  dependency_verification:
    enabled: true
    use_lockfile: true        # Cargo.lock / yarn.lock equivalent
    vulnerability_scan: true  # Weekly CVE scan
  
  model_verification:
    checksum: true            # SHA-256 verification of model files
    source: "huggingface"     # Only from trusted repositories
```

**Binary signing:**
- All release binaries are GPG-signed with the Objective release key
- Signatures verified by package managers (Homebrew, APT)
- Windows binaries are Authenticode-signed

**Dependency management:**
- All dependencies pinned with lockfiles
- Weekly automated vulnerability scanning
- Dependencies minimized (audit each new dependency for necessity)
- Rust crates from crates.io with checksum verification

**Model integrity:**
- Models downloaded over HTTPS from HuggingFace
- SHA-256 checksum verified after download
- Model directory integrity checked at startup
- Corrupt models are re-downloaded

### Network Security

```yaml
network:
  listening_interfaces:
    - "127.0.0.1"             # Localhost only by default
    - "::1"                   # IPv6 localhost
  
  ports:
    dashboard: 8080           # Dashboard HTTP
    streaming: 8081           # Audio streaming / WebSocket
  
  tls:
    enabled: false            # No TLS on localhost (default)
    cert_file: null           # User can configure for remote access
    key_file: null
  
  outbound:
    allow_sources: true       # Fetching source content
    allow_models: true        # Downloading models (can be disabled)
    allow_updates: true       # Update checks
    block_otherwise: true     # Block unexpected outbound
```

**Default: localhost only**
- Dashboard and API are only accessible from the local machine
- Users can configure remote access with TLS
- Outbound connections are limited to configured sources and model repositories
- All outbound connections use HTTPS/TLS

### Plugin Security

See `docs/api/plugin-api.md` for detailed plugin security model.

Summary:
- Plugins run as separate processes (no in-process plugins)
- Plugins have no direct access to the knowledge graph
- Plugin I/O goes through the Plugin Host (validated)
- Resource limits (memory, CPU, network) enforced
- Network access declared in manifest (default: blocked)
- Plugin output validated against schema

### Incident Response

**Security incident procedures:**

1. **Detection:**
   - Automated: audit log anomalies, unexpected outbound connections, file integrity changes
   - Manual: user reports suspicious behavior

2. **Containment:**
   - Stop Objective: `objective stop --force`
   - Disconnect machine from network (if remote access configured)
   - Revoke any exposed credentials

3. **Investigation:**
   - Review audit logs: `~/.objective/logs/audit.log`
   - Review system logs: `~/.objective/logs/system.log`
   - Check snapshot integrity: `objective snapshot verify`
   - Check file integrity: `objective fsck`

4. **Recovery:**
   - Restore from pre-incident snapshot: `objective snapshot restore --pre-incident`
   - Rotate all credentials in source configuration
   - Update Objective to latest version
   - Resume operation

5. **Post-mortem:**
   - Document incident timeline
   - Identify root cause
   - Update security controls
   - Release security advisory if applicable

## Interfaces

- `privacy-model.md` — privacy considerations related to security
- `docs/api/plugin-api.md` — plugin security model
- `docs/deployment/operations.md` — operational security procedures

## Failure Modes

| Failure | Impact | Mitigation |
|--------|--------|------------|
| Credential storage backend unavailable | Sources can't authenticate | Fallback to encrypted file storage |
| Binary signature verification fails | Install/update blocked | Manual override with checksum verification |
| Dependency vulnerability discovered | Potential exploit | Automated scanning; rapid patch release |
| Plugin bypasses sandbox | Potential data access | Defense in depth: output validation + resource limits |
| Model repository compromised | Malicious model loaded | Checksum verification; pin specific model versions |
| Source server compromised (supplies malicious content) | Injection attack | Input sanitization; content validation |

## Future Extensions

- Full disk encryption integration (FileVault, LUKS, BitLocker)
- TPM/secure enclave key storage
- SELinux/AppArmor profiles
- Automatic security advisory notifications
- Bug bounty program
- Security-focused code audit (third-party)
- Formal verification of critical components
