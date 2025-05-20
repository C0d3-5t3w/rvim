# RVim - Rust-based Vim Clone

RVim is a lightweight Vim clone written in Rust with a Lua configuration system similar to Neovim.

## Features

- Vim-like modal editing (Normal, Insert, Visual, Command modes)
- Lua configuration system
- Plugin support through Lua
- Fast and memory-efficient
- Cross-platform

## Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/rvim.git
cd rvim

# Build the project
make build

# Install globally
make install
```

## Usage

```bash
# Open a file
rvim path/to/file.txt

# Create a new file
rvim
```

## Configuration

RVim uses Lua for configuration. The default configuration file is located at:

- Linux/macOS: `~/.config/rvim/config.lua`
- Windows: `%APPDATA%\rvim\config.lua`

Example configuration:

```lua
-- Set key mappings
rvim.map('n', '<C-s>', ':w<CR>')  -- Ctrl+S to save in normal mode

-- User settings
local settings = {
  number = true,           -- Show line numbers
  tabstop = 4,             -- Tab width
  expandtab = true,        -- Use spaces instead of tabs
}

-- Define custom functions
function hello_world()
  print("Hello from Lua!")
end
```

## Creating Plugins

Plugins can be created as Lua modules:

```lua
local plugin = {}

function plugin.setup()
  -- Plugin initialization
  rvim.map('n', '<leader>p', function()
    print("Plugin function called!")
  end)
end

return plugin
```

## License 📄

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details

## Credits 🤘🏼

- GitHub: [@C0d3-5t3w](https://github.com/C0d3-5t3w)
