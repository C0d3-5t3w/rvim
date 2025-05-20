use std::error::Error as StdError;
use std::fmt;
use std::io;
use std::path::PathBuf;
use std::sync::PoisonError;

/// RVim's main error type that contains all possible error variants
#[derive(Debug)]
pub enum Error {
    /// File-related errors
    Io(io::Error),
    /// File not found
    FileNotFound(PathBuf),
    /// Permission denied
    PermissionDenied(PathBuf),
    /// File already exists
    FileExists(PathBuf),
    /// Directory not found
    DirectoryNotFound(PathBuf),
    /// Invalid file name
    InvalidFileName(String),
    
    /// Configuration errors
    ConfigError(String),
    ConfigParseError {
        file: PathBuf,
        message: String,
    },
    MissingConfig(PathBuf),
    
    /// Lua errors
    LuaError(String),
    LuaExecutionError(String),
    PluginError {
        name: String,
        message: String,
    },
    
    /// LSP errors
    LspError {
        code: i32,
        message: String,
        language: Option<String>,
    },
    LspServerNotFound(String),
    LspConnectionError(String),
    LspInitializationError(String),
    
    /// Shell errors
    ShellSpawnError(String),
    ShellInputError(String),
    ShellOutputError(String),
    ShellTerminationError(String),
    
    /// Terminal UI errors
    TerminalError(String),
    RenderError(String),
    
    /// Generic string error
    Message(String),
    
    /// Mutex lock error
    LockError(String),
    
    /// Other unknown errors
    Other(Box<dyn StdError + Send + Sync>),
}

/// Type alias for RVim's Result type
pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(err) => write!(f, "I/O error: {}", err),
            Error::FileNotFound(path) => write!(f, "File not found: {:?}", path),
            Error::PermissionDenied(path) => write!(f, "Permission denied: {:?}", path),
            Error::FileExists(path) => write!(f, "File already exists: {:?}", path),
            Error::DirectoryNotFound(path) => write!(f, "Directory not found: {:?}", path),
            Error::InvalidFileName(name) => write!(f, "Invalid file name: {}", name),
            
            Error::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
            Error::ConfigParseError { file, message } => 
                write!(f, "Failed to parse config file {:?}: {}", file, message),
            Error::MissingConfig(path) => write!(f, "Missing configuration: {:?}", path),
            
            Error::LuaError(msg) => write!(f, "Lua error: {}", msg),
            Error::LuaExecutionError(msg) => write!(f, "Lua execution error: {}", msg),
            Error::PluginError { name, message } => 
                write!(f, "Plugin '{}' error: {}", name, message),
            
            Error::LspError { code, message, language } => {
                if let Some(lang) = language {
                    write!(f, "LSP error [{}] ({}): {}", code, lang, message)
                } else {
                    write!(f, "LSP error [{}]: {}", code, message)
                }
            },
            Error::LspServerNotFound(server) => write!(f, "LSP server not found: {}", server),
            Error::LspConnectionError(msg) => write!(f, "LSP connection error: {}", msg),
            Error::LspInitializationError(msg) => write!(f, "LSP initialization error: {}", msg),
            
            Error::ShellSpawnError(msg) => write!(f, "Failed to spawn shell: {}", msg),
            Error::ShellInputError(msg) => write!(f, "Shell input error: {}", msg),
            Error::ShellOutputError(msg) => write!(f, "Shell output error: {}", msg),
            Error::ShellTerminationError(msg) => write!(f, "Shell termination error: {}", msg),
            
            Error::TerminalError(msg) => write!(f, "Terminal error: {}", msg),
            Error::RenderError(msg) => write!(f, "Render error: {}", msg),
            
            Error::Message(msg) => write!(f, "{}", msg),
            Error::LockError(msg) => write!(f, "Lock error: {}", msg),
            Error::Other(err) => write!(f, "Error: {}", err),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Io(err) => Some(err),
            _ => None,
        }
    }
}

// Fix missing From implementations for common error types
impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

impl<E> From<Box<E>> for Error 
where 
    E: StdError + Send + Sync + 'static 
{
    fn from(err: Box<E>) -> Self {
        Error::Other(err)
    }
}

impl From<Box<dyn StdError + Send + Sync>> for Error {
    fn from(err: Box<dyn StdError + Send + Sync>) -> Self {
        Error::Other(err)
    }
}

impl From<mlua::Error> for Error {
    fn from(err: mlua::Error) -> Self {
        Error::LuaError(err.to_string())
    }
}

// Fix the conversion for simplelog error types
impl From<log::SetLoggerError> for Error {
    fn from(err: log::SetLoggerError) -> Self {
        Error::Message(format!("Logger setup error: {}", err))
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Error::Message(message)
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Error::Message(message.to_string())
    }
}

impl<T> From<PoisonError<T>> for Error {
    fn from(err: PoisonError<T>) -> Self {
        Error::LockError(err.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Message(format!("JSON error: {}", err))
    }
}

// Add conversion from &Path for file not found errors
impl From<(PathBuf, io::Error)> for Error {
    fn from((path, err): (PathBuf, io::Error)) -> Self {
        match err.kind() {
            io::ErrorKind::NotFound => Error::FileNotFound(path),
            io::ErrorKind::PermissionDenied => Error::PermissionDenied(path),
            _ => Error::Io(err),
        }
    }
}

// Helper methods for creating specific errors
impl Error {
    pub fn file_not_found(path: impl Into<PathBuf>) -> Self {
        Error::FileNotFound(path.into())
    }
    
    pub fn config_error(message: impl Into<String>) -> Self {
        Error::ConfigError(message.into())
    }
    
    pub fn lsp_error(code: i32, message: impl Into<String>, language: Option<impl Into<String>>) -> Self {
        Error::LspError {
            code,
            message: message.into(),
            language: language.map(|l| l.into()),
        }
    }
    
    pub fn plugin_error(name: impl Into<String>, message: impl Into<String>) -> Self {
        Error::PluginError {
            name: name.into(),
            message: message.into(),
        }
    }
}
