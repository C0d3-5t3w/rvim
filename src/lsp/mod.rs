use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio, Child};
use std::sync::{Arc, Mutex};
use std::env;
use std::fs;
use log::{info, error, warn};

/// Map file extensions to language IDs
fn get_language_id_from_extension(ext: &str) -> Option<&'static str> {
    match ext.to_lowercase().as_str() {
        "rs" => Some("rust"),
        "go" => Some("go"),
        "js" | "jsx" => Some("javascript"),
        "ts" | "tsx" => Some("typescript"),
        "py" => Some("python"),
        "c" | "h" => Some("c"),
        "cpp" | "hpp" | "cc" | "cxx" => Some("cpp"),
        "java" => Some("java"),
        "lua" => Some("lua"),
        "rb" => Some("ruby"),
        "php" => Some("php"),
        "html" => Some("html"),
        "css" => Some("css"),
        "json" => Some("json"),
        "md" | "markdown" => Some("markdown"),
        "yaml" | "yml" => Some("yaml"),
        "toml" => Some("toml"),
        "xml" => Some("xml"),
        "sh" | "bash" => Some("bash"),
        _ => None,
    }
}

/// Define known LSP server configurations
struct LspServerConfig {
    language_id: &'static str,
    executable: &'static str,
    args: Vec<&'static str>,
    installation_paths: Vec<PathBuf>, // Possible locations to look for the server
    installation_check: fn() -> bool, // Function to check if the server is installed
    install_command: &'static str,    // Command to suggest for installation
}

/// LSP error structure
#[derive(Debug, Clone)]
pub struct LspError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl fmt::Display for LspError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LSP Error {}: {}", self.code, self.message)
    }
}

impl Error for LspError {}

/// Active language server process
pub struct LanguageServer {
    language_id: String,
    process: Child,
    root_dir: PathBuf,
    capabilities: serde_json::Value,
    initialized: bool,
}

impl LanguageServer {
    pub fn new(language_id: &str, executable: &str, args: &[&str], root_dir: &Path) -> Result<Self, Box<dyn Error>> {
        info!("Starting language server for {}: {} {:?}", language_id, executable, args);
        
        let process = Command::new(executable)
            .args(args)
            .current_dir(root_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        
        Ok(Self {
            language_id: language_id.to_string(),
            process,
            root_dir: root_dir.to_path_buf(),
            capabilities: serde_json::Value::Null,
            initialized: false,
        })
    }
    
    pub fn shutdown(&mut self) -> Result<(), Box<dyn Error>> {
        info!("Shutting down language server for {}", self.language_id);
        self.process.kill()?;
        Ok(())
    }
}

impl Drop for LanguageServer {
    fn drop(&mut self) {
        if let Err(e) = self.process.kill() {
            error!("Failed to kill language server process: {}", e);
        }
    }
}

/// LSP Manager that scans for and manages language servers
pub struct LspManager {
    servers: HashMap<String, Arc<Mutex<LanguageServer>>>,
    server_configs: Vec<LspServerConfig>,
    workspace_root: PathBuf,
}

impl LspManager {
    pub fn new(workspace_root: PathBuf) -> Self {
        // Define known LSP server configurations
        let configs = vec![
            // Rust Analyzer
            LspServerConfig {
                language_id: "rust",
                executable: "rust-analyzer",
                args: vec![],
                installation_paths: vec![
                    PathBuf::from("/usr/local/bin/rust-analyzer"),
                    PathBuf::from("/usr/bin/rust-analyzer"),
                    dirs::home_dir().unwrap_or_default().join(".cargo/bin/rust-analyzer"),
                ],
                installation_check: || {
                    Command::new("rust-analyzer").arg("--version").output().is_ok() ||
                    Command::new("rustup").args(["component", "list", "--installed"]).output()
                        .map(|output| String::from_utf8_lossy(&output.stdout).contains("rust-analyzer"))
                        .unwrap_or(false)
                },
                install_command: "rustup component add rust-analyzer",
            },
            // Python - pyright
            LspServerConfig {
                language_id: "python",
                executable: "pyright-langserver",
                args: vec!["--stdio"],
                installation_paths: vec![
                    PathBuf::from("/usr/local/bin/pyright-langserver"),
                    PathBuf::from("/usr/bin/pyright-langserver"),
                    dirs::home_dir().unwrap_or_default().join(".local/bin/pyright-langserver"),
                ],
                installation_check: || {
                    Command::new("pyright-langserver").arg("--version").output().is_ok() ||
                    Command::new("npm").args(["list", "-g", "pyright"]).output()
                        .map(|output| String::from_utf8_lossy(&output.stdout).contains("pyright"))
                        .unwrap_or(false)
                },
                install_command: "npm install -g pyright",
            },
            // TypeScript
            LspServerConfig {
                language_id: "typescript",
                executable: "typescript-language-server",
                args: vec!["--stdio"],
                installation_paths: vec![
                    PathBuf::from("/usr/local/bin/typescript-language-server"),
                    PathBuf::from("/usr/bin/typescript-language-server"),
                    dirs::home_dir().unwrap_or_default().join(".local/bin/typescript-language-server"),
                ],
                installation_check: || {
                    Command::new("typescript-language-server").arg("--version").output().is_ok() ||
                    Command::new("npm").args(["list", "-g", "typescript-language-server"]).output()
                        .map(|output| String::from_utf8_lossy(&output.stdout).contains("typescript-language-server"))
                        .unwrap_or(false)
                },
                install_command: "npm install -g typescript typescript-language-server",
            },
            // Lua - sumneko_lua
            LspServerConfig {
                language_id: "lua",
                executable: "lua-language-server",
                args: vec![],
                installation_paths: vec![
                    PathBuf::from("/usr/local/bin/lua-language-server"),
                    PathBuf::from("/usr/bin/lua-language-server"),
                    dirs::home_dir().unwrap_or_default().join(".local/bin/lua-language-server"),
                ],
                installation_check: || {
                    Command::new("lua-language-server").arg("--version").output().is_ok()
                },
                install_command: "See https://github.com/sumneko/lua-language-server/wiki/Build-and-Run for installation instructions",
            },
            // Go
            LspServerConfig {
                language_id: "go",
                executable: "gopls",
                args: vec![],
                installation_paths: vec![
                    PathBuf::from("/usr/local/bin/gopls"),
                    PathBuf::from("/usr/bin/gopls"),
                    dirs::home_dir().unwrap_or_default().join("go/bin/gopls"),
                ],
                installation_check: || {
                    Command::new("gopls").arg("version").output().is_ok()
                },
                install_command: "go install golang.org/x/tools/gopls@latest",
            },
        ];
        
        Self {
            servers: HashMap::new(),
            server_configs: configs,
            workspace_root,
        }
    }
    
