# Security Audit

Run a security analysis of your codebase using CodeGraph's built-in scanner with 65+ rules covering OWASP Top 10, CWE Top 25, cryptographic weaknesses, and secret detection.

## The Scenario

Before deploying your application, you need to:
- Find security vulnerabilities in the source code
- Check for OWASP Top 10 and CWE Top 25 issues
- Detect hardcoded secrets (API keys, passwords, tokens)
- Review cryptographic usage
- Understand the blast radius of vulnerabilities

## Step 1: Quick Security Overview

**You:** "Run a security scan on this project"

**Claude calls:**
```
codegraph_security_summary()
```

**What you see:** An aggregate view of findings by severity.

```
Security Summary:
  Critical: 2
  High: 5
  Medium: 8
  Low: 3

  Top categories:
    - injection: 3 findings
    - secrets: 2 findings
    - crypto: 4 findings
    - config: 6 findings
```

## Step 2: OWASP Top 10 Check

**You:** "Check for OWASP Top 10 vulnerabilities"

**Claude calls:**
```
codegraph_check_owasp()
```

**What you see:** Findings organized by OWASP category.

```
A03:2021 - Injection
  OWASP-A03-001 at src/api/users.py:45 (critical)
    SQL injection: query uses string concatenation with user input
    Fix: Use parameterized queries

A02:2021 - Cryptographic Failures
  OWASP-A02-001 at src/auth/hash.py:12 (high)
    Weak hash: MD5 used for password hashing
    Fix: Use Argon2, bcrypt, or scrypt

A07:2021 - Identification and Authentication Failures
  OWASP-A07-001 at src/config/settings.py:8 (medium)
    Debug mode enabled in production configuration
    Fix: Set DEBUG=False
```

## Step 3: Find Hardcoded Secrets

**You:** "Are there any hardcoded secrets in the code?"

**Claude calls:**
```
codegraph_find_secrets()
```

**What you see:** Detected secrets with file locations.

```
SECRET-001 at src/config/aws.py:5 (critical)
  AWS Access Key ID: AKIA...
  Fix: Use IAM roles or environment variables

SECRET-005 at src/services/email.py:12 (high)
  Hardcoded password: SMTP_PASSWORD = "..."
  Fix: Move to environment variable or secrets manager

SECRET-003 at .env.example:3 (info)
  Example API key (not a real secret)
```

## Step 4: Trace the Vulnerability

Once you've found a vulnerability, understand how it's reachable:

**You:** "What calls the function at src/api/users.py that has the SQL injection?"

**Claude calls:**
```
codegraph_callers(symbol: "get_user_by_id", depth: 3)
```

**What you see:** The call chain from entry points to the vulnerable code.

```
get_user_by_id (src/api/users.py:40)
  <- UserController.get (src/controllers/users.py:25)
    <- Router.handle_request (src/routes/api.py:18)
      <- Application.dispatch (src/app.py:45)
```

Now you know the full attack path: HTTP request -> router -> controller -> vulnerable query.

## Step 5: Assess Impact

**You:** "What's the blast radius if this function is compromised?"

**Claude calls:**
```
codegraph_impact(target: "src/api/users.py")
```

**What you see:**

```
Impact Analysis: src/api/users.py
  Risk:                 critical
  Direct dependents:    6
  Transitive dependents:15
  Affected files:       9
```

A SQL injection here affects 15 transitive dependents across 9 files. This is a high-priority fix.

## Step 6: Audit Cryptographic Usage

**You:** "Review all cryptographic code in the project"

**Claude calls:**
```
codegraph_crypto_audit()
```

**What you see:** All crypto-related findings.

```
CRYPTO-001 at src/auth/hash.py:12 (high)
  MD5 used for hashing - CWE-327
  Fix: Use SHA-256+ for hashing, Argon2/bcrypt for passwords

CRYPTO-003 at src/utils/encrypt.py:8 (high)
  ECB mode encryption - CWE-327
  Fix: Use CBC or GCM mode with proper IV

CRYPTO-005 at src/auth/token.py:22 (medium)
  Weak random for token generation - CWE-330
  Fix: Use secrets.token_urlsafe() or os.urandom()
```

## Step 7: Verify the Fix

After fixing vulnerabilities, re-scan to confirm:

**You:** "Re-run the security scan. Did we fix everything?"

**Claude calls:**
```
codegraph_security_summary()
```

Compare with your earlier results. The findings for fixed issues should be gone.

## Security Rule Categories

| Category | What It Catches |
|----------|----------------|
| **injection** | SQL injection, command injection, code injection, LDAP injection |
| **xss** | innerHTML, document.write, dangerouslySetInnerHTML |
| **crypto** | MD5/SHA1 for security, ECB mode, weak random, small key sizes |
| **secrets** | AWS keys, Google Cloud keys, GitHub tokens, hardcoded passwords |
| **config** | Debug mode, permissive CORS, insecure HTTP |
| **authentication** | Direct object references, open redirects, weak session handling |
| **pathtraversal** | File operations with user-controlled paths |
| **deserialization** | pickle.loads, yaml.load without SafeLoader, eval() |

## Security Scan Coverage

| Standard | Coverage |
|----------|----------|
| OWASP Top 10 2021 | A01 through A10 |
| CWE Top 25 | 25 most dangerous weaknesses |
| PCI DSS | Relevant code-level checks |
| SANS Top 25 | Mapped via CWE identifiers |

## Custom Rules

You can add project-specific security rules. See [security-rules.md](../security-rules.md) for the full YAML format.

## Sample Prompts

```
"Run a full security audit"
"Check for OWASP Top 10 vulnerabilities"
"Find all hardcoded API keys and passwords"
"Is there any SQL injection in this codebase?"
"Review cryptographic usage for weaknesses"
"Trace how user input reaches the database"
"What's the impact if this vulnerability is exploited?"
"Show me all critical and high severity findings"
```

## Related Workflows

- [Understand a Codebase](understand-codebase.md) -- Get context before auditing
- [Fix a Bug](fix-a-bug.md) -- Debug and fix the vulnerabilities you find
