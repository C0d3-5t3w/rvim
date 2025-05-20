-- RVim Default Configuration

-- Define _G.rvim if it doesn't exist
_G.rvim = _G.rvim or {}
_G.rvim.api = _G.rvim.api or { get_version = function() return "0.1.0" end }
_G.rvim.command = _G.rvim.command or {}

-- Load Neovim compatibility layer
local nvim_compat = require('nvim_compat')

-- Print version info at startup
print("Loading RVim " .. rvim.api.get_version())

-- Path to store plugins
local install_path = vim.fn.stdpath('data') .. '/site/pack/lazy/start/lazy.nvim'

-- Auto-install lazy.nvim if not present
if not vim.loop.fs_stat(install_path) then
  print("Installing lazy.nvim plugin manager...")
  vim.fn.system({
    "git", "clone", "--filter=blob:none",
    "https://github.com/folke/lazy.nvim.git",
    "--branch=stable",
    install_path,
  })
end

-- Prepend lazy.nvim to the runtimepath
vim.opt.rtp:prepend(install_path)

-- Example plugin specification with lazy.nvim
local plugins = {
  -- LSP Configuration
  {
    "neovim/nvim-lspconfig",
    dependencies = {
      "williamboman/mason.nvim",
      "williamboman/mason-lspconfig.nvim",
    },
    config = function()
      -- LSP setup would go here
      require("mason").setup()
      require("mason-lspconfig").setup({
        ensure_installed = { "lua_ls", "rust_analyzer" }
      })
      
      local lspconfig = require("lspconfig")
      lspconfig.lua_ls.setup({})
      lspconfig.rust_analyzer.setup({})
    end
  },
  
  -- Other plugins can be added here
}

-- Initialize the plugin system with lazy.nvim
if pcall(require, "lazy") then
  require("lazy").setup(plugins)
end

-- Original RVim key mappings
rvim.map('n', '<C-s>', ':w<CR>')
rvim.map('n', '<space>e', 'toggle_file_tree')
rvim.map('n', '<space>v', 'open_vertical_shell')
rvim.map('n', '<space>h', 'open_horizontal_shell')
rvim.map('n', '<space>w', 'cycle_window')
rvim.map('n', '<space>q', 'close_window')
rvim.map('n', '<space>x', 'close_buffer')

-- Tab navigation
rvim.map('n', '<space><tab>', 'next_tab')     -- Space+Tab to go to next tab
rvim.map('n', '<space><S-tab>', 'prev_tab')   -- Space+Shift+Tab to go to previous tab
rvim.map('n', '<C-tab>', 'next_tab')          -- Ctrl+Tab alternative
rvim.map('n', '<C-S-tab>', 'prev_tab')        -- Ctrl+Shift+Tab alternative

-- LSP keybindings
vim.keymap.set('n', 'gd', vim.lsp.buf.definition, { desc = "Go to definition" })
vim.keymap.set('n', 'K', vim.lsp.buf.hover, { desc = "Show hover information" })
vim.keymap.set('n', '<leader>rn', vim.lsp.buf.rename, { desc = "Rename symbol" })
vim.keymap.set('n', '<leader>ca', vim.lsp.buf.code_action, { desc = "Code actions" })

-- User settings
local settings = {
  number = true,
  relativenumber = false,
  tabstop = 4,
  shiftwidth = 4,
  expandtab = true,
  syntax = true,
  theme = "default",
  file_tree = {
    width = 30,
    show_hidden = false,
  }
}

-- Apply settings to vim.o for compatibility
for k, v in pairs(settings) do
  if type(v) ~= "table" then
    vim.o[k] = v
  end
end
