-- Neovim API compatibility layer for RVim

-- Define _G.rvim for compatibility with RVim's native API
_G.rvim = _G.rvim or {}

-- Initialize the vim global table
_G.vim = _G.vim or {}

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

-- Command execution - Use rvim's execute_command for compatibility
vim.cmd = function(cmd)
    return rvim.execute_command and rvim.execute_command(cmd) or nil
end

-- Define the version function
vim.version = function()
    return {
        major = 0, 
        minor = 9,
        patch = 0,
    }
end

-- Basic API functions
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

-- Plugin management
vim.fn.stdpath = function(what)
    if what == "config" then
        return rvim.config_path and rvim.config_path() or nil
    elseif what == "data" then
        return rvim.data_path and rvim.data_path() or nil
    elseif what == "cache" then
        return rvim.cache_path and rvim.cache_path() or nil
    end
    return nil
end

-- Implement plugin loader
vim.loader = {
    enabled = true,
}

-- Implement required functions for lazy.nvim
vim.loop = {
    fs_stat = function(path)
        return rvim.fs_stat and rvim.fs_stat(path) or nil
    end,
    fs_mkdir = function(path, mode)
        return rvim.fs_mkdir and rvim.fs_mkdir(path, mode) or nil
    end
}

-- Return the compatibility module
return vim
