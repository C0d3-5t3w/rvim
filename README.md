# RVim - Rust-based Vim Clone

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

RVim is a lightweight, modern Vim clone written in Rust with a Lua configuration system similar to Neovim. It aims to provide a familiar modal editing experience with additional modern features while maintaining high performance and cross-platform compatibility.

## Features

- **Vim-like Modal Editing**
  - Normal, Insert, Visual, and Command modes
  - Familiar keybindings for Vim users
  - Command-line with `:` prefixed commands
  
- **Multiple Windows & Buffers**
  - Split windows horizontally or vertically
  - Efficient buffer management
  - Multiple file editing
  
- **File Management**
  - Built-in file tree browser (toggle with `Space+e`)
  - Directory navigation and file operations
  
- **Integrated Terminal**
  - Open shell terminals within the editor
  - Horizontal shells with `Space+h`
  - Vertical shells with `Space+v`
  
- **Extensibility**
  - Lua configuration system
  - Plugin support through Lua
  - Neovim compatibility layer

- **Modern Development Features**
  - LSP (Language Server Protocol) integration
  - Syntax highlighting
  - Fast and memory-efficient
  - Cross-platform (Linux, macOS, Windows)

## Quick Start

### Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/rvim.git
cd rvim

# Build the project
make build

# Install globally (copies config to ~/.config/rvim/ and binary to /usr/local/bin/)
make install
```

### Basic Usage

```bash
# Open a file
rvim path/to/file.txt

# Create a new file
rvim

# Open multiple files
rvim file1.txt file2.txt
```

## Key Bindings

### Global

- `Space` - Leader key for special commands
- `:help` - Show help screen with keybindings

### Navigation Modes

- `Esc` - Return to Normal mode from any other mode
- `i` - Enter Insert mode
- `v` - Enter Visual mode
- `:` - Enter Command mode

### File Operations

- `:w` - Save current file
- `:q` - Quit
- `:wq` - Save and quit

### Window Management

- `Space+w` - Cycle through windows
- `Space+q` - Close current window
- `Space+x` - Close current buffer

### File Navigation

- `Space+e` - Toggle file tree browser
- In file tree: 
  - `j/k` - Navigate up/down
  - `l/Enter` - Open file or expand directory
  - `h` - Collapse directory or go to parent

### Terminal Integration

- `Space+h` - Open horizontal shell
- `Space+v` - Open vertical shell
- In shell mode:
  - `Esc` - Exit shell mode
  - `Enter` - Execute command
  - Up/Down arrows - Navigate command history

## Configuration

RVim uses Lua for configuration. The main configuration file is located at:

- Linux/macOS: `~/.config/rvim/config.lua`
- Windows: `%APPDATA%\rvim\config.lua`

Example configuration:

```lua
-- Set key mappings
rvim.map('n', '<C-s>', ':w<CR>')                 -- Ctrl+S to save in normal mode
rvim.map('n', '<space>v', 'open_vertical_shell') -- Space+v for vertical shell

-- User settings
local settings = {
  number = true,           -- Show line numbers
  tabstop = 4,             -- Tab width
  expandtab = true,        -- Use spaces instead of tabs
  file_tree = {
    width = 30,            -- Width of file tree panel
    show_hidden = false,   -- Show hidden files
  }
}
```

For detailed documentation, see the [DOC.md](doc/DOC.md) file.

## Development Status

RVim is currently in active development. While it's stable enough for daily use, you may encounter bugs or missing features. Contributions are welcome!

## Contributing

Contributions are welcome! Feel free to submit issues, feature requests, or pull requests.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License 📄

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details

## Credits 🤘🏼

- GitHub: [@C0d3-5t3w](https://github.com/C0d3-5t3w)
