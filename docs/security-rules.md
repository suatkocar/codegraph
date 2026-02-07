# Security Rules Guide

CodeGraph ships with 65+ security rules organized into 4 bundled rulesets. You can also write custom rules in YAML and load them from disk.

## Bundled Rulesets

| Ruleset | File | Rules | Coverage |
|---------|------|-------|----------|
| OWASP Top 10 2021 | `rules/owasp-top10.yaml` | ~20 | A01-A10 vulnerability categories |
| CWE Top 25 | `rules/cwe-top25.yaml` | ~25 | Most dangerous software weaknesses |
| Crypto | `rules/crypto.yaml` | ~10 | Weak algorithms, insecure usage |
| Secrets | `rules/secrets.yaml` | ~10 | Hardcoded keys, passwords, tokens |

All bundled rules are compiled into the binary at build time via `include_str!`. No external files are needed at runtime.

## Rule Format

Each rule file is a YAML document with a top-level `name`, `version`, `description`, and a `rules` array:

```yaml
name: My Custom Rules
version: "1.0.0"
description: Project-specific security rules

rules:
  - id: CUSTOM-001
    name: Dangerous Function Call
    severity: high
    cwe: "CWE-78"
    owasp: "A03:2021"
    languages: ["python", "javascript"]
    pattern: 'os\.system\('
    message: Use of os.system() allows command injection
    fix: Use subprocess.run() with shell=False
    category: injection
```

## Field Reference

### Required Fields

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Unique rule identifier (e.g., `OWASP-A03-001`, `CUSTOM-001`) |
| `name` | string | Human-readable rule name |
| `severity` | enum | `info`, `low`, `medium`, `high`, `critical` |
| `pattern` | string | Regex pattern to match against source code |
| `message` | string | Description of the issue when the pattern matches |

### Optional Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `cwe` | string | null | CWE identifier (e.g., `CWE-89`) |
| `owasp` | string | null | OWASP Top 10 category (e.g., `A03:2021`) |
| `languages` | list | [] (all) | Languages this rule applies to. Empty = all languages |
| `fix` | string | null | Remediation guidance |
| `category` | enum | `other` | Rule category (see below) |

### Severity Levels

| Level | When to Use |
|-------|-------------|
| `critical` | Exploitable vulnerability with severe impact (RCE, data breach) |
| `high` | Significant security weakness (SQL injection, XSS) |
| `medium` | Moderate risk (insecure configuration, weak validation) |
| `low` | Minor issue (informational disclosure, best practice violation) |
| `info` | Informational finding (TODOs, deprecated usage) |

### Categories

| Category | Description |
|----------|-------------|
| `injection` | SQL injection, command injection, code injection |
| `crypto` | Weak hash algorithms, insecure encryption, poor key management |
| `secrets` | Hardcoded API keys, passwords, tokens, connection strings |
| `config` | Insecure configuration, debug mode, permissive CORS |
| `authentication` | Broken authentication, session issues, access control |
| `xss` | Cross-site scripting via innerHTML, document.write, etc. |
| `pathtraversal` | Path traversal via user-controlled file paths |
| `deserialization` | Unsafe deserialization of untrusted data |
| `other` | Anything that doesn't fit the above categories |

### Language Filtering

The `languages` field controls which languages a rule applies to. Values are matched case-insensitively.

```yaml
# Only match Python and JavaScript files
languages: ["python", "javascript"]

# Match all languages (empty list or omit the field)
languages: []
```

Supported language names: `typescript`, `javascript`, `python`, `go`, `rust`, `java`, `c`, `cpp`, `csharp`, `php`, `ruby`, `swift`, `kotlin`, `bash`, `scala`, `haskell`, `julia`, `lua`, `elixir`, `clojure`, `dart`, `fortran`, `groovy`, `zig`, `r`, `verilog`, `erlang`, `elm`, `powershell`, `nix`.

### Pattern Syntax

Patterns use Rust `regex` crate syntax (similar to PCRE but without lookaround):

```yaml
# Literal match with escaped special characters
pattern: 'eval\('

# Case-insensitive match
pattern: '(?i)md5'

# Match with alternatives
pattern: '(?:innerHTML|outerHTML)\s*='

# Match across common code patterns
pattern: 'password\s*=\s*[''"][^''"]{3,}[''"]'
```