    // Scan system for installed language servers
    pub fn scan_for_language_servers(&self) -> Vec<String> {
        let mut found_servers = Vec::new();
        
        for config in &self.server_configs {
            // Check if executable exists in PATH
            if (config.installation_check)() {
                found_servers.push(config.language_id.to_string());
                info!("Found language server for {}", config.language_id);
                continue;
            }
            
            // Check for language server in known installation paths
            for path in &config.installation_paths {
                if path.exists() && path.is_file() {
                    found_servers.push(config.language_id.to_string());
                    info!("Found language server for {} at {:?}", config.language_id, path);
                    break;
                }
            }
        }
        
        found_servers
    }
    
    // Get language ID for a given file
    pub fn get_language_id_for_file(&self, file_path: &Path) -> Option<String> {
        if let Some(ext) = file_path.extension() {
            if let Some(ext_str) = ext.to_str() {
                if let Some(lang_id) = get_language_id_from_extension(ext_str) {
                    return Some(lang_id.to_string());
                }
            }
        }
        None
    }
    
    // Start a language server for a specific file if available
    pub fn start_server_for_file(&mut self, file_path: &Path) -> Result<Option<String>, Box<dyn Error>> {
        if let Some(lang_id) = self.get_language_id_for_file(file_path) {
            // Check if server for this language is already running
            if self.servers.contains_key(&lang_id) {
                info!("Language server for {} is already running", lang_id);
                return Ok(Some(lang_id));
            }
            
            // Find server config for this language
            for config in &self.server_configs {
                if config.language_id == lang_id {
                    // Check if server is installed
                    if (config.installation_check)() {
                        // Server is installed, start it
                        match LanguageServer::new(
                            config.language_id, 
                            config.executable, 
                            &config.args, 
                            &self.workspace_root
                        ) {
                            Ok(server) => {
                                info!("Started language server for {}", lang_id);
                                self.servers.insert(lang_id.clone(), Arc::new(Mutex::new(server)));
                                return Ok(Some(lang_id));
                            },
                            Err(e) => {
                                error!("Failed to start language server for {}: {}", lang_id, e);
                                return Err(e);
                            }
                        }
                    } else {
                        // Server not installed, suggest how to install
                        warn!("Language server for {} not found. Install with: {}", lang_id, config.install_command);
                        return Ok(None);
                    }
                }
            }
            
            warn!("No server configuration found for language: {}", lang_id);
        } else {
            info!("No language server available for file: {:?}", file_path);
        }
        
        Ok(None)
    }
    
    // Shutdown all running servers
    pub fn shutdown_all_servers(&mut self) -> Result<(), Box<dyn Error>> {
        for (lang_id, server) in self.servers.iter() {
            if let Ok(mut server) = server.lock() {
                if let Err(e) = server.shutdown() {
                    error!("Error shutting down language server for {}: {}", lang_id, e);
                }
            }
        }
        
        self.servers.clear();
        Ok(())
    }
    
    // Get a reference to a running server
    pub fn get_server(&self, language_id: &str) -> Option<Arc<Mutex<LanguageServer>>> {
        self.servers.get(language_id).cloned()
    }
}

impl Drop for LspManager {
    fn drop(&mut self) {
        if let Err(e) = self.shutdown_all_servers() {
            error!("Error shutting down language servers: {}", e);
        }
    }
}
