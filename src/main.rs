#![allow(unused)]

use std::env;
use std::fs;
use std::path::PathBuf;
use simplelog::*;
use std::fs::File;
use log::info;

mod cli;
mod lsp;
mod error;

use error::{Error, Result};

fn main() -> Result<()> {
    // Initialize logging
    let log_file = File::create("rvim.log")?;
    CombinedLogger::init(vec![
        WriteLogger::new(LevelFilter::Info, Config::default(), log_file),
    ])?;
    
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let filename = args.get(1).map(|s| s.as_str());
    
    // Load configuration
    let config_path = get_config_path()?;
    
    // Initialize plugin manager
    let mut plugin_manager = cli::plugin::PluginManager::new(&config_path);
    plugin_manager.discover_plugins()?;
    
    // Initialize and run the editor
    let mut editor = cli::editor::Editor::new(config_path)?;
    
    // Set up plugin manager in the editor
    editor.set_plugin_manager(plugin_manager)?;
    
    if let Some(file) = filename {
        editor.open_file(file)?;
    }
    
    editor.run()
}

fn get_config_path() -> Result<PathBuf> {
    // First try the user's configuration directory (installed location)
    let user_config_path = dirs::config_dir()
        .ok_or_else(|| Error::ConfigError("Could not find config directory".to_string()))?
        .join("rvim");
    
    if user_config_path.exists() && user_config_path.join("config.lua").exists() {
        info!("Using installed config at: {:?}", user_config_path);
        return Ok(user_config_path);
    }
    
    // If user config doesn't exist or is incomplete, check if we're running from source
    let current_dir = env::current_dir()?;
    // Fix the source config path - don't use ~ as it's not expanded automatically
    let source_config_path = current_dir.join("config");
    
    if source_config_path.exists() && source_config_path.join("config.lua").exists() {
        info!("Using source config at: {:?}", source_config_path);
        return Ok(source_config_path);
    }
    
    // If neither exists, create the user config directory and copy default config
    fs::create_dir_all(&user_config_path)?;
    
    // Create default config.lua using a hardcoded default configuration
    let default_config_path = user_config_path.join("config.lua");
    if !default_config_path.exists() {
        info!("Creating default config at: {:?}", default_config_path);
        let default_config = r#"-- RVim Default Configuration

-- Define _G.rvim if it doesn't exist
_G.rvim = _G.rvim or {}
_G.rvim.api = _G.rvim.api or { get_version = function() return "0.1.0" end }
_G.rvim.command = _G.rvim.command or {}

-- Print version info at startup
print("Loading RVim " .. rvim.api.get_version())

-- Define key mappings
-- Mode can be: 'n' (normal), 'i' (insert), 'v' (visual), 'c' (command)
rvim.map('n', '<C-s>', ':w<CR>')             -- Ctrl+S to save in normal mode
rvim.map('n', '<space>e', 'toggle_file_tree') -- Space+e to toggle file explorer
rvim.map('n', '<space>v', 'open_vertical_shell')   -- Space+v for vertical shell
rvim.map('n', '<space>h', 'open_horizontal_shell') -- Space+h for horizontal shell
rvim.map('n', '<space>w', 'cycle_window')     -- Space+w to cycle through windows
rvim.map('n', '<space>q', 'close_window')     -- Space+q to close the current window
rvim.map('n', '<space>x', 'close_buffer')     -- Space+x to close the current buffer

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
end"#;
        
        fs::write(default_config_path, default_config)?;
    }
    
    Ok(user_config_path)
}
