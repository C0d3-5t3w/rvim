use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use log::info;
use mlua::{Lua, Table, Value};

/// Represents a Vim plugin
pub struct Plugin {
    pub name: String,
    pub path: PathBuf,
    pub enabled: bool,
    pub config: Option<String>,
}

/// Manages plugin loading and execution
pub struct PluginManager {
    plugins_dir: PathBuf,
    plugins: Vec<Plugin>,
    lua: Option<mlua::Lua>,
}

impl PluginManager {
    /// Create a new plugin manager
    pub fn new(config_dir: &Path) -> Self {
        let plugins_dir = config_dir.join("plugins");
        Self {
            plugins_dir,
            plugins: Vec::new(),
            lua: None,
        }
    }
    
    /// Set the Lua state used by the plugin manager
    pub fn set_lua(&mut self, lua: mlua::Lua) {
        self.lua = Some(lua);
    }
    
    /// Discover and load plugins
    pub fn discover_plugins(&mut self) -> Result<(), Box<dyn Error>> {
        if !self.plugins_dir.exists() {
            fs::create_dir_all(&self.plugins_dir)?;
        }
        
        info!("Scanning for plugins in {:?}", self.plugins_dir);
        
        // For each directory in plugins_dir, check if it contains a Lua plugin
        for entry in fs::read_dir(&self.plugins_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_dir() {
                let plugin_name = path.file_name()
                    .ok_or("Invalid plugin directory name")?
                    .to_string_lossy()
                    .to_string();
                
                // Check for plugin entry points
                let init_lua = path.join("init.lua");
                let plugin_lua = path.join("plugin").join("init.lua");
                let lua_dir = path.join("lua");
                
                if init_lua.exists() || plugin_lua.exists() || lua_dir.exists() {
                    self.plugins.push(Plugin {
                        name: plugin_name.clone(),
                        path: path.clone(),
                        enabled: true,
                        config: None,
                    });
                    
                    info!("Discovered plugin: {}", plugin_name);
                }
            }
        }
        
        Ok(())
    }
    
    /// Load all plugins into the Lua state
    pub fn load_plugins(&self) -> Result<(), Box<dyn Error>> {
        if let Some(lua) = &self.lua {
            for plugin in &self.plugins {
                if plugin.enabled {
                    info!("Loading plugin: {}", plugin.name);
                    self.load_plugin(lua, plugin)?;
                }
            }
        } else {
            return Err("Lua state not set".into());
        }
        
        Ok(())
    }
    
    /// Load a specific plugin
    fn load_plugin(&self, lua: &mlua::Lua, plugin: &Plugin) -> Result<(), Box<dyn Error>> {
        // Add plugin's lua directory to package.path
        let lua_dir = plugin.path.join("lua");
        if lua_dir.exists() {
            let package: Table = lua.globals().get("package")?;
            let current_path: String = package.get("path")?;
            
            let lua_path = format!("{}/?.lua;{}/{}?.lua;{}",
                lua_dir.to_string_lossy(), 
                lua_dir.to_string_lossy(),
                std::path::MAIN_SEPARATOR,
                current_path);
                
            package.set("path", lua_path)?;
        }
        
        // Try loading init.lua
        let init_lua = plugin.path.join("init.lua");
        if init_lua.exists() {
            let init_content = fs::read_to_string(&init_lua)?;
            lua.load(&init_content).exec()?;
        }
        
        // Try loading plugin/init.lua
        let plugin_lua = plugin.path.join("plugin").join("init.lua");
        if plugin_lua.exists() {
            let plugin_content = fs::read_to_string(&plugin_lua)?;
            lua.load(&plugin_content).exec()?;
        }
        
        Ok(())
    }
    
    /// Install a plugin from a Git repository
    pub fn install_plugin(&mut self, url: &str) -> Result<(), Box<dyn Error>> {
        // Extract plugin name from URL (last part of URL without .git)
        let name = url.split('/').last()
            .ok_or("Invalid URL format")?
            .trim_end_matches(".git");
            
        info!("Installing plugin: {} from {}", name, url);
        
        // Create plugin directory
        let plugin_dir = self.plugins_dir.join(name);
        
        if plugin_dir.exists() {
            info!("Plugin already installed: {}", name);
            return Ok(());
        }
        
        // For a real implementation, you would use a library like git2-rs
        // or spawn a git process to clone the repository
        // Here, we'll just create the directory as a placeholder
        fs::create_dir_all(&plugin_dir)?;
        
        // Create a basic init.lua file to mark this as a plugin
        let init_lua = plugin_dir.join("init.lua");
        fs::write(&init_lua, format!("-- Plugin: {}\n-- URL: {}\n\nreturn {{\n  setup = function()\n    print('Plugin {} loaded')\n  end\n}}\n", name, url, name))?;
        
        info!("Plugin {} installed successfully", name);
        
        Ok(())
    }
}
