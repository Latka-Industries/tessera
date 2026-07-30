---@mod tessera Tessera Neovim client (tes-lsp)
local buffer = require("tessera.buffer")

local M = {}

---@class tessera.Config
---@field cmd? string[] Command used to start tes-lsp (default: resolve on PATH / repo target/)
---@field root_markers? string[] Markers for workspace root (default: { ".git" })
---@field autostart? boolean Attach on FileType / *.tes (default: true)
---@field project? boolean Replace `.tes` buffer with Tessprek via `tes edit-read` (default: true)
---@field format_on_save? boolean Run `tes format` before write-back (default: false)
---@field conceal_directives? boolean Conceal `<!-- tes … -->` / header lines (default: false)

---@type tessera.Config
local defaults = {
  cmd = nil,
  root_markers = { ".git" },
  autostart = true,
  project = true,
  format_on_save = false,
  conceal_directives = false,
}

---@type tessera.Config
M.config = vim.deepcopy(defaults)

---Resolve `tes-lsp` binary: config.cmd, repo `target/`, then PATH.
---@return string[]|nil
local function resolve_cmd()
  if M.config.cmd and #M.config.cmd > 0 then
    return M.config.cmd
  end

  local src = debug.getinfo(1, "S").source:sub(2)
  local nvim_root = vim.fs.dirname(vim.fs.dirname(vim.fs.dirname(src)))
  local repo_root = vim.fs.dirname(vim.fs.dirname(nvim_root))
  for _, rel in ipairs({ "target/debug/tes-lsp", "target/release/tes-lsp" }) do
    local candidate = vim.fs.joinpath(repo_root, rel)
    if vim.fn.executable(candidate) == 1 then
      return { candidate }
    end
  end

  if vim.fn.executable("tes-lsp") == 1 then
    return { "tes-lsp" }
  end

  return nil
end

---@param bufnr integer
---@return string
local function root_dir(bufnr)
  local path = vim.api.nvim_buf_get_name(bufnr)
  local markers = M.config.root_markers or defaults.root_markers
  local found = vim.fs.root(path ~= "" and path or vim.uv.cwd(), markers)
  return found or vim.uv.cwd()
end

---@param bufnr integer
---@return boolean
local function is_tes_buf(bufnr)
  local name = vim.api.nvim_buf_get_name(bufnr)
  return name:match("%.[tT][eE][sS]$") ~= nil or vim.bo[bufnr].filetype == "tes"
end

---Project Tessprek into the buffer (once), then attach tes-lsp.
---@param bufnr integer
local function open_tes(bufnr)
  if not vim.api.nvim_buf_is_valid(bufnr) or not is_tes_buf(bufnr) then
    return
  end
  if vim.b[bufnr].tessera_opening then
    return
  end
  vim.b[bufnr].tessera_opening = true

  local ok = true
  if M.config.project ~= false and not vim.b[bufnr].tessera_projected then
    ok = buffer.project(bufnr)
  elseif vim.bo[bufnr].filetype ~= "tes" then
    vim.bo[bufnr].filetype = "tes"
  end

  vim.b[bufnr].tessera_opening = nil
  if not ok then
    return
  end

  -- Defer so projection + filetype settle before the client sends didOpen.
  vim.schedule(function()
    if vim.api.nvim_buf_is_valid(bufnr) then
      M.start(bufnr)
    end
  end)
end

---Start or attach tes-lsp for `bufnr` (default: current).
---@param bufnr? integer
---@return integer|nil client_id
function M.start(bufnr)
  bufnr = bufnr or vim.api.nvim_get_current_buf()
  local cmd = resolve_cmd()
  if not cmd then
    vim.notify(
      "tessera.nvim: tes-lsp not found (build with `cargo build --bin tes-lsp` or set opts.cmd)",
      vim.log.levels.ERROR
    )
    return nil
  end

  local id = vim.lsp.start({
    name = "tes-lsp",
    cmd = cmd,
    root_dir = root_dir(bufnr),
  }, {
    bufnr = bufnr,
  })
  if not id then
    vim.notify(
      "tessera.nvim: vim.lsp.start failed for " .. table.concat(cmd, " "),
      vim.log.levels.ERROR
    )
  end
  return id
end

---Stop tes-lsp clients attached to `bufnr`, then start again.
---@param bufnr? integer
function M.restart(bufnr)
  bufnr = bufnr or vim.api.nvim_get_current_buf()
  local clients = vim.lsp.get_clients({ bufnr = bufnr, name = "tes-lsp" })
  for _, client in ipairs(clients) do
    client:stop(true)
  end
  vim.defer_fn(function()
    if vim.api.nvim_buf_is_valid(bufnr) then
      M.start(bufnr)
    end
  end, 100)
end

---Re-run `tes edit-read` into the current buffer (discards unsaved edits).
---@param bufnr? integer
function M.project(bufnr)
  bufnr = bufnr or vim.api.nvim_get_current_buf()
  vim.b[bufnr].tessera_projected = nil
  open_tes(bufnr)
end

---Normalize Tessprek directives from Markdown-shaped bodies (`tes format`).
---@param bufnr? integer
---@return boolean ok
function M.format(bufnr)
  bufnr = bufnr or vim.api.nvim_get_current_buf()
  return buffer.format(bufnr)
end

---@param bufnr integer
local function apply_conceal(bufnr)
  if M.config.conceal_directives == false then
    return
  end
  vim.api.nvim_buf_call(bufnr, function()
    vim.opt_local.conceallevel = 2
    vim.opt_local.concealcursor = "nvic"
    pcall(vim.cmd, [[syntax match TesseraDirective /<!--\s*tes\%(sera:\)\?\_.\{-}-->/ conceal]])
    pcall(vim.cmd, "highlight default link TesseraDirective Comment")
  end)
end

---@param opts? tessera.Config
function M.setup(opts)
  if vim.g.tessera_setup_done then
    M.config = vim.tbl_deep_extend("force", M.config, opts or {})
    return
  end
  vim.g.tessera_setup_done = true
  M.config = vim.tbl_deep_extend("force", defaults, opts or {})

  vim.api.nvim_create_user_command("TesseraLspRestart", function()
    M.restart()
  end, { desc = "Restart tes-lsp for the current buffer" })

  vim.api.nvim_create_user_command("TesseraProject", function()
    M.project()
  end, { desc = "Reload Tessprek projection via tes edit-read" })

  vim.api.nvim_create_user_command("TesseraFormat", function()
    M.format()
  end, { desc = "Normalize Tessprek directives via tes format" })

  vim.api.nvim_create_user_command("TesseraLspInfo", function()
    local cmd = resolve_cmd()
    local clients = vim.lsp.get_clients({ name = "tes-lsp" })
    local lines = {
      "tes-lsp cmd: " .. (cmd and table.concat(cmd, " ") or "(not found)"),
      "tes CLI: " .. (buffer.resolve_tes_cli() or "(not found)"),
      "clients: " .. #clients,
      "projected: " .. tostring(vim.b.tessera_projected),
      "filetype: " .. vim.bo.filetype,
      "format_on_save: " .. tostring(M.config.format_on_save),
      "conceal_directives: " .. tostring(M.config.conceal_directives),
    }
    for _, c in ipairs(clients) do
      table.insert(lines, string.format("  id=%s root=%s", c.id, c.root_dir or "?"))
    end
    vim.notify(table.concat(lines, "\n"), vim.log.levels.INFO)
  end, { desc = "Show tessera.nvim / tes-lsp status" })

  if M.config.autostart == false then
    return
  end

  local group = vim.api.nvim_create_augroup("tessera.lsp", { clear = true })

  vim.api.nvim_create_autocmd("FileType", {
    group = group,
    pattern = "tes",
    callback = function(args)
      open_tes(args.buf)
      apply_conceal(args.buf)
    end,
  })

  vim.api.nvim_create_autocmd({ "BufReadPost", "BufNewFile" }, {
    group = group,
    pattern = "*.tes",
    callback = function(args)
      open_tes(args.buf)
    end,
  })

  -- Intercept `:w` so Tessprek text never overwrites the binary `.tes` on disk.
  vim.api.nvim_create_autocmd("BufWriteCmd", {
    group = group,
    pattern = "*.tes",
    callback = function(args)
      if M.config.format_on_save then
        if not buffer.format(args.buf) then
          return
        end
      end
      buffer.write(args.buf)
    end,
  })

  for _, bufnr in ipairs(vim.api.nvim_list_bufs()) do
    if vim.api.nvim_buf_is_loaded(bufnr) and is_tes_buf(bufnr) then
      open_tes(bufnr)
      apply_conceal(bufnr)
    end
  end
end

return M
