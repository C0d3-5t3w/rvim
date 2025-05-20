# RVim - Rust-based Vim Clone

RVim is a lightweight Vim clone written in Rust with a Lua configuration system similar to Neovim.

## Features

- Vim-like modal editing (Normal, Insert, Visual, Command modes)
- File tree browser (toggle with Space+e)
- Integrated shell terminals (horizontal with Space+h, vertical with Space+v)
- Buffer management (close current buffer with Space+x)
- Window management
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

# Install globally (copies config to ~/.config/rvim/ and binary to /usr/local/bin/)
make install
```

## Configuration

RVim uses Lua for configuration. When you run `make install`, the configuration files are copied to:

- Linux/macOS: `~/.config/rvim/`
- Windows: `%APPDATA%\rvim\`

The main configuration file is `config.lua`. You can edit this file to customize RVim to your liking.

If you run RVim without installing, it will look for configuration files in the following order:
1. User config directory (`~/.config/rvim/` on Linux/macOS)
2. Source code's `config/` directory
3. If neither exists, it will create a default configuration

## Usage

```bash
# Open a file
rvim path/to/file.txt

# Create a new file
rvim
```

### File Tree Browser

- Press `Space` followed by `e` to toggle the file tree browser
- Navigate with `j` (down) and `k` (up)
- Press `Enter` or `l` to open a file or expand a directory
- Press `h` to collapse a directory
- Press `Esc` to close the file tree

### Shell Commands

- Press `Space` followed by `h` to open a horizontal shell terminal
- Press `Space` followed by `v` to open a vertical shell terminal
- Type commands and press `Enter` to execute them
- Use arrow keys to navigate command history
- Press `Esc` to exit shell mode

### Buffer Management

- Press `Space` followed by `x` to close the current buffer
- Use `:w` to save the current buffer
- Use `:q` to quit
- Use `:wq` to save and quit

### Window Management

- Press `Space` followed by `w` to cycle through windows
- Press `Space` followed by `q` to close the current window

## Configuration

RVim uses Lua for configuration. The default configuration file is located at:

- Linux/macOS: `~/.config/rvim/config.lua`
- Windows: `%APPDATA%\rvim\config.lua`

Example configuration:

```lua
-- Set key mappings
rvim.map('n', '<C-s>', ':w<CR>')                 -- Ctrl+S to save in normal mode
rvim.map('n', '<space>v', 'open_vertical_shell') -- Space+v for vertical shell
rvim.map('n', '<space>h', 'open_horizontal_shell') -- Space+h for horizontal shell
rvim.map('n', '<space>x', 'close_buffer')        -- Space+x to close current buffer

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

-- Define custom functions
local function hello_world()
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
