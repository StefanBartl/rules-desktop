# Dependencies

### `DEP-01` — pin every dependency

```rule
id = "DEP-01",
severity = "recommended",
check = { type = "grep", pattern = "foo" },
```

Every dependency is pinned to an exact version.

### `DEP-02` — no wildcard

```rule
id = "DEP-02",
severity = "critical",
```

A version range never ends in a wildcard.
