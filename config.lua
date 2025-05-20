-- RVim Default Configuration

-- Define _G.rvim if it doesn't exist (this helps silence linter warnings)
_G.rvim = _G.rvim or {}
_G.rvim.api = _G.rvim.api or { get_version = function() return "0.1.0" end }
_G.rvim.command = _G.rvim.command or {}

-- Print version info at startup
print("Loading RVim " .. rvim.api.get_version())

-- Define key mappings
-- Mode can be: 'n' (normal), 'i' (insert), 'v' (visual), 'c' (command)
rvim.map('n', '<C-s>', ':w<CR>')             -- Ctrl+S to save in normal mode
rvim.map('n', '<space>e', 'toggle_file_tree') -- Space+e to toggle file explorer
rvim.map('n', '<space>v', 'split_vertical')   -- Space+v for vertical split
rvim.map('n', '<space>h', 'split_horizontal') -- Space+h for horizontal split
rvim.map('n', '<space>w', 'cycle_window')     -- Space+w to cycle through windows
rvim.map('n', '<space>q', 'close_window')     -- Space+q to close the current window

-- User settings
local settings = {
  number = true,           -- Show line numbers
  relativenumber = false,  -- Show relative line numbers
  tabstop = 4,             -- Tab width
  shiftwidth = 4,          -- Indentation width
  expandtab = true,        -- Use spaces instead of tabs
  syntax = true,           -- Enable syntax highlighting
  theme = "default",       -- Color theme
  file_tree = {
    width = 30,            -- Width of file tree panel
    show_hidden = false,   -- Show hidden files
  }
}

-- Define a custom function (example of plugin-like functionality)
local function hello_world()
  print("Hello from Lua!")
end

-- You can define custom commands
rvim.command.Hello = hello_world

-- Example of how plugins would be defined
local plugins = {
  {
    name = "example-plugin",
    setup = function()
      -- Plugin initialization code
      print("Example plugin loaded")
    end
  }
}

-- Configure plugins
for _, plugin in ipairs(plugins) do
  if type(plugin.setup) == "function" then
    plugin.setup()
  end
end
