-- Runs the checks of one family of a rule set with the engine's own runner, as `:Rules check`
-- would. Only tests/engine.rs uses it, after the hub has read the fixture vault, to show that the
-- canary of the fixture (LUA-01) does write its marker when a check runs: so its marker missing
-- before is the proof that the hub ran none.
--
-- nvim --headless -u NONE -i NONE -n -l run-check.lua <rules.nvim> <lib.nvim> <rule set> <family>
local engine_dir, lib_dir, ruleset, family = arg[1], arg[2], arg[3], arg[4]
vim.opt.rtp:prepend(engine_dir)
vim.opt.rtp:prepend(lib_dir)
local loader = require("rules.engine.loader")
local runner = require("rules.engine.runner")
local loaded = loader.load({ ruleset })
local of_family = {}
for _, rule in ipairs(loaded.rules or loaded) do
  if tostring(rule.id):sub(1, #family + 1) == family .. "-" then
    table.insert(of_family, rule)
  end
end
if #of_family == 0 then
  io.stderr:write("no rule of the family " .. family .. "\n")
  os.exit(1)
end
for _, result in ipairs(runner.check_family(of_family, family, vim.fn.tempname())) do
  print(result.status)
end
