use std::error::Error as StdError;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent},
    execute,
    terminal::{self, ClearType},
    style::{Color, SetForegroundColor, SetBackgroundColor, ResetColor},
};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use mlua::Lua;
use log::info;

use std::env;

use crate::cli::filetree::FileTree;
use crate::cli::window::{Window, SplitType};
use crate::cli::shell::Shell;
use crate::error::{Error, Result};

// Editor modes
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Normal,
    Insert,
    Visual,
    Command,
    FileTree,
    Shell,
    Help, // Added Help mode
}

// Buffer represents a single open file or shell
struct Buffer {
    document: Document,
    cursor_x: usize,
    cursor_y: usize,
    offset_x: usize,
    offset_y: usize,
    is_shell: bool,
    shell: Option<Shell>,
    filename: Option<String>,
}

impl Buffer {
    fn new() -> Self {
        Self {
            document: Document::new(),
            cursor_x: 0,
            cursor_y: 0,
            offset_x: 0,
            offset_y: 0,
            is_shell: false,
            shell: None,
            filename: None,
        }
    }

    fn from_file(filename: &str) -> Result<Self> {
        let document = Document::from_file(filename)?;
        Ok(Self {
            document,
            cursor_x: 0,
            cursor_y: 0,
            offset_x: 0,
            offset_y: 0,
            is_shell: false,
            shell: None,
            filename: Some(filename.to_string()),
        })
    }

    fn from_shell(is_horizontal: bool) -> Self {
        Self {
            document: Document::new(), // Empty document for shells
            cursor_x: 0,
            cursor_y: 0,
            offset_x: 0,
            offset_y: 0,
            is_shell: true,
            shell: Some(Shell::new(is_horizontal)),
            filename: None,
        }
    }

    fn save(&mut self) -> Result<()> {
        if self.is_shell {
            return Err(crate::error::Error::Message("Cannot save shell buffer".into()));
        }
        self.document.save()
    }
}

// Document representation
struct Document {
    lines: Vec<String>,
    filename: Option<String>,
    modified: bool,
}

impl Document {
    fn new() -> Self {
        Self {
            lines: vec![String::new()],
            filename: None,
            modified: false,
        }
    }

    fn from_file(filename: &str) -> Result<Self> {
        let content = fs::read_to_string(filename)
            .map_err(|e| crate::error::Error::Io(e))?;
        let lines: Vec<String> = content.lines().map(String::from).collect();
        let lines = if lines.is_empty() { vec![String::new()] } else { lines };
        
        Ok(Self {
            lines,
            filename: Some(filename.to_string()),
            modified: false,
        })
    }

    fn save(&mut self) -> Result<()> {
        if let Some(filename) = &self.filename {
            let content = self.lines.join("\n");
            fs::write(filename, content)
                .map_err(|e| crate::error::Error::Io(e))?;
            self.modified = false;
            Ok(())
        } else {
            Err(crate::error::Error::Message("No filename specified".into()))
        }
    }

    fn insert_char(&mut self, row: usize, col: usize, c: char) {
        if row >= self.lines.len() {
            return;
        }
        
        let line = &mut self.lines[row];
        if col > line.len() {
            line.push(c);
        } else {
            line.insert(col, c);
        }
        self.modified = true;
    }

    fn delete_char(&mut self, row: usize, col: usize) -> bool {
        if row >= self.lines.len() {
            return false;
        }
        
        let line = &mut self.lines[row];
        if col < line.len() {
            line.remove(col);
            self.modified = true;
            true
        } else {
            false
        }
    }
}

// Editor state
pub struct Editor {
    buffers: Vec<Buffer>,
    active_buffer: usize,
    cursor_x: usize,
    cursor_y: usize,
    offset_x: usize,
    offset_y: usize,
    terminal_height: usize,
    terminal_width: usize,
    mode: Mode,
    command_line: String,
    config_path: PathBuf,
    lua: Lua,
    quit: bool,
    waiting_for_second_key: bool,
    file_tree: Option<FileTree>,
    previous_mode: Mode,
    windows: Vec<Window>,
    active_window: usize,
}

impl Editor {
    pub fn new(config_path: PathBuf) -> Result<Self> {
        // Initialize terminal
        terminal::enable_raw_mode()?;
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            cursor::Show,
            event::EnableMouseCapture  // Enable mouse events
        )?;
        
        let (cols, rows) = terminal::size()?;
        
        // Initialize Lua
        let lua = Lua::new();
        
        // Create initial window
        let initial_window = Window::new(0, 0, cols as usize, rows as usize - 2);
        
        // Create initial buffer
        let initial_buffer = Buffer::new();
        
        let mut editor = Self {
            buffers: vec![initial_buffer],
            active_buffer: 0,
            cursor_x: 0,
            cursor_y: 0,
            offset_x: 0,
            offset_y: 0,
            terminal_height: rows as usize,
            terminal_width: cols as usize,
            mode: Mode::Normal,
            command_line: String::new(),
            config_path,
            lua,
            quit: false,
            waiting_for_second_key: false,
            file_tree: None,
            previous_mode: Mode::Normal,
            windows: vec![initial_window],
            active_window: 0,
        };
        
        // Load Lua configuration
        editor.load_config()?;
        
        // Initialize file tree with current directory
        let current_dir = env::current_dir()?;
        editor.file_tree = Some(FileTree::new(&current_dir)?);
        
