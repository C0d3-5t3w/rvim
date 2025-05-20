#![allow(unused)]

use std::env;
use std::fs;
use std::path::PathBuf;
use std::error::Error;
use simplelog::*;
use std::fs::File;
use log::info;

mod cli;

fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging
    CombinedLogger::init(vec![
        WriteLogger::new(LevelFilter::Info, Config::default(), File::create("rvim.log")?),
    ])?;
    
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let filename = args.get(1).map(|s| s.as_str());
    
    // Load configuration
    let config_path = get_config_path()?;
    
    // Initialize and run the editor
    let mut editor = cli::editor::Editor::new(config_path)?;
    
    if let Some(file) = filename {
        editor.open_file(file)?;
    }
    
    editor.run()
}

fn get_config_path() -> Result<PathBuf, Box<dyn Error>> {
    // First try the user's configuration directory (installed location)
    let user_config_path = dirs::config_dir()
        .ok_or("Could not find config directory")?
        .join("rvim");
    
    if user_config_path.exists() && user_config_path.join("config.lua").exists() {
        info!("Using installed config at: {:?}", user_config_path);
        return Ok(user_config_path);
    }
    
    // If user config doesn't exist or is incomplete, check if we're running from source
    let current_dir = env::current_dir()?;
    let source_config_path = current_dir.join("config");
    
    if source_config_path.exists() && source_config_path.join("config.lua").exists() {
        info!("Using source config at: {:?}", source_config_path);
        return Ok(source_config_path);
    }
    
    // If neither exists, create the user config directory and copy default config
    fs::create_dir_all(&user_config_path)?;
    
    // Create default config.lua
    let default_config_path = user_config_path.join("config.lua");
    if !default_config_path.exists() {
        info!("Creating default config at: {:?}", default_config_path);
        let default_config = include_str!("../config.lua");
        fs::write(default_config_path, default_config)?;
    }
    
    Ok(user_config_path)
}
