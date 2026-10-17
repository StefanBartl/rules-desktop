# Lua

### `LUA-01` — the canary

The check of this rule is a function. Run, it writes the file it is given in place of the marker
below (the test that copies this vault puts a path there); the hub must never run it, so after
every read of the hub the file must not exist.

```rule
id = "LUA-01",
severity = "critical",
check = { type = "lua_predicate", fn = function()
  local f = io.open("@MARKER@", "w")
  if f then
    f:write("a check ran")
    f:close()
  end
  return true
end },
```

Nothing the app does runs a check.
