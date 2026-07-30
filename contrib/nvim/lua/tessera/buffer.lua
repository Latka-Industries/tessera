---@mod tessera.buffer Tessprek projection for `.tes` buffers
local M = {}

---Resolve `tes` CLI: repo `target/debug|release/tes` first, then PATH.
---Preferring the checkout build avoids a stale `cargo install` on PATH
---failing on newer fixtures (e.g. `InlineKind::Underline`).
---@return string|nil
function M.resolve_tes_cli()
  local src = debug.getinfo(1, "S").source:sub(2)
  local nvim_root = vim.fs.dirname(vim.fs.dirname(vim.fs.dirname(src)))
  local repo_root = vim.fs.dirname(vim.fs.dirname(nvim_root))
  for _, rel in ipairs({ "target/debug/tes", "target/release/tes" }) do
    local candidate = vim.fs.joinpath(repo_root, rel)
    if vim.fn.executable(candidate) == 1 then
      return candidate
    end
  end
  if vim.fn.executable("tes") == 1 then
    return "tes"
  end
  return nil
end

---Replace buffer contents with `tes edit-read` Tessprek projection.
---@param bufnr integer
---@return boolean ok
function M.project(bufnr)
  local path = vim.api.nvim_buf_get_name(bufnr)
  if path == "" or not path:match("%.[tT][eE][sS]$") then
    return false
  end

  local tes = M.resolve_tes_cli()
  if not tes then
    vim.notify(
      "tessera.nvim: `tes` CLI not found (build with `cargo build --bin tes`)",
      vim.log.levels.ERROR
    )
    return false
  end

  local result = vim.system({ tes, "edit-read", path }, { text = true }):wait()
  if result.code ~= 0 then
    local err = (result.stderr or result.stdout or ""):gsub("%s+$", "")
    vim.notify("tessera.nvim: edit-read failed: " .. err, vim.log.levels.ERROR)
    return false
  end

  local text = result.stdout or ""
  local lines = vim.split(text, "\n", { plain = true })
  if lines[#lines] == "" then
    table.remove(lines)
  end

  local bo = vim.bo[bufnr]
  bo.binary = false
  bo.modifiable = true
  -- Mark projected before filetype so FileType autocommands do not re-enter project().
  vim.b[bufnr].tessera_projected = true
  vim.api.nvim_buf_set_lines(bufnr, 0, -1, false, lines)
  bo.modified = false
  if bo.filetype ~= "tes" then
    bo.filetype = "tes"
  end
  return true
end

---Normalize Tessprek in-buffer via `tes format --stdin` (Markdown → roles / chunk ids).
---@param bufnr integer
---@return boolean ok
function M.format(bufnr)
  local tes = M.resolve_tes_cli()
  if not tes then
    vim.notify(
      "tessera.nvim: `tes` CLI not found (build with `cargo build --bin tes`)",
      vim.log.levels.ERROR
    )
    return false
  end

  local lines = vim.api.nvim_buf_get_lines(bufnr, 0, -1, false)
  local text = table.concat(lines, "\n")
  if text ~= "" and not text:match("\n$") then
    text = text .. "\n"
  end

  local result = vim.system({ tes, "format", "--stdin" }, { text = true, stdin = text }):wait()
  if result.code ~= 0 then
    local err = (result.stderr or result.stdout or ""):gsub("%s+$", "")
    vim.notify("tessera.nvim: format failed: " .. err, vim.log.levels.ERROR)
    return false
  end

  local out = result.stdout or ""
  local out_lines = vim.split(out, "\n", { plain = true })
  if out_lines[#out_lines] == "" then
    table.remove(out_lines)
  end

  local cur = table.concat(lines, "\n")
  local next = table.concat(out_lines, "\n")
  if cur == next then
    return true
  end

  local view = vim.fn.winsaveview()
  vim.api.nvim_buf_set_lines(bufnr, 0, -1, false, out_lines)
  vim.fn.winrestview(view)
  return true
end

---Write Tessprek back via LSP `tessera.write` (does not dump buffer bytes onto `.tes`).
---@param bufnr integer
---@return boolean ok
function M.write(bufnr)
  local clients = vim.lsp.get_clients({ bufnr = bufnr, name = "tes-lsp" })
  if #clients == 0 then
    vim.notify("tessera.nvim: no tes-lsp client — cannot write", vim.log.levels.ERROR)
    return false
  end

  local uri = vim.uri_from_bufnr(bufnr)
  local results, err = vim.lsp.buf_request_sync(bufnr, "workspace/executeCommand", {
    command = "tessera.write",
    arguments = { uri },
  }, 15000)

  if err then
    vim.notify("tessera.nvim: tessera.write error: " .. tostring(err), vim.log.levels.ERROR)
    return false
  end
  if not results then
    vim.notify("tessera.nvim: tessera.write timed out / no response", vim.log.levels.ERROR)
    return false
  end

  local client_id = clients[1].id
  local resp = results[client_id]
  if not resp then
    -- Fallback: first result entry
    local _, first = next(results)
    resp = first
  end
  if not resp then
    vim.notify("tessera.nvim: tessera.write empty response", vim.log.levels.ERROR)
    return false
  end
  if resp.err then
    vim.notify("tessera.nvim: tessera.write error: " .. vim.inspect(resp.err), vim.log.levels.ERROR)
    return false
  end

  local result = resp.result
  if type(result) == "table" and result.ok then
    vim.bo[bufnr].modified = false
    local hash = result.source_hash
    local short = hash and hash:sub(1, 12) or "?"
    vim.notify(
      "tessera.nvim: wrote " .. vim.api.nvim_buf_get_name(bufnr) .. " (" .. short .. "…)",
      vim.log.levels.INFO
    )
    return true
  end

  local code = type(result) == "table" and result.code or "unknown"
  vim.notify("tessera.nvim: write refused (" .. tostring(code) .. ")", vim.log.levels.ERROR)
  return false
end

return M
