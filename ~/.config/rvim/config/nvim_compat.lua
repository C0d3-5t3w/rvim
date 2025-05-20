-- Neovim API compatibility layer for RVim

-- Define _G.rvim for compatibility with RVim's native API
_G.rvim = _G.rvim or {}

-- Initialize the vim global table
_G.vim = _G.vim or {}

-- Find Neovim configuration directory
local home_dir = os.getenv("HOME") or os.getenv("USERPROFILE")
local nvim_config_path = home_dir and (home_dir .. "/.config/nvim") or nil
local nvim_config_file = nvim_config_path and io.open(nvim_config_path .. "/init.lua", "r") 
                     or nvim_config_path and io.open(nvim_config_path .. "/init.vim", "r")

if nvim_config_file then
    nvim_config_file:close()
    print("Found Neovim configuration at: " .. nvim_config_path)
else
    print("No Neovim configuration found")
end

-- Create standard Neovim namespaces
vim.g = {}  -- Global variables
vim.b = {}  -- Buffer variables
vim.w = {}  -- Window variables
vim.o = {}  -- Options
vim.bo = {} -- Buffer-local options
vim.wo = {} -- Window-local options
vim.env = {} -- Environment variables
vim.fn = {} -- Vim functions
vim.api = {} -- Neovim API
vim.lsp = {} -- LSP namespace
vim.diagnostic = {} -- Diagnostic namespace
vim.keymap = {} -- Keymap namespace

-- Path handling
vim.fn.stdpath = function(what)
    if what == "config" then
        -- Try to use Neovim's config path if available, fall back to RVim's
        return nvim_config_path or (rvim.config_path and rvim.config_path() or nil)
    elseif what == "data" then
        -- Use Neovim's data directory for plugins if available
        return home_dir and (home_dir .. "/.local/share/nvim") or (rvim.data_path and rvim.data_path() or nil)
    elseif what == "cache" then
        return home_dir and (home_dir .. "/.cache/nvim") or (rvim.cache_path and rvim.cache_path() or nil)
    end
    return nil
end

-- Core functionality
vim.cmd = function(cmd)
    return rvim.execute_command and rvim.execute_command(cmd) or nil
end

vim.version = function()
    return {
        major = 0, 
        minor = 9,
        patch = 0,
    }
end

-- API functions
vim.api.nvim_set_option = function(name, value)
    return rvim.set_option and rvim.set_option(name, value) or nil
end

vim.api.nvim_get_option = function(name)
    return rvim.get_option and rvim.get_option(name) or nil
end

vim.api.nvim_command = function(command)
    return rvim.execute_command and rvim.execute_command(command) or nil
end

vim.api.nvim_buf_set_option = function(bufnr, name, value)
    return rvim.buf_set_option and rvim.buf_set_option(bufnr, name, value) or nil
end

-- LSP functionality
vim.lsp.start = function(config)
    return rvim.lsp_start and rvim.lsp_start(config) or nil
end

vim.lsp.buf = {
    format = function(opts)
        return rvim.lsp_buf_format and rvim.lsp_buf_format(opts) or nil
    end,
    hover = function()
        return rvim.lsp_buf_hover and rvim.lsp_buf_hover() or nil
    end,
    definition = function()
        return rvim.lsp_buf_definition and rvim.lsp_buf_definition() or nil
    end,
    references = function()
        return rvim.lsp_buf_references and rvim.lsp_buf_references() or nil
    end,
}

-- Keymap functionality
vim.keymap.set = function(mode, lhs, rhs, opts)
    return rvim.map and rvim.map(mode, lhs, rhs, opts) or nil
end

-- Set up runtime path to include Neovim plugins
vim.opt = vim.opt or {}
vim.opt.rtp = {
    append = function(path) end,
    prepend = function(path) end,
}

-- Plugin loader
vim.loader = {
    enabled = true,
}

-- Required for lazy.nvim
vim.loop = {
    fs_stat = function(path)
        return rvim.fs_stat and rvim.fs_stat(path) or nil
    end,
    fs_mkdir = function(path, mode)
        return rvim.fs_mkdir and rvim.fs_mkdir(path, mode) or nil
    end
}

-- Local functions to load Neovim plugins
local function setup_nvim_plugins()
    if not nvim_config_path then return end
    
    -- Add Neovim plugin directories to runtime path
    local plugin_dirs = {
        nvim_config_path .. "/pack/*/start/*",           -- Built-in package manager
        home_dir .. "/.local/share/nvim/site/pack/*/start/*",  -- Packer
        home_dir .. "/.local/share/nvim/lazy/*",         -- lazy.nvim
    }
    
    -- Set up Lua paths
    local lua_paths = {}
    for _, path in ipairs(plugin_dirs) do
        table.insert(lua_paths, path .. "/lua/?.lua")
        table.insert(lua_paths, path .. "/lua/?/init.lua")
    end
    
    if #lua_paths > 0 then
        package.path = table.concat(lua_paths, ";") .. ";" .. package.path
    end
    
    print("RVim is using Neovim plugins from: " .. nvim_config_path)
end

-- Try to initialize Neovim plugins
if nvim_config_path then
    setup_nvim_plugins()
end

-- Return the compatibility module
return vim