Patterns are applied line-by-line across the source file. Each regex match produces a finding with the line number and column.

## Writing Custom Rules

### Step 1: Create a YAML file

```yaml
name: My Project Rules
version: "1.0.0"
description: Security rules specific to our codebase

rules:
  - id: PROJ-001
    name: Direct Database Query
    severity: high
    cwe: "CWE-89"
    languages: ["python"]
    pattern: 'db\.execute\([^)]*f[''"]'
    message: f-string in database query enables SQL injection
    fix: Use parameterized queries with db.execute(query, params)
    category: injection

  - id: PROJ-002
    name: Debug Mode Enabled
    severity: medium
    languages: ["python"]
    pattern: 'DEBUG\s*=\s*True'
    message: Debug mode should be disabled in production
    fix: Set DEBUG=False or use environment variable
    category: config

  - id: PROJ-003
    name: Hardcoded Database URL
    severity: high
    cwe: "CWE-798"
    languages: []
    pattern: '(?:postgres|mysql|mongodb)://[^/\s]+:[^/\s]+@'
    message: Database connection string with credentials detected
    fix: Use environment variables for database credentials
    category: secrets
```

### Step 2: Load the rules

Custom rules can be loaded from any YAML file on disk. The security scanner will merge them with the bundled rules.

### Step 3: Test your patterns

Before deploying a rule, test the regex pattern against sample code:

```python
# This SHOULD match PROJ-001:
db.execute(f"SELECT * FROM users WHERE id = {user_id}")

# This should NOT match PROJ-001:
db.execute("SELECT * FROM users WHERE id = %s", (user_id,))
```

## Example: Full Custom Ruleset

```yaml
name: E-Commerce Security Rules
version: "1.0.0"
description: Rules for our e-commerce platform

rules:
  # Payment security
  - id: ECOM-001
    name: Raw Credit Card Number
    severity: critical
    cwe: "CWE-311"
    owasp: "A02:2021"
    languages: []
    pattern: '(?:card_number|cc_num|credit_card)\s*=\s*[''"]?\d{13,19}'
    message: Potential credit card number in source code
    fix: Never store raw card numbers; use a payment processor token
    category: secrets

  # API security
  - id: ECOM-002
    name: Missing Rate Limit
    severity: medium
    languages: ["python"]
    pattern: '@app\.route\([^)]*methods.*POST'
    message: POST endpoint may need rate limiting
    fix: Add rate limiting middleware to prevent abuse
    category: authentication

  # Data handling
  - id: ECOM-003
    name: Logging Sensitive Data
    severity: high
    cwe: "CWE-532"
    languages: ["python", "javascript"]
    pattern: '(?:logger?\.(?:info|debug|warn)|console\.log)\([^)]*(?:password|secret|token|api_key)'
    message: Sensitive data may be written to logs
    fix: Redact sensitive fields before logging
    category: secrets

  # Framework-specific
  - id: ECOM-004
    name: Mass Assignment
    severity: high
    cwe: "CWE-915"
    owasp: "A08:2021"
    languages: ["python"]
    pattern: '\.objects\.create\(\*\*request\.'
    message: Mass assignment from request data without validation
    fix: Explicitly list allowed fields instead of passing request data directly
    category: injection
```

## Bundled Rule Examples

### OWASP Top 10 rules detect:
- SQL injection via string concatenation
- XSS via innerHTML/document.write
- Path traversal via user-controlled file paths
- Insecure deserialization (pickle, yaml.load, eval)
- Weak cryptographic algorithms (MD5, SHA1 for passwords)
- Hardcoded credentials and debug mode

### CWE Top 25 rules detect:
- Command injection (os.system, exec, child_process)
- Buffer overflow patterns (strcpy, sprintf, gets)
- Use-after-free indicators
- Integer overflow in arithmetic
- Improper input validation

### Crypto rules detect:
- MD5/SHA1 usage for security purposes
- ECB mode encryption
- Small RSA key sizes
- Hardcoded initialization vectors
- Insecure random number generation (Math.random, random.random)

### Secret detection rules detect:
- AWS access keys and secret keys
- Google Cloud API keys
- GitHub tokens
- Slack tokens and webhooks
- Generic API keys and passwords in code
- Private keys (RSA, SSH, PGP)
- JWT secrets
