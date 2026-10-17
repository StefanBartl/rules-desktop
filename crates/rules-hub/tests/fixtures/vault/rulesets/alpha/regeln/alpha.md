# Alpha rules

Synthetic rules for the tests of rules-desktop.

#### `AA-01` - Never ignore an error

```rule
id = "AA-01",
severity = "critical",
check = { type = "grep", include = "%.lua$", pattern = "TODO" },
```

An error value is handled or passed on, never dropped. <b>Not markup</b>.

#### `AA-02` - Name things by what they are

```rule
id = "AA-02",
severity = "recommended",
agent = { include = { "**/*.lua" }, question = "Are the names honest?" },
```

A name says what a thing is.

#### `AA-03` - Keep a function short

```rule
id = "AA-03",
severity = "nice-to-have",
```

A function that does not fit on a screen is split.

