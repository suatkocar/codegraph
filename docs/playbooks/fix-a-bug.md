# Fix a Bug

You have a bug report. Here's how CodeGraph helps you diagnose the root cause, understand the affected code, and verify your fix won't break anything.

## The Scenario

**Bug report:** "Users are getting 'Connection refused' errors when logging in during peak hours"

You need to:
- Find the relevant code
- Understand the call chain from entry point to failure
- Identify the root cause
- Verify your fix won't introduce regressions

## Step 1: Locate the Error

**You:** "Find where the 'Connection refused' error is handled in the codebase"

**Claude calls:**
```
codegraph_query(query: "Connection refused error login", limit: 10)
```

**What you see:** Ranked results from hybrid search (keyword + semantic).

```
1. DatabasePool.getConnection (src/db/pool.ts:45)
   Score: 0.89 - "throw new Error('Connection refused')"

2. AuthService.authenticate (src/auth/service.ts:28)
   Score: 0.72 - catches connection errors during login

3. RetryPolicy.execute (src/utils/retry.ts:15)
   Score: 0.65 - retry logic for transient failures
```

**Claude calls:**
```
codegraph_node(name: "DatabasePool.getConnection")
```

**What you see:** Full symbol details including source code, file location, and relationships.

## Step 2: Trace the Call Chain

**You:** "Show me what calls getConnection and how it relates to the login flow"

**Claude calls:**
```
codegraph_callers(symbol: "DatabasePool.getConnection", depth: 3)
```

**What you see:** The reverse call graph from the database pool up to the entry points.

```
DatabasePool.getConnection (src/db/pool.ts:45)
  <- UserRepository.findByEmail (src/repositories/user.ts:23)
    <- AuthService.authenticate (src/auth/service.ts:28)
      <- LoginController.handleLogin (src/controllers/login.ts:15)
        <- Router.post('/login') (src/routes/auth.ts:8)
  <- SessionStore.create (src/session/store.ts:12)
    <- AuthService.authenticate (src/auth/service.ts:35)
```

Now you see the full path: HTTP request -> router -> controller -> auth service -> user repository -> database pool. The `authenticate` function calls `getConnection` in two places.

## Step 3: Check What the Function Calls

**You:** "What does getConnection depend on?"

**Claude calls:**
```
codegraph_callees(symbol: "DatabasePool.getConnection", depth: 1)
```

**What you see:** Forward call graph -- what `getConnection` calls.

```
DatabasePool.getConnection
  -> Config.getPoolSize (src/config/index.ts:22)
  -> Logger.warn (src/utils/logger.ts:18)
  -> HealthCheck.ping (src/db/health.ts:5)
```

## Step 4: Understand the Impact

**You:** "If I change the connection pool, what else is affected?"

**Claude calls:**
```
codegraph_impact(target: "src/db/pool.ts")
```

**What you see:**

```
Impact Analysis: src/db/pool.ts
  Risk:                 high
  Direct dependents:    5
  Transitive dependents:18
  Affected files:       11
    - src/repositories/user.ts
    - src/repositories/order.ts
    - src/session/store.ts
    - src/auth/service.ts
    - src/controllers/login.ts
    ...
```

This file is high-impact. Changes require careful testing.

## Step 5: Check Related Tests

**You:** "Are there tests for the database pool?"

**Claude calls:**
```
codegraph_tests(symbol: "DatabasePool.getConnection")
```

**What you see:** Test files that reference this symbol.

```
Tests for DatabasePool.getConnection:
  - tests/db/pool.test.ts:12 - "should return a connection"
  - tests/db/pool.test.ts:28 - "should throw when pool exhausted"
  - tests/db/pool.test.ts:45 - "should retry on transient failure"
```

Good -- there are existing tests. Your fix should maintain compatibility with these.

## Step 6: Get Full Context for the Fix

**You:** "I need to modify the connection pool. Give me the full context"

**Claude calls:**
```
codegraph_context(query: "database connection pool timeout retry", budget: 4000)
```

**What you see:** Token-budgeted context with the 4-tier structure.

```
=== Core (40%) ===
[Full source of DatabasePool, getConnection, releaseConnection]

=== Near (25%) ===
[Signatures of callers: UserRepository.findByEmail, SessionStore.create]
[Signatures of callees: Config.getPoolSize, HealthCheck.ping]

=== Extended (20%) ===
[Related tests: pool.test.ts excerpts]
[Sibling: DatabasePool.releaseConnection]

=== Background (15%) ===
[File listing: src/db/, src/config/, src/utils/]
```

This gives you everything needed to make an informed fix without reading the entire codebase.

## Step 7: Verify After the Fix

After making your changes, verify nothing broke:

**You:** "I changed the pool timeout from 5s to 30s. What should I test?"

**Claude calls:**
```
codegraph_callers(symbol: "DatabasePool.getConnection", depth: 2)
codegraph_tests(symbol: "DatabasePool")
```

**What you see:** All callers and all tests, so you know exactly what to re-test.

## Tools Used in This Workflow

| Tool | Purpose |
|------|---------|
| `codegraph_query` | Find error messages and relevant code via hybrid search |
| `codegraph_node` | Get full symbol details with source code |
| `codegraph_callers` | Reverse call graph -- what calls this function? |
| `codegraph_callees` | Forward call graph -- what does this function call? |
| `codegraph_impact` | Blast radius -- what breaks if this changes? |
| `codegraph_tests` | Find related test coverage |
| `codegraph_context` | Get token-budgeted context for the AI agent |

## Debugging Patterns

### "Error appears randomly"
```
codegraph_query -> find the error source
codegraph_callers -> trace all paths that reach it
codegraph_impact -> understand which users/features are affected
```

### "Worked before, broke recently"
```
codegraph_query -> find the affected code
codegraph_callers -> understand the dependency chain
codegraph_tests -> check existing test coverage
```

### "Works for some users, not others"
```
codegraph_query -> find the branch point
codegraph_callees -> see what different code paths call
codegraph_dependencies -> trace configuration differences
```

### "Performance degraded"
```
codegraph_callers (transitive) -> find all paths to slow code
codegraph_impact -> understand the bottleneck's reach
codegraph_structure -> find high-PageRank symbols (load-bearing code)
```

## Sample Prompts

```
"Find where this error message comes from"
"What calls this function? Show me the full chain"
"What's the blast radius if I change this file?"
"Are there tests for this function?"
"Give me full context to fix this bug"
"What else depends on this module?"
"Show me everything related to connection handling"
"Is there retry logic anywhere in the codebase?"
```

## Related Workflows

- [Understand a Codebase](understand-codebase.md) -- Get context before debugging
- [Security Audit](security-audit.md) -- Check if the bug has security implications
