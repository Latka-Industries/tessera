-- Autoload when this tree is on runtimepath.
-- lazy.nvim users who pass `opts = {}` call setup() themselves; guard against a second run.
if vim.g.tessera_loaded then
  return
end
vim.g.tessera_loaded = true

require("tessera").setup()
