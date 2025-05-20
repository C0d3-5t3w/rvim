use std::env;
use std::fs;
use std::path::PathBuf;
use std::error::Error;
use simplelog::*;
use std::fs::File;

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
    let config_path = dirs::config_dir()
        .ok_or("Could not find config directory")?
        .join("rvim");
    
    if !config_path.exists() {
        fs::create_dir_all(&config_path)?;
        
        // Create default config.lua
        let default_config_path = config_path.join("config.lua");
        if !default_config_path.exists() {
            let default_config = include_str!("../config.lua");
            fs::write(default_config_path, default_config)?;
        }
    }
    
    Ok(config_path)
}