        Ok(editor)
    }
    
    pub fn open_file(&mut self, filename: &str) -> Result<()> {
        let buffer = Buffer::from_file(filename)?;
        
        // Replace the current buffer with the new one
        if self.buffers.is_empty() {
            self.buffers.push(buffer);
            self.active_buffer = 0;
        } else {
            self.buffers[self.active_buffer] = buffer;
        }
        
        // Update file tree path to new file's directory
        let path = PathBuf::from(filename);
        if let Some(parent) = path.parent() {
            self.file_tree = Some(FileTree::new(parent)?);
        }
        
        // Set the file path for the active window
        if !self.windows.is_empty() {
            self.windows[self.active_window].file_path = Some(path);
        }
        
        Ok(())
    }
    
    fn open_shell(&mut self, is_horizontal: bool) -> Result<()> {
        let shell_buffer = Buffer::from_shell(is_horizontal);
        
        // Add the new shell buffer
        self.buffers.push(shell_buffer);
        
        // Make the new shell the active buffer
        self.active_buffer = self.buffers.len() - 1;
        
        // Switch to shell mode
        self.previous_mode = self.mode;
        self.mode = Mode::Shell;
        
        info!("Opened {} shell", if is_horizontal { "horizontal" } else { "vertical" });
        
        Ok(())
    }
    
    fn close_current_buffer(&mut self) -> Result<()> {
        if self.buffers.len() <= 1 {
            info!("Cannot close the last buffer");
            return Ok(());
        }
        
        // Remove the current buffer
        self.buffers.remove(self.active_buffer);
        
        // Adjust the active buffer index if needed
        if self.active_buffer >= self.buffers.len() {
            self.active_buffer = self.buffers.len() - 1;
        }
        
        info!("Closed buffer, now at buffer {}", self.active_buffer + 1);
        
        Ok(())
    }
    
    fn load_config(&mut self) -> Result<()> {
        let config_file = self.config_path.join("config.lua");
        
        // Register API functions
        self.register_api()?;
        
        // Load config file if exists
        if config_file.exists() {
            info!("Loading config from: {:?}", config_file);
            let config_content = fs::read_to_string(config_file)?;
            self.lua.load(&config_content).exec()?;
        } else {
            info!("No config file found at: {:?}", config_file);
        }
        
        Ok(())
    }
    
    fn register_api(&mut self) -> Result<()> {
        // Create a global 'rvim' table
        let rvim_table = self.lua.create_table()?;
        
        // Add the map function (similar to Neovim's vim.keymap.set)
        let map_fn = self.lua.create_function(|_, (mode, key, action): (String, String, String)| {
            // This would actually set keybindings
            info!("Mapping in mode '{}': {} -> {}", mode, key, action);
            Ok(())
        })?;
        
        rvim_table.set("map", map_fn)?;
        
        // Create an API module
        let api_table = self.lua.create_table()?;
        
        // Example API function
        let get_version_fn = self.lua.create_function(|_, ()| {
            Ok("rvim 0.1.0")
        })?;
        
        api_table.set("get_version", get_version_fn)?;
        rvim_table.set("api", api_table)?;
        
        // Set the global rvim table
        self.lua.globals().set("rvim", rvim_table)?;
        
        Ok(())
    }
    
    pub fn set_plugin_manager(&mut self, plugin_manager: crate::cli::plugin::PluginManager) -> Result<()> {
        // Register the plugin manager's Lua functions
        let plugin_table = self.lua.create_table()?;
        
        // Add function to get installed plugins
        let get_plugins_fn = self.lua.create_function(|_, ()| {
            Ok("List of installed plugins")
        })?;
        plugin_table.set("get_plugins", get_plugins_fn)?;
        
        // Add function to install a plugin
        let install_plugin_fn = self.lua.create_function(move |_, plugin_url: String| {
            info!("Installing plugin: {}", plugin_url);
            // In a real implementation, this would call plugin_manager.install_plugin(...)
            Ok(())
        })?;
        plugin_table.set("install", install_plugin_fn)?;
        
        // Set the plugins table in the global rvim table
        let globals = self.lua.globals();
        let rvim_table: mlua::Table = globals.get("rvim")?;
        rvim_table.set("plugins", plugin_table)?;
        
        info!("Plugin manager initialized");
        Ok(())
    }
    
    pub fn run(&mut self) -> Result<()> {
        self.refresh_screen()?;
        
        while !self.quit {
            self.process_keypress()?;
            self.refresh_screen()?;
        }
        
        // Cleanup terminal on exit
        execute!(
            io::stdout(),
            LeaveAlternateScreen,
            event::DisableMouseCapture,  // Disable mouse capture when exiting
            cursor::Show
        )?;
        terminal::disable_raw_mode()?;
        
        Ok(())
    }
    
    fn refresh_screen(&mut self) -> Result<()> {
        // Poll shell output if in shell mode and buffer exists
        if self.mode == Mode::Shell {
            if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                if let Some(shell) = buffer.shell.as_mut() {
                    shell.poll_output();
                    if !shell.running { // If shell terminated, switch mode
                        self.mode = self.previous_mode;
                        // Consider closing the shell buffer or marking it as non-interactive
                        // For now, just switch mode. The buffer remains.
                        info!("Shell terminated, switching to mode: {:?}", self.mode);
                    }
                }
            }
        }

        execute!(
            io::stdout(),
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )?;

        if self.mode == Mode::Help {
            self.draw_help_screen()?;
        } else {
            // Draw the file tree if visible
            let filetree_offset = if let Some(tree) = &self.file_tree {
                if tree.visible {
                    self.draw_file_tree()?;
                    tree.width + 1
                } else {
                    0
                }
            } else {
                0
            };
            
            // Draw each window
            for (idx, window) in self.windows.iter().enumerate() {
                // Adjust for file tree
                let adjusted_x = window.x + filetree_offset;
                
                // Draw window borders if there are multiple windows
                if self.windows.len() > 1 {
                    self.draw_window_borders(window, adjusted_x, idx == self.active_window)?;
                }
                
                // Draw window content
                self.draw_window_content(window, adjusted_x)?;
            }
        }
        
        self.draw_status_line()?;
        self.draw_message_line()?;
        
        // Position cursor based on mode
        match self.mode {
            Mode::Help => {
                // Hide cursor or move to a non-obtrusive place for help screen
                execute!(io::stdout(), cursor::Hide)?;
            }
            Mode::FileTree => {
                execute!(io::stdout(), cursor::Show)?;
                if let Some(tree) = &self.file_tree {
                    let tree_cursor_y = tree.cursor.min(self.terminal_height - 3);
                    execute!(io::stdout(), cursor::MoveTo(2, tree_cursor_y as u16))?;
                }
            },
            Mode::Shell => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) { 
                    if let Some(shell) = buffer.shell.as_mut() { 
                        shell.poll_output(); 
                        if !shell.running && self.mode == Mode::Shell { 
                             self.mode = self.previous_mode;
                        } else if self.mode == Mode::Shell { 
                            let window = &self.windows[self.active_window];
                            let effective_height = if self.windows.len() > 1 { window.height - 2 } else { window.height };

                            let content_y_start = if self.windows.len() > 1 { 
                                window.y + 1 
                            } else { 
                                0 
                            };
                            
                            let filetree_width = if let Some(tree) = &self.file_tree { 
                                if tree.visible { tree.width + 1 } else { 0 } 
                            } else { 0 };
                            
                            let content_x_start = if self.windows.len() > 1 { 
                                window.x + filetree_width + 1 
                            } else { 
                                filetree_width 
                            };
                            
                            // Calculate the Y position for RVim's input line.
                            // This is the number of output lines from the shell that will actually be displayed.
                            let displayed_output_lines_count = shell.lines.len().min(effective_height.saturating_sub(1));
                            let rvim_input_line_screen_y = content_y_start + displayed_output_lines_count;
                            
                            // Cursor position for RVim's input_line
                            let rvim_input_cursor_screen_x = content_x_start + shell.cursor_pos + 2; // +2 for "$ " visual prefix
                            
                            execute!(io::stdout(), cursor::MoveTo(
                                rvim_input_cursor_screen_x as u16, 
                                rvim_input_line_screen_y as u16
                            ))?;
                        }
                    }
                }
            },
            _ => {
                if !self.buffers.is_empty() && self.active_buffer < self.buffers.len() {
                    let buffer = &self.buffers[self.active_buffer];
                    let window = &self.windows[self.active_window];
                    
                    let filetree_width = if let Some(tree) = &self.file_tree { 
                        if tree.visible { tree.width + 1 } else { 0 } 
                    } else { 0 };
                    
                    let adjusted_x = window.x + filetree_width;
                    let content_x = if self.windows.len() > 1 { adjusted_x + 1 } else { adjusted_x };
                    let content_y = if self.windows.len() > 1 { window.y + 1 } else { window.y };
                    
                    let screen_x = content_x + buffer.cursor_x.saturating_sub(buffer.offset_x);
                    let screen_y = content_y + buffer.cursor_y.saturating_sub(buffer.offset_y);
                    execute!(io::stdout(), cursor::MoveTo(screen_x as u16, screen_y as u16))?;
                }
            }
        }
        
        io::stdout().flush()?;
        
        Ok(())
    }
    
    fn draw_file_tree(&self) -> Result<()> {
        if let Some(tree) = &self.file_tree {
            let tree_width = tree.width;
            let display_height = self.terminal_height.saturating_sub(2);
            
            // Draw tree border
            for y in 0..display_height {
                execute!(
                    io::stdout(),
                    cursor::MoveTo(tree_width as u16, y as u16),
                    SetForegroundColor(Color::DarkGrey)
                )?;
                print!("│");
            }
            execute!(io::stdout(), ResetColor)?;
            
            // Draw file tree entries
            for (idx, entry) in tree.entries.iter().enumerate() {
                if idx >= display_height {
                    break;
                }
                
                let prefix = if entry.is_dir {
                    if entry.is_expanded { "▼ " } else { "▶ " }
                } else {
                    "  "
                };
                
                let indent = "  ".repeat(entry.level);
                let name = if entry.is_dir {
                    format!("{}/ ", entry.name)
                } else {
                    entry.name.clone()
                };
                
                // Format the line with proper indentation
                let line = format!("{}{}{}", indent, prefix, name);
                
                // Truncate if too long
                let display_line = if line.len() > tree_width - 1 {
                    format!("{}…", &line[0..tree_width - 2])
                } else {
                    line
                };
                
                execute!(
                    io::stdout(),
                    cursor::MoveTo(0, idx as u16)
                )?;
                
                // Highlight current selection
                if idx == tree.cursor {
                    execute!(
                        io::stdout(),
                        SetBackgroundColor(Color::DarkBlue),
                        SetForegroundColor(Color::White)
                    )?;
                } else if entry.is_dir {
                    execute!(
                        io::stdout(),
                        SetForegroundColor(Color::Blue)
                    )?;
                }
                
                print!("{:width$}", display_line, width = tree_width);
                execute!(io::stdout(), ResetColor)?;
            }
        }
        
        Ok(())
    }
    
    fn draw_window_borders(&self, window: &Window, adjusted_x: usize, is_active: bool) -> Result<()> {
        let border_color = if is_active { Color::Green } else { Color::Grey };
        
        // Draw horizontal borders
        for x in 0..window.width {
            // Top border
            execute!(
                io::stdout(),
                cursor::MoveTo((adjusted_x + x) as u16, window.y as u16),
                SetForegroundColor(border_color)
            )?;
            print!("─");
            
            // Bottom border
            execute!(
                io::stdout(),
                cursor::MoveTo((adjusted_x + x) as u16, (window.y + window.height - 1) as u16)
            )?;
            print!("─");
        }
        
        // Draw vertical borders
        for y in 0..window.height {
            // Left border
            execute!(
                io::stdout(),
                cursor::MoveTo(adjusted_x as u16, (window.y + y) as u16),
                SetForegroundColor(border_color)
            )?;
            print!("│");
            
            // Right border
            execute!(
                io::stdout(),
                cursor::MoveTo((adjusted_x + window.width - 1) as u16, (window.y + y) as u16)
            )?;
            print!("│");
        }
        
        // Draw corners
        execute!(io::stdout(), cursor::MoveTo(adjusted_x as u16, window.y as u16))?;
        print!("┌");
        execute!(io::stdout(), cursor::MoveTo((adjusted_x + window.width - 1) as u16, window.y as u16))?;
        print!("┐");
        execute!(io::stdout(), cursor::MoveTo(adjusted_x as u16, (window.y + window.height - 1) as u16))?;
        print!("└");
        execute!(io::stdout(), cursor::MoveTo((adjusted_x + window.width - 1) as u16, (window.y + window.height - 1) as u16))?;
        print!("┘");
        
        execute!(io::stdout(), ResetColor)?;
        
        Ok(())
    }
    
    fn draw_window_content(&self, window: &Window, adjusted_x: usize) -> Result<()> {
        let effective_width = if self.windows.len() > 1 { window.width - 2 } else { window.width };
        let effective_height = if self.windows.len() > 1 { window.height - 2 } else { window.height };
        
        // Adjust starting position if window has borders
        let content_x = if self.windows.len() > 1 { adjusted_x + 1 } else { adjusted_x };
        let content_y = if self.windows.len() > 1 { window.y + 1 } else { window.y };
        
        // Get the active buffer
        if self.buffers.is_empty() || self.active_buffer >= self.buffers.len() {
            return Ok(());
        }
        
        let buffer = &self.buffers[self.active_buffer];
        
        if buffer.is_shell {
            // Draw shell content
            if let Some(shell) = &buffer.shell { // No mut needed for drawing
                let mut line_counter = 0;
                // Display previous lines from the shell's actual output
                // Consider viewport scrolling for shell.lines here
                let start_line_idx = shell.lines.len().saturating_sub(effective_height.saturating_sub(1)); // Show last lines, leave one for input

                for (idx, line_content) in shell.lines.iter().skip(start_line_idx).enumerate() {
                    if line_counter >= effective_height -1 { // Reserve last line for input
                        break;
                    }
                    execute!(io::stdout(), cursor::MoveTo(content_x as u16, (content_y + line_counter) as u16))?;
                    let display_line = if line_content.len() > effective_width {
                        &line_content[0..effective_width]
                    } else {
                        line_content
                    };
                    print!("{}", display_line);
                    line_counter += 1;
                }
                
                // Draw RVim's current input line with a visual prompt
                if line_counter < effective_height {
                    execute!(io::stdout(), cursor::MoveTo(content_x as u16, (content_y + line_counter) as u16))?;
                    let prompt_and_input = format!("$ {}", shell.input_line); // RVim's visual prompt
                    let display_prompt_input = if prompt_and_input.len() > effective_width {
                        &prompt_and_input[0..effective_width]
                    } else {
                        &prompt_and_input
                    };
                    print!("{}", display_prompt_input);
                }
            }
        } else {
            // Draw document content
            for y in 0..effective_height {
                let file_row = y + buffer.offset_y;
                
                execute!(io::stdout(), cursor::MoveTo(content_x as u16, (content_y + y) as u16))?;
                
                if file_row >= buffer.document.lines.len() {
                    if y == window.height / 3 && buffer.document.lines.len() == 1 && buffer.document.lines[0].is_empty() {
                        let welcome = format!("RVim - Version 0.1.0");
                        let padding = (effective_width - welcome.len()) / 2;
                        print!("~{}{}", " ".repeat(padding.saturating_sub(1)), welcome);
                    } else {
                        print!("~");
                    }
                } else {
                    let line = &buffer.document.lines[file_row];
                    let start = buffer.offset_x.min(line.len());
                    let end = (buffer.offset_x + effective_width).min(line.len());
                    if start < end {
                        print!("{}", &line[start..end]);
                    }
                }
            }
        }
        
        Ok(())
    }
    
    fn draw_status_line(&self) -> Result<()> {
        let status = match self.mode {
            Mode::Normal => " NORMAL ",
            Mode::Insert => " INSERT ",
            Mode::Visual => " VISUAL ",
            Mode::Command => " COMMAND ",
            Mode::FileTree => " FILE TREE ",
            Mode::Shell => " SHELL ",
            Mode::Help => " HELP ", // Added Help status
        };
        
        // Get buffer info
        let buffer_info = if !self.buffers.is_empty() && self.active_buffer < self.buffers.len() {
            let buffer = &self.buffers[self.active_buffer];
            if buffer.is_shell {
                "[Shell]".to_string()
            } else {
                let name = buffer.filename.clone().unwrap_or_else(|| "[No Name]".into());
                let modified = if buffer.document.modified { "[+]" } else { "" };
                format!("{}{}", name, modified)
            }
        } else {
            "[No Buffer]".to_string()
        };
        
        let cursor_info = if !self.buffers.is_empty() && self.active_buffer < self.buffers.len() {
            let buffer = &self.buffers[self.active_buffer];
            if buffer.is_shell {
                // For shell, show nothing special for now
                "".to_string()
            } else {
                format!(" - {}/{}", buffer.cursor_y + 1, buffer.document.lines.len())
            }
        } else {
            "".to_string()
        };
        
        let status_line = format!("{}{}{}", status, buffer_info, cursor_info);
        
        execute!(
            io::stdout(),
            cursor::MoveTo(0, self.terminal_height as u16 - 2),
            SetForegroundColor(Color::Black),
            SetBackgroundColor(Color::White)
        )?;
        
        let width = self.terminal_width;
        let padding = width.saturating_sub(status_line.len());
        print!("{}{}", status_line, " ".repeat(padding));
        
        execute!(
            io::stdout(),
            ResetColor
        )?;
        
        Ok(())
    }
    
    fn draw_message_line(&self) -> Result<()> {
        execute!(
            io::stdout(),
            cursor::MoveTo(0, self.terminal_height as u16 - 1),
            terminal::Clear(ClearType::CurrentLine)
        )?;
        
        if let Mode::Command = self.mode {
            print!(":{}", self.command_line);
        } else if self.mode == Mode::Help {
            let help_msg = "Press any key to close help.";
            let padding = self.terminal_width.saturating_sub(help_msg.len()) / 2;
            print!("{}{}", " ".repeat(padding), help_msg);
        }
        
        Ok(())
    }
    
    fn process_insert_mode(&mut self, key: KeyEvent) -> Result<()> {
        if self.buffers.is_empty() || self.active_buffer >= self.buffers.len() {
            return Ok(());
        }
        
        let buffer = &mut self.buffers[self.active_buffer];
        
        if buffer.is_shell {
            // If the buffer is a shell, switch to shell mode
            self.mode = Mode::Shell;
            return self.process_shell_mode(key);
        }
        
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Char(c) => {
                buffer.document.insert_char(buffer.cursor_y, buffer.cursor_x, c);
                buffer.cursor_x += 1;
            },
            KeyCode::Backspace => {
                if buffer.cursor_x > 0 {
                    buffer.cursor_x -= 1;
                    buffer.document.delete_char(buffer.cursor_y, buffer.cursor_x);
                }
            },
            KeyCode::Enter => {
                // Handle enter in insert mode (split line)
                let new_line = String::new();
                buffer.document.lines.insert(buffer.cursor_y + 1, new_line);
                buffer.cursor_y += 1;
                buffer.cursor_x = 0;
            },
            _ => {}
        }
        
        Ok(())
    }
    
    fn process_shell_mode(&mut self, key: KeyEvent) -> Result<()> {
        if self.buffers.is_empty() || self.active_buffer >= self.buffers.len() {
            return Ok(());
        }
        
        let buffer = &mut self.buffers[self.active_buffer];
        
        if !buffer.is_shell || buffer.shell.is_none() {
            self.mode = self.previous_mode; 
            return Ok(());
        }
        
        let shell = buffer.shell.as_mut().unwrap();
        shell.poll_output(); // Poll output before processing key

        if !shell.running {
            self.mode = self.previous_mode;
            info!("Shell is not running. Switching to mode: {:?}", self.mode);
            // Optionally close the buffer or mark as non-interactive
            // For now, if the user tries to type into a dead shell, they'll just switch out.
            return Ok(());
        }
        
        match key.code {
            KeyCode::Esc => {
                self.mode = self.previous_mode; // Revert to previous mode
            },
            KeyCode::Enter => {
                shell.execute_command()?; // This now sends to the child shell
                // poll_output will be called at the start of the next refresh_screen or keypress
            },
            KeyCode::Char(c) => {
                shell.input_char(c);
            },
            KeyCode::Backspace => {
                shell.input_backspace();
            },
            KeyCode::Delete => {
                shell.input_delete();
            },
            KeyCode::Left => {
                shell.move_cursor_left();
            },
            KeyCode::Right => {
                shell.move_cursor_right();
            },
            KeyCode::Up => {
                shell.history_up();
            },
            KeyCode::Down => {
                shell.history_down();
            },
            _ => {}
        }
        
        Ok(())
    }
    
    fn process_keypress(&mut self) -> Result<()> {
        match event::read()? {
            Event::Key(key_event) => {
                match self.mode {
                    Mode::Normal => {
                        if self.waiting_for_second_key {
                            self.process_second_key(key_event)?;
                        } else {
                            self.process_normal_mode(key_event)?;
                        }
                    },
                    Mode::Insert => self.process_insert_mode(key_event)?,
                    Mode::Visual => self.process_visual_mode(key_event)?,
                    Mode::Command => self.process_command_mode(key_event)?,
                    Mode::FileTree => self.process_file_tree_mode(key_event)?,
                    Mode::Shell => self.process_shell_mode(key_event)?,
                    Mode::Help => self.process_help_mode(key_event)?, // Added help mode processing
                }
            },
            Event::Mouse(mouse_event) => {
                self.process_mouse_event(mouse_event)?;
            },
            _ => {}
        }
        
        Ok(())
    }
    
    fn process_normal_mode(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char(' ') => {
                self.waiting_for_second_key = true;
                return Ok(());
            },
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char(':') => {
                self.mode = Mode::Command;
                self.command_line.clear();
            },
            KeyCode::Char('i') => self.mode = Mode::Insert,
            KeyCode::Char('v') => self.mode = Mode::Visual,
            KeyCode::Char('h') => self.move_cursor_left(),
            KeyCode::Char('j') => self.move_cursor_down(),
            KeyCode::Char('k') => self.move_cursor_up(),
            KeyCode::Char('l') => self.move_cursor_right(),
            KeyCode::Char('w') => self.move_to_next_word_start(),
            KeyCode::Char('e') => self.move_to_next_word_end(),
            KeyCode::Char('b') => self.move_to_prev_word_start(),
            _ => {}
        }
        
        Ok(())
    }
    
    fn process_visual_mode(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Char('h') => self.move_cursor_left(),
            KeyCode::Char('j') => self.move_cursor_down(),
            KeyCode::Char('k') => self.move_cursor_up(),
            KeyCode::Char('l') => self.move_cursor_right(),
            _ => {}
        }
        
        Ok(())
    }
    
    fn process_command_mode(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                self.execute_command()?;
                // execute_command might change the mode (e.g. to Help)
                // so only switch to Normal if not already changed.
                if self.mode == Mode::Command {
                    self.mode = Mode::Normal;
                }
            },
            KeyCode::Backspace => {
                self.command_line.pop();
            },
            KeyCode::Char(c) => {
                self.command_line.push(c);
            },
            _ => {}
        }
        
        Ok(())
    }
    
    fn process_file_tree_mode(&mut self, key: KeyEvent) -> Result<()> {
        if let Some(tree) = &mut self.file_tree {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => { // Added 'q' to close file tree
                    tree.toggle_visible();
                    self.mode = self.previous_mode;
                },
                KeyCode::Char('j') => {
                    tree.move_cursor_down();
                },
                KeyCode::Char('k') => {
                    tree.move_cursor_up();
                },
                KeyCode::Enter | KeyCode::Char('l') => {
                    // Store values for later use to avoid borrow conflicts
                    let opt_path_result = if let Some(path) = tree.get_selected_path() {
                        if path.is_dir() {
                            tree.toggle_expand()?;
                            None
                        } else {
                            Some(path)
                        }
                    } else {
                        None
                    };
                    
                    // Now handle file opening if needed
                    if let Some(path) = opt_path_result {
                        // Open the selected file
                        match Buffer::from_file(path.to_str().unwrap()) {
                            Ok(buffer) => {
                                if !self.buffers.is_empty() {
                                    self.buffers[self.active_buffer] = buffer;
                                } else {
                                    self.buffers.push(buffer);
                                    self.active_buffer = 0;
                                }
                                tree.toggle_visible();
                                self.mode = self.previous_mode;
                            },
                            Err(e) => {
                                return Err(e);
                            }
                        }
                    }
                },
                KeyCode::Char('h') => {
                    // First get the required information
                    let (is_dir, is_expanded, path_clone) = if let Some(path) = tree.get_selected_path() {
                        let is_expanded = tree.is_directory_expanded(path.clone());
                        (path.is_dir(), is_expanded, path)
                    } else {
                        (false, false, PathBuf::new())
                    };
                    
                    // Then perform the actions
                    if is_dir && is_expanded {
                        // If directory is expanded, collapse it
                        tree.toggle_expand()?;
                    } else {
                        // Otherwise move to parent directory if possible
                        tree.move_to_parent()?;
                    }
                },
                _ => {}
            }
        }
        
        Ok(())
    }
    
    fn process_second_key(&mut self, key: KeyEvent) -> Result<()> {
        self.waiting_for_second_key = false;
        
        match key.code {
            KeyCode::Char('e') => {
                // Toggle file tree visibility
                if let Some(tree) = &mut self.file_tree {
                    tree.toggle_visible();
                    if tree.visible {
                        self.previous_mode = self.mode;
                        self.mode = Mode::FileTree;
                    } else {
                        self.mode = self.previous_mode;
                    }
                }
            },
            KeyCode::Char('v') => {
                // Open vertical shell
                self.open_shell(false)?;
            },
            KeyCode::Char('h') => {
                // Open horizontal shell
                self.open_shell(true)?;
            },
            KeyCode::Char('w') => {
                // Cycle through windows
                self.cycle_window();
            },
            KeyCode::Char('q') => {
                // Close the current window
                self.close_window()?;
            },
            KeyCode::Char('x') => {
                // Close the current buffer
                self.close_current_buffer()?;
            },
            _ => {
                // Ignore other keys after space
            }
        }
        
        Ok(())
    }
    
    fn process_help_mode(&mut self, _key: KeyEvent) -> Result<()> {
        // Any key closes help
        self.mode = Mode::Normal;
        execute!(io::stdout(), cursor::Show)?; // Ensure cursor is shown when leaving help
        Ok(())
    }
    
    fn process_mouse_event(&mut self, event: event::MouseEvent) -> Result<()> {
        // Disable mouse events when help is active
        if self.mode == Mode::Help {
            if let event::MouseEventKind::Down(_) = event.kind {
                self.mode = Mode::Normal; // Close help on mouse click
                execute!(io::stdout(), cursor::Show)?;
            }
            return Ok(());
        }

        match event.kind {
            event::MouseEventKind::Down(event::MouseButton::Left) => {
                // Handle mouse click for positioning cursor or selecting window
                let (mouse_x, mouse_y) = (event.column as usize, event.row as usize);
                
                // Check if click is in the file tree area
                if self.mode == Mode::FileTree || 
                   self.file_tree.as_ref().map_or(false, |tree| tree.visible && mouse_x < tree.width) {
                    if let Some(tree) = &mut self.file_tree {
                        if mouse_y < self.terminal_height.saturating_sub(2) {
                            if mouse_y < tree.entries.len() {
                                tree.cursor = mouse_y;
                                // Handle double click (simulated with single click here)
                                if let Some(path) = tree.get_selected_path() {
                                    if path.is_dir() {
                                        tree.toggle_expand()?;
                                    } else {
                                        // Open file
                                        let buffer = Buffer::from_file(path.to_str().unwrap())?;
                                        if !self.buffers.is_empty() {
                                            self.buffers[self.active_buffer] = buffer;
                                        } else {
                                            self.buffers.push(buffer);
                                            self.active_buffer = 0;
                                        }
                                        tree.toggle_visible();
                                        self.mode = self.previous_mode;
                                    }
                                }
                            }
                        }
                        return Ok(());
                    }
                }
                
                // Check if click is in a window and select it
                let filetree_offset = if let Some(tree) = &self.file_tree {
                    if tree.visible { tree.width + 1 } else { 0 }
                } else { 0 };
                
                // First collect windows needing updates
                let mut selected_window_idx = None;
                let mut buffer_cursor_update = None;
                
                for (idx, window) in self.windows.iter().enumerate() {
                    let adjusted_x = window.x + filetree_offset;
                    let window_x_end = adjusted_x + window.width;
                    let window_y_end = window.y + window.height;
                    
                    if mouse_x >= adjusted_x && mouse_x < window_x_end &&
                       mouse_y >= window.y && mouse_y < window_y_end {
                        
                        // Mark this window for selection
                        if idx != self.active_window {
                            selected_window_idx = Some(idx);
                        }
                        
                        // Calculate cursor position within the document
                        let content_x_offset = if self.windows.len() > 1 { 1 } else { 0 };
                        let content_y_offset = if self.windows.len() > 1 { 1 } else { 0 };
                        
                        let buffer_x = mouse_x.saturating_sub(adjusted_x + content_x_offset);
                        let buffer_y = mouse_y.saturating_sub(window.y + content_y_offset);
                        
                        if !self.buffers.is_empty() && self.active_buffer < self.buffers.len() {
                            buffer_cursor_update = Some((buffer_x, buffer_y));
                        }
                        
                        break;
                    }
                }
                
                // Now apply the window selection
                if let Some(new_active_window) = selected_window_idx {
                    self.windows[self.active_window].is_active = false;
                    self.active_window = new_active_window;
                    self.windows[self.active_window].is_active = true;
                }
                
                // Update buffer cursor position
                if let Some((buffer_x, buffer_y)) = buffer_cursor_update {
                    if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                        // Update buffer cursor position
                        buffer.cursor_x = buffer.offset_x + buffer_x;
                        buffer.cursor_y = buffer.offset_y + buffer_y;
                        
                        // Ensure cursor is within document bounds
                        if buffer.cursor_y >= buffer.document.lines.len() {
                            buffer.cursor_y = buffer.document.lines.len().saturating_sub(1);
                        }
                        
                        if buffer.cursor_y < buffer.document.lines.len() {
                            let line_len = buffer.document.lines[buffer.cursor_y].len();
                            if buffer.cursor_x > line_len {
                                buffer.cursor_x = line_len;
                            }
                        }
                    }
                }
            },
            event::MouseEventKind::ScrollUp => {
                // Scroll up in the active window/buffer
                if self.mode == Mode::FileTree {
                    if let Some(tree) = &mut self.file_tree {
                        tree.move_cursor_up();
                    }
                } else if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    if buffer.offset_y > 0 {
                        buffer.offset_y -= 1;
                    }
                }
            },
            event::MouseEventKind::ScrollDown => {
                // Scroll down in the active window/buffer
                if self.mode == Mode::FileTree {
                    if let Some(tree) = &mut self.file_tree {
                        tree.move_cursor_down();
                    }
                } else if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    if buffer.offset_y + self.windows[self.active_window].height < buffer.document.lines.len() {
                        buffer.offset_y += 1;
                    }
                }
            },
            _ => {}
        }
        
        Ok(())
    }
    
    fn execute_command(&mut self) -> Result<()> {
        match self.command_line.as_str() {
            "w" => {
                if self.buffers.is_empty() || self.active_buffer >= self.buffers.len() {
                    return Err(Error::Message("No buffer to save".to_string()));
                }
                
                if let Err(e) = self.buffers[self.active_buffer].save() {
                    self.command_line = format!("Error saving: {}", e);
                    return Err(e);
                } else {
                    self.command_line = "File saved".to_string();
                    return Ok(());
                }
            },
            "q" => {
                self.quit = true;
                Ok(())
            },
            "wq" => {
                if self.buffers.is_empty() || self.active_buffer >= self.buffers.len() {
                    return Err(Error::Message("No buffer to save".to_string()));
                }
                
                if let Err(e) = self.buffers[self.active_buffer].save() {
                    self.command_line = format!("Error saving: {}", e);
                    return Err(e);
                } else {
                    self.quit = true;
                    return Ok(());
                }
            },
            "help" => {
                self.previous_mode = self.mode;
                self.mode = Mode::Help;
                self.command_line.clear();
                Ok(())
            },
            _ => {
                self.command_line = format!("Unknown command: {}", self.command_line);
                Ok(())
            }
        }
    }
    
    // Cursor movement methods
    fn move_cursor_left(&mut self) {
        if self.buffers.is_empty() || self.active_buffer >= self.buffers.len() {
            return;
        }
        
        let buffer = &mut self.buffers[self.active_buffer];
        if buffer.cursor_x > 0 {
            buffer.cursor_x -= 1;
        }
    }
    
    fn move_cursor_right(&mut self) {
        if self.buffers.is_empty() || self.active_buffer >= self.buffers.len() {
            return;
        }
        
        let buffer = &mut self.buffers[self.active_buffer];
        if buffer.cursor_y < buffer.document.lines.len() {
            let line_len = buffer.document.lines[buffer.cursor_y].len();
            if buffer.cursor_x < line_len {
                buffer.cursor_x += 1;
            }
        }
    }
    
    fn move_cursor_up(&mut self) {
        if self.buffers.is_empty() || self.active_buffer >= self.buffers.len() {
            return;
        }
        
        let buffer = &mut self.buffers[self.active_buffer];
        if buffer.cursor_y > 0 {
            buffer.cursor_y -= 1;
            let line_len = buffer.document.lines[buffer.cursor_y].len();
            if buffer.cursor_x > line_len {
                buffer.cursor_x = line_len;
            }
        }
    }
    
    fn move_cursor_down(&mut self) {
        if self.buffers.is_empty() || self.active_buffer >= self.buffers.len() {
            return;
        }
        
        let buffer = &mut self.buffers[self.active_buffer];
        if buffer.cursor_y < buffer.document.lines.len() - 1 {
            buffer.cursor_y += 1;
            let line_len = buffer.document.lines[buffer.cursor_y].len();
            if buffer.cursor_x > line_len {
                buffer.cursor_x = line_len;
            }
        }
    }
    
    // Word navigation methods
    fn move_to_next_word_start(&mut self) {
        if self.buffers.is_empty() || self.active_buffer >= self.buffers.len() {
            return;
        }
        
        let buffer = &mut self.buffers[self.active_buffer];
        
        if buffer.cursor_y >= buffer.document.lines.len() {
            return;
        }
        
        let line = &buffer.document.lines[buffer.cursor_y];
        let mut found = false;
        
        // Try to find next word in current line
        if buffer.cursor_x < line.len() {
            let mut in_word = !Editor::is_word_separator(line.chars().nth(buffer.cursor_x).unwrap_or(' '));
            
            for (i, c) in line.chars().enumerate().skip(buffer.cursor_x + 1) {
                if in_word && Editor::is_word_separator(c) {
                    in_word = false;
                } else if !in_word && !Editor::is_word_separator(c) {
                    buffer.cursor_x = i;
                    found = true;
                    break;
                }
            }
        }
        
        // If no word found in current line, move to next line
        if !found && buffer.cursor_y < buffer.document.lines.len() - 1 {
            buffer.cursor_y += 1;
            buffer.cursor_x = 0;
            
            // If the next line is not empty, find first word
            if !buffer.document.lines[buffer.cursor_y].is_empty() {
                let line = &buffer.document.lines[buffer.cursor_y];
                for (i, c) in line.chars().enumerate() {
                    if !Editor::is_word_separator(c) {
                        buffer.cursor_x = i;
                        break;
                    }
                }
            }
        }
    }
    
    fn move_to_next_word_end(&mut self) {
        if self.buffers.is_empty() || self.active_buffer >= self.buffers.len() {
            return;
        }
        
        let buffer = &mut self.buffers[self.active_buffer];
        
        if buffer.cursor_y >= buffer.document.lines.len() {
            return;
        }
        
        let line = &buffer.document.lines[buffer.cursor_y];
        let mut found = false;
        
        // Try to find word end in current line
        if buffer.cursor_x < line.len() {
            let mut in_word = !Editor::is_word_separator(line.chars().nth(buffer.cursor_x).unwrap_or(' '));
            
            for (i, c) in line.chars().enumerate().skip(buffer.cursor_x + 1) {
                if !in_word && !Editor::is_word_separator(c) {
                    in_word = true;
                } else if in_word && Editor::is_word_separator(c) {
                    buffer.cursor_x = i - 1;
                    found = true;
                    break;
                }
            }
            
            // Check if we're at the end of a word at the end of the line
            if !found && in_word && buffer.cursor_x < line.len() - 1 {
                buffer.cursor_x = line.len() - 1;
                found = true;
            }
        }
        
        // If no word end found in current line, move to next line
        if !found && buffer.cursor_y < buffer.document.lines.len() - 1 {
            buffer.cursor_y += 1;
            buffer.cursor_x = 0;
            
            // Find first word end in the next line
            let line = &buffer.document.lines[buffer.cursor_y];
            if !line.is_empty() {
                let mut in_word = !Editor::is_word_separator(line.chars().next().unwrap_or(' '));
                
                for (i, c) in line.chars().enumerate().skip(1) {
                    if in_word && Editor::is_word_separator(c) {
                        buffer.cursor_x = i - 1;
                        found = true;
                        break;
                    } else if !in_word && !Editor::is_word_separator(c) {
                        in_word = true;
                    }
                }
                
                // If we have a word at the end of the line
                if !found && in_word && !line.is_empty() {
                    buffer.cursor_x = line.len() - 1;
                }
            }
        }
    }
    
    fn move_to_prev_word_start(&mut self) {
        if self.buffers.is_empty() || self.active_buffer >= self.buffers.len() {
            return;
        }
        
        let buffer = &mut self.buffers[self.active_buffer];
        
        if buffer.cursor_y >= buffer.document.lines.len() {
            return;
        }
        
        let line = &buffer.document.lines[buffer.cursor_y];
        let mut found = false;
        
        // Try to find previous word in current line
        if buffer.cursor_x > 0 {
            let mut pos = buffer.cursor_x;
            
            // If we're in the middle of a word, go to its start
            while pos > 0 && !Editor::is_word_separator(line.chars().nth(pos - 1).unwrap_or(' ')) {
                pos -= 1;
            }
            
            // If we moved, we found the start of the current word
            if pos < buffer.cursor_x {
                buffer.cursor_x = pos;
                found = true;
            } else {
                // Otherwise we need to find the previous word
                while pos > 0 {
                    pos -= 1;
                    if !Editor::is_word_separator(line.chars().nth(pos).unwrap_or(' ')) {
                        // We found a word character, now go to its start
                        while pos > 0 && !Editor::is_word_separator(line.chars().nth(pos - 1).unwrap_or(' ')) {
                            pos -= 1;
                        }
                        buffer.cursor_x = pos;
                        found = true;
                        break;
                    }
                }
            }
        }
        
    }

    fn cycle_window(&mut self) {
        if self.windows.len() > 1 {
            self.windows[self.active_window].is_active = false;
            self.active_window = (self.active_window + 1) % self.windows.len();
            self.windows[self.active_window].is_active = true;
            info!("Cycled to window {}", self.active_window);
        }
    }
    
    fn close_window(&mut self) -> Result<()> {
        if self.windows.len() <= 1 {
            // Optionally, quit if it's the last window and buffer
            if self.buffers.len() <= 1 && self.mode != Mode::Help { // Don't quit if help is shown over last buffer
                self.quit = true;
                info!("Closing the last window and buffer. Quitting.");
            } else {
                info!("Cannot close the last window if other buffers exist or help is active.");
            }
            return Ok(());
        }

        self.windows.remove(self.active_window);

        if self.active_window >= self.windows.len() {
            self.active_window = self.windows.len() - 1;
        }
        
        if !self.windows.is_empty() {
            self.windows[self.active_window].is_active = true;
        }

        // Potentially close the associated buffer if it's no longer used by any window
        // For now, we'll keep buffer management separate or simplify it.
        // If the active buffer was associated with the closed window,
        // and no other window uses it, we might want to close it.
        // This part needs more sophisticated logic if buffers are tied to windows.
        // For now, closing a window doesn't automatically close its buffer.
        // User can use <space>x to close buffer.

        info!("Closed window. Active window is now {}", self.active_window);
        Ok(())
    }

    // Helper method to check for word separators
    fn is_word_separator(c: char) -> bool {
        c.is_whitespace() || c.is_ascii_punctuation()
    }
    
    fn draw_help_screen(&self) -> Result<()> {
        let help_title = " RVim Keybindings ";
        let help_content = vec![
            "",
            "Normal Mode:",
            "  i             - Enter Insert Mode",
            "  v             - Enter Visual Mode",
            "  :             - Enter Command Mode",
            "  h/j/k/l       - Navigate cursor",
            "  w             - Move to next word start",
            "  e             - Move to next word end",
            "  b             - Move to previous word start",
            "  q             - Quit RVim (in some contexts, e.g. single buffer)",
            "",
            "Space Leader Key (Normal Mode):",
            "  <space> e     - Toggle File Tree",
            "  <space> v     - Open Vertical Shell",
            "  <space> h     - Open Horizontal Shell",
            "  <space> w     - Cycle Windows",
            "  <space> q     - Close Current Window",
            "  <space> x     - Close Current Buffer",
            "",
            "Insert Mode:",
            "  Esc           - Exit to Normal Mode",
            "  Backspace     - Delete char before cursor",
            "  Enter         - New line",
            "",
            "Command Mode:",
            "  Esc           - Exit to Normal Mode",
            "  Enter         - Execute command",
            "  :w            - Save file",
            "  :q            - Quit",
            "  :wq           - Save and Quit",
            "  :help         - Show this help screen",
            "",
            "File Tree Mode:",
            "  Esc / q       - Close File Tree",
            "  j / k         - Navigate up/down",
            "  l / Enter     - Open file / Expand directory",
            "  h             - Collapse directory / Go to parent",
            "",
            "Shell Mode:",
            "  Esc           - Return to previous mode (shell process continues)",
            "  Enter         - Send command to shell",
            "  exit          - (Typed in RVim prompt) Close shell process & return",
            "  Arrow Up/Down - Command history (RVim's input line)",
            "",
            "  Note: To exit the actual shell process, type its native exit command",
            "        (e.g., 'exit' or 'logout') directly into the shell.",
        ];

        let popup_width = help_content.iter().map(|s| s.len()).max().unwrap_or(70).max(help_title.len()) + 4;
        let popup_height = help_content.len() + 4;

        let term_width = self.terminal_width;
        let term_height = self.terminal_height.saturating_sub(2); // Account for status/message line

        let start_x = (term_width.saturating_sub(popup_width)) / 2;
        let start_y = (term_height.saturating_sub(popup_height)) / 2;

        // Draw border
        execute!(io::stdout(), SetBackgroundColor(Color::DarkGrey), SetForegroundColor(Color::White))?;
        for y in 0..popup_height {
            for x in 0..popup_width {
                execute!(io::stdout(), cursor::MoveTo((start_x + x) as u16, (start_y + y) as u16))?;
                if y == 0 || y == popup_height - 1 {
                    if x == 0 { print!("{}", if y == 0 { "┌" } else { "└" }); }
                    else if x == popup_width - 1 { print!("{}", if y == 0 { "┐" } else { "┘" }); }
                    else { print!("─"); }
                } else if x == 0 || x == popup_width - 1 {
                    print!("│");
                } else {
                    print!(" "); // Fill background
                }
            }
        }
        
        // Draw title
        let title_x = start_x + (popup_width.saturating_sub(help_title.len())) / 2;
        execute!(io::stdout(), cursor::MoveTo(title_x as u16, start_y as u16), SetBackgroundColor(Color::Blue))?;
        print!("{}", help_title);
        execute!(io::stdout(), SetBackgroundColor(Color::DarkGrey))?;


        // Draw content
        execute!(io::stdout(), SetForegroundColor(Color::White))?;
        for (i, line) in help_content.iter().enumerate() {
            let line_x = start_x + 2;
            let line_y = start_y + 2 + i;
            if line_y < start_y + popup_height -1 { // Ensure content is within bounds
                 execute!(io::stdout(), cursor::MoveTo(line_x as u16, line_y as u16))?;
                 print!("{}", line);
            }
        }

        execute!(io::stdout(), ResetColor)?;
        Ok(())
    }
}