---@mod tessera Tessera Neovim client (tes-lsp)
local M = {}

---@class tessera.Config
---@field cmd? string[] Command used to start tes-lsp (default: resolve on PATH / repo target/)
---@field root_markers? string[] Markers for workspace root (default: { ".git" })
---@field autostart? boolean Attach on FileType tes (default: true)

---@type tessera.Config
local defaults = {
  cmd = nil,
  root_markers = { ".git" },
  autostart = true,
}

---@type tessera.Config
M.config = vim.deepcopy(defaults)

---Resolve `tes-lsp` binary: config.cmd, PATH, then repo `target/debug|release`.
---@return string[]|nil
local function resolve_cmd()
  if M.config.cmd and #M.config.cmd > 0 then
    return M.config.cmd
  end

  if vim.fn.executable("tes-lsp") == 1 then
    return { "tes-lsp" }
  end

  -- Walk up from this plugin file: contrib/nvim/lua/tessera/init.lua → repo root.
  local src = debug.getinfo(1, "S").source:sub(2)
  local nvim_root = vim.fs.dirname(vim.fs.dirname(vim.fs.dirname(src)))
  local repo_root = vim.fs.dirname(vim.fs.dirname(nvim_root))
  for _, rel in ipairs({ "target/debug/tes-lsp", "target/release/tes-lsp" }) do
    local candidate = vim.fs.joinpath(repo_root, rel)
    if vim.fn.executable(candidate) == 1 then
      return { candidate }
    end
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

  return vim.lsp.start({
    name = "tes-lsp",
    cmd = cmd,
    root_dir = root_dir(bufnr),
  }, {
    bufnr = bufnr,
  })
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

---@param opts? tessera.Config
function M.setup(opts)
  if vim.g.tessera_setup_done then
    -- Allow re-configure (e.g. lazy opts after plugin/ autoload).
    M.config = vim.tbl_deep_extend("force", M.config, opts or {})
    return
  end
  vim.g.tessera_setup_done = true
  M.config = vim.tbl_deep_extend("force", defaults, opts or {})

  vim.api.nvim_create_user_command("TesseraLspRestart", function()
    M.restart()
  end, { desc = "Restart tes-lsp for the current buffer" })

  if M.config.autostart == false then
    return
  end

  local group = vim.api.nvim_create_augroup("tessera.lsp", { clear = true })
  vim.api.nvim_create_autocmd("FileType", {
    group = group,
    pattern = "tes",
    callback = function(args)
      M.start(args.buf)
    end,
  })
  -- Catch `.tes` opens when filetype detection is off / late (e.g. nvim -u NONE).
  vim.api.nvim_create_autocmd({ "BufReadPost", "BufNewFile" }, {
    group = group,
    pattern = "*.tes",
    callback = function(args)
      if vim.bo[args.buf].filetype ~= "tes" then
        vim.bo[args.buf].filetype = "tes"
      end
      M.start(args.buf)
    end,
  })

  -- Attach to buffers already open before setup ran (common with `nvim file -c …`).
  for _, bufnr in ipairs(vim.api.nvim_list_bufs()) do
    if vim.api.nvim_buf_is_loaded(bufnr) then
      local name = vim.api.nvim_buf_get_name(bufnr)
      if name:match("%.[tT][eE][sS]$") or vim.bo[bufnr].filetype == "tes" then
        if vim.bo[bufnr].filetype ~= "tes" then
          vim.bo[bufnr].filetype = "tes"
        end
        M.start(bufnr)
      end
    end
  end
end

return M
