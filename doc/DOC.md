# RVim Documentation

## Table of Contents

- [Introduction](#introduction)
- [Installation](#installation)
- [Core Concepts](#core-concepts)
- [Modes](#modes)
- [Key Bindings](#key-bindings)
- [Windows and Buffers](#windows-and-buffers)
- [File Browser](#file-browser)
- [Terminal Integration](#terminal-integration)
- [Configuration](#configuration)
- [Lua API](#lua-api)
- [Plugin System](#plugin-system)
- [LSP Integration](#lsp-integration)
- [Performance Considerations](#performance-considerations)
- [Troubleshooting](#troubleshooting)

## Introduction

RVim is a lightweight Vim clone written in Rust with a Lua configuration system similar to Neovim. It aims to provide a familiar modal editing experience with modern features while maintaining high performance.

### Philosophy

RVim was created with the following goals in mind:

1. **Performance**: Fast startup time and efficient memory usage
2. **Simplicity**: Clean, intuitive interface without unnecessary complexity
3. **Extensibility**: Powerful Lua configuration and plugin system
4. **Familiarity**: Maintain compatibility with Vim/Neovim where possible

## Installation

### Prerequisites

- Rust toolchain (1.70 or newer)
- Cargo package manager
- For optional features:
  - Language servers for LSP integration
  - Git for plugin management

### Building from Source

```bash
# Clone the repository
git clone https://github.com/yourusername/rvim.git
cd rvim

# Build the project
make build

# Install globally
make install
```

### Installation Locations

- **Binary**: `/usr/local/bin/rvim`
- **Configuration**: `~/.config/rvim/` (Linux/macOS) or `%APPDATA%\rvim\` (Windows)

## Core Concepts

### Files, Buffers, and Windows

- **File**: A file on disk
- **Buffer**: An in-memory representation of a file
- **Window**: A viewport displaying a buffer

Multiple windows can display the same buffer, and a buffer may not be associated with any file (like a new, unsaved buffer).

### Modes

RVim is a modal editor, meaning it has different modes of operation where the same keys perform different functions.

## Modes

### Normal Mode

Normal mode is the default mode when you start RVim. It's primarily used for navigation and entering commands.

### Insert Mode

Insert mode is used for inserting and editing text. Enter insert mode by pressing `i` in normal mode.

### Visual Mode

Visual mode is used for selecting text. Enter visual mode by pressing `v` in normal mode.

### Command Mode

Command mode is used for entering commands. Enter command mode by pressing `:` in normal mode.

### File Tree Mode

File tree mode allows navigation of the file system. Toggle with `Space+e`.

### Shell Mode

Shell mode provides an interactive terminal within RVim. Activate with `Space+h` or `Space+v`.

### Help Mode

Help mode displays a help screen with key bindings. Access with `:help` command.

## Key Bindings

### Global Commands

| Key           | Action                      |
|---------------|----------------------------|
| `Space`       | Leader key                 |
| `:help`       | Show help screen           |
| `:q`          | Quit                       |
| `:w`          | Save current file          |
| `:wq`         | Save and quit              |

### Normal Mode

| Key           | Action                      |
|---------------|----------------------------|
| `i`           | Enter Insert mode          |
| `v`           | Enter Visual mode          |
| `:`           | Enter Command mode         |
| `h/j/k/l`     | Move cursor left/down/up/right |
| `w`           | Move to next word start    |
| `e`           | Move to next word end      |
| `b`           | Move to previous word start|
| `q`           | Quit (in some contexts)    |

### Leader Key Commands

| Key           | Action                      |
|---------------|----------------------------|
| `Space+e`     | Toggle File Tree           |
| `Space+v`     | Open Vertical Shell        |
| `Space+h`     | Open Horizontal Shell      |
| `Space+w`     | Cycle Windows              |
| `Space+q`     | Close Current Window       |
| `Space+x`     | Close Current Buffer       |

### Insert Mode

| Key           | Action                      |
|---------------|----------------------------|
| `Esc`         | Exit to Normal Mode        |
| `Backspace`   | Delete char before cursor  |
| `Enter`       | New line                   |

### File Tree Mode

| Key           | Action                      |
|---------------|----------------------------|
| `Esc` / `q`   | Close File Tree            |
| `j` / `k`     | Navigate up/down           |
| `l` / `Enter` | Open file / Expand directory |
| `h`           | Collapse directory / Go to parent |

### Shell Mode

| Key           | Action                      |
|---------------|----------------------------|
| `Esc`         | Return to previous mode    |
| `Enter`       | Send command to shell      |
| `Up/Down`     | Navigate command history   |

## Windows and Buffers

### Window Management

Windows are viewports that display buffers. RVim allows multiple windows to be open simultaneously.

#### Window Commands

- `Space+w` - Cycle through windows
- `Space+q` - Close current window

When multiple windows are open, borders will indicate the window boundaries, with the active window highlighted.

### Buffer Management

Buffers are in-memory representations of files. Multiple buffers can be open at once.

#### Buffer Commands

- `Space+x` - Close current buffer

## File Browser

The file browser allows navigation of the file system directly within RVim.

### Commands

- `Space+e` - Toggle file browser
- `j/k` - Navigate up/down
- `l/Enter` - Open file or expand directory
- `h` - Collapse directory or go to parent

### Features

- Directory tree view
- File and directory icons
- Hidden file filtering (configurable)

## Terminal Integration

RVim includes an integrated terminal that allows running shell commands without leaving the editor.

### Opening Terminals

- `Space+h` - Open horizontal shell
- `Space+v` - Open vertical shell

### Terminal Interaction

- `Esc` - Exit shell mode (shell continues running)
- `Enter` - Execute command
- `Up/Down` - Navigate command history
- Type `exit` to close the shell process

## Configuration

RVim uses Lua for configuration, allowing powerful and flexible customization.

### Configuration File Location

- Linux/macOS: `~/.config/rvim/config.lua`
- Windows: `%APPDATA%\rvim\config.lua`

### Configuration Structure

```lua
-- Example configuration

-- Define global settings
local settings = {
  number = true,            -- Show line numbers
  relativenumber = false,   -- Show relative line numbers
  tabstop = 4,              -- Tab width
  shiftwidth = 4,           -- Indentation width
  expandtab = true,         -- Use spaces instead of tabs
  syntax = true,            -- Enable syntax highlighting
  theme = "default",        -- Color theme
  file_tree = {
    width = 30,             -- Width of file tree panel
    show_hidden = false,    -- Show hidden files
  }
}

-- Key mappings
rvim.map('n', '<C-s>', ':w<CR>')
rvim.map('n', '<space>e', 'toggle_file_tree')
rvim.map('n', '<space>v', 'open_vertical_shell')
rvim.map('n', '<space>h', 'open_horizontal_shell')
rvim.map('n', '<space>w', 'cycle_window')
rvim.map('n', '<space>q', 'close_window')
rvim.map('n', '<space>x', 'close_buffer')

-- Define custom functions
local function hello_world()
  print("Hello from Lua!")
end

-- Register a custom command
rvim.command.Hello = hello_world
```

## Lua API

RVim provides a Lua API for configuration and extension.

### Global Tables

- `rvim` - Main RVim namespace
  - `rvim.api` - Core API functions
  - `rvim.command` - Command registration
  - `rvim.map` - Key mapping functions

### Mapping Functions

```lua
rvim.map(mode, key, action, opts)
```

Parameters:
- `mode`: String - 'n' (normal), 'i' (insert), 'v' (visual), 'c' (command)
- `key`: String - Key combination (e.g., '<C-s>')
- `action`: String/Function - Command or function to execute
- `opts`: Table (optional) - Options

Example:
```lua
rvim.map('n', '<C-s>', ':w<CR>')  -- Ctrl+S to save in normal mode
rvim.map('n', '<leader>h', function() print("Hello!") end)
```

### Neovim Compatibility Layer

RVim includes a compatibility layer for Neovim plugins and configurations:

```lua
-- Access through the vim global table
vim.g.some_variable = "value"
vim.opt.rtp:append(some_path)

-- API functions are available too
vim.api.nvim_set_option("tabstop", 4)
```

## Plugin System

RVim supports plugins through its Lua configuration system.

### Plugin Structure

Plugins are Lua modules that can be loaded through the configuration:

```lua
-- Example plugin
local plugin = {}

function plugin.setup()
  -- Plugin initialization code
  rvim.map('n', '<leader>p', function()
    print("Plugin function called!")
  end)
end

return plugin
```

### Loading Plugins

Plugins can be loaded in the configuration file:

```lua
-- Load a plugin
local my_plugin = require("my_plugin")
my_plugin.setup()
```

RVim also includes a compatibility layer for using many Neovim plugins:

```lua
-- Using lazy.nvim package manager (example)
local plugins = {
  {
    "neovim/nvim-lspconfig",
    config = function()
      -- LSP setup
    end
  }
}

require("lazy").setup(plugins)
```

## LSP Integration

RVim includes support for the Language Server Protocol (LSP), enabling rich code intelligence features.

### Supported Language Servers

- Rust (rust-analyzer)
- Python (pyright)
- TypeScript/JavaScript (typescript-language-server)
- Lua (lua-language-server)
- Go (gopls)
- And more...

### LSP Configuration

LSP servers are automatically detected and started when opening files of supported languages. Server-specific settings can be configured in your `config.lua`:

```lua
-- Example LSP configuration
local lsp_settings = {
  rust_analyzer = {
    checkOnSave = {
      command = "clippy"
    }
  }
}

-- Use with Neovim-compatible plugins
require("lspconfig").rust_analyzer.setup(lsp_settings.rust_analyzer)
```

## Performance Considerations

RVim is designed to be fast and efficient, but there are ways to optimize performance further:

- **Minimize plugins**: Only load plugins you actively use
- **Use efficient key mappings**: Complex key mappings can slow down response time
- **Optimize configuration**: Large or complex Lua configurations can impact startup time
- **Memory usage**: Monitor memory usage with many buffers open

## Troubleshooting

### Common Issues

#### RVim won't start

- Ensure Rust is installed and up-to-date
- Check for errors in the configuration file
- Examine the log file (`rvim.log`)

#### Configuration not loading

- Verify the configuration file path
- Check for Lua syntax errors
- Look for error messages in the log

#### LSP features not working

- Ensure the language server is installed
- Check if the language server is automatically detected
- Look for LSP-related errors in the log

#### Performance problems

- Disable unused plugins
- Simplify configuration
- Update to the latest version
- Check system resources (memory, CPU)

### Getting Help

- Create an issue on GitHub
- Check the log file for errors (`rvim.log`)
- Review this documentation for guidance

## Acknowledgments

RVim draws inspiration from:

- Vim and Neovim for the modal editing paradigm
- Rust community for excellent libraries and tools
- Lua community for the powerful, embeddable language

## Advanced Features

### Smart Text Editing
- Efficient rope-based text storage with `ropey`
- Multi-cursor editing support
- Advanced undo/redo tree
- Auto-indent and smart indent detection

### Enhanced Syntax Highlighting
- Tree-sitter based syntax highlighting
- Support for multiple languages
- Real-time parsing and highlighting
- Custom theme support via `syntect`

### Fuzzy Finding
- Fast file fuzzy finding with `fuzzy-matcher`
- Command palette with fuzzy search
- Symbol search in current file
- Project-wide symbol search

### File System Integration
- Real-time file system watching with `notify`
- Auto-reload on external changes
- Git integration for status and diff
- Project-wide search and replace

### Performance Optimizations
- Parallel processing with `rayon`
- Async I/O operations with `tokio`
- Thread-safe data structures with `dashmap`
- Efficient mutexes with `parking_lot`
