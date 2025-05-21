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
use crate::cli::tabs::TabManager;
use crate::error::{Error, Result};
use crate::cli::buffer::Buffer; // Use the buffer module's Buffer type
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

// Editor modes
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Normal,
    Insert,
    Visual,
    Command,
    FileTree,
    Shell,
    Help,
    TabSwitcher, // Add new mode for tab switching
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
    tab_manager: TabManager,
    fuzzy_matcher: SkimMatcherV2,
    fuzzy_results: Vec<(String, i64)>, // (path, score)
    command_palette_items: Vec<String>,
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
            tab_manager: TabManager::new(),
            fuzzy_matcher: SkimMatcherV2::default(),
            fuzzy_results: Vec::new(),
            command_palette_items: Vec::new(),
        };
        
        // Load Lua configuration
        editor.load_config()?;
        
        // Initialize file tree with current directory
        let current_dir = env::current_dir()?;
        editor.file_tree = Some(FileTree::new(&current_dir)?);
        
        // Initialize command palette items
        editor.command_palette_items = vec![
            ":w".to_string(),
            ":q".to_string(),
            ":wq".to_string(),
            ":help".to_string(),
            // Add more commands
        ];
        
        Ok(editor)
    }
    
    pub fn open_file(&mut self, filename: &str) -> Result<()> {
        let buffer = Buffer::from_file(filename)?;
        
        // Create a new tab for the file
        self.tab_manager.create_tab(filename.to_string(), buffer)?;
        
        // Update file tree path to new file's directory
        let path = PathBuf::from(filename);
        if let Some(parent) = path.parent() {
            self.file_tree = Some(FileTree::new(parent)?);
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
    
    fn draw_tabs(&self) -> Result<()> {
        let start_x = 0;
        let start_y = 0;
        let tab_list = self.tab_manager.tab_list();
        let mut current_x = start_x;

        execute!(io::stdout(), cursor::MoveTo(0, 0))?;

        // Draw tab bar background
        execute!(
            io::stdout(),
            SetBackgroundColor(Color::DarkGrey),
            SetForegroundColor(Color::White)
        )?;

        for x in 0..self.terminal_width {
            execute!(io::stdout(), cursor::MoveTo(x as u16, start_y as u16))?;
            print!(" ");
        }

        // Draw each tab
        for (idx, (id, name)) in tab_list.iter().enumerate() {
            let is_current = idx == self.tab_manager.current_tab();
            let tab_style = if is_current {
                execute!(
                    io::stdout(),
                    SetBackgroundColor(Color::Blue),
                    SetForegroundColor(Color::White)
                )
            } else {
                execute!(
                    io::stdout(),
                    SetBackgroundColor(Color::DarkGrey),
                    SetForegroundColor(Color::White)
                )
            }?;

            let tab_text = format!(" {} ", name);
            execute!(io::stdout(), cursor::MoveTo(current_x as u16, start_y as u16))?;
            print!("{}", tab_text);
            
            current_x += tab_text.len();
        }

        execute!(io::stdout(), ResetColor)?;
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

        // Draw tabs at the top
        self.draw_tabs()?;

        // Adjust other content to start below tabs
        let content_offset = 1; // Height of tab bar

        if self.mode == Mode::Help {
            self.draw_help_screen()?;
        } else {
            // Adjust filetree and windows to start below tabs
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
            let total_lines = buffer.document.lines.len();
            let gutter_width = total_lines.to_string().len().max(2);
            for y in 0..effective_height {
                let file_row = y + buffer.offset_y;
                execute!(io::stdout(),
                    cursor::MoveTo(content_x as u16, (content_y + y) as u16)
                )?;
                // line-number gutter
                if file_row < total_lines {
                    print!("{:>width$} ", file_row + 1, width = gutter_width);
                } else {
                    print!("{:width$} ", "", width = gutter_width);
                }
                // then the text
                if file_row >= buffer.document.lines.len() {
                    print!(" ");
                } else {
                    let line = &buffer.document.lines[file_row];
                    let start = buffer.offset_x.min(line.len());
                    let end = (buffer.offset_x + effective_width - gutter_width - 1).min(line.len());
                    if start < end {
                        print!("{}", &line[start..end]);
                    }
                }
            }
        }
        
        Ok(())
    }
    
    fn draw_status_line(&self) -> Result<()> {
        // File and position info
        let (line, col, total) = if let Some(buf) = self.buffers.get(self.active_buffer) {
            let l = buf.cursor_y + 1;
            let c = buf.cursor_x + 1;
            let t = buf.document.lines.len();
            (l, c, t)
        } else { (0,0,0) };
        let pct = if total > 0 { (line as f32 / total as f32) * 100.0 } else { 0.0 };
        let pos_info = format!("L{}/C{}  {}/{} ({:.0}%)", line, col, line, total, pct);

        let status = match self.mode {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Visual => "VISUAL",
            Mode::Command => "COMMAND",
            Mode::FileTree => "FILETREE",
            Mode::Shell => "SHELL",
            Mode::Help => "HELP",
            Mode::TabSwitcher => "TAB",
        };
        let fname = self.buffers
            .get(self.active_buffer)
            .and_then(|b| b.filename.clone())
            .unwrap_or("[No Name]".into());
        let modified = if let Some(b) = self.buffers.get(self.active_buffer) {
            if b.document.modified { "[+]" } else { "" }
        } else { "" };
        let status_line = format!(" {} | {}{} | {} ",
            status, fname, modified, pos_info);

        execute!(
            io::stdout(),
            cursor::MoveTo(0, self.terminal_height as u16 - 2),
            SetForegroundColor(Color::Black),
            SetBackgroundColor(Color::White)
        )?;
        let pad = self.terminal_width.saturating_sub(status_line.len());
        print!("{}{}", status_line, " ".repeat(pad));
        execute!(io::stdout(), ResetColor)?;
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
                    Mode::Help => self.process_help_mode(key_event)?,
                    Mode::TabSwitcher => self.process_tab_switcher_mode(key_event)?,
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
                Ok(())
            },
            KeyCode::Char('q') => {
                self.quit = true;
                Ok(())
            },
            KeyCode::Char(':') => {
                self.mode = Mode::Command;
                self.command_line.clear();
                Ok(())
            },
            KeyCode::Char('i') => {
                self.mode = Mode::Insert;
                Ok(())
            },
            KeyCode::Char('v') => {
                self.mode = Mode::Visual;
                Ok(())
            },
            KeyCode::Char('h') => self.move_cursor_left(),
            KeyCode::Char('j') => self.move_cursor_down(),
            KeyCode::Char('k') => self.move_cursor_up(),
            KeyCode::Char('l') => self.move_cursor_right(),
            KeyCode::Char('w') => self.move_to_next_word_start(),
            KeyCode::Char('e') => self.move_to_next_word_end(),
            KeyCode::Char('b') => self.move_to_prev_word_start(),
            KeyCode::Char('d') => {
                self.delete_current_line()?;
                Ok(())
            },
            KeyCode::Char('x') => {
                self.delete_char_under_cursor()?;
                Ok(())
            },
            _ => Ok(())
        }
    }
    
    fn process_visual_mode(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                Ok(())
            },
            KeyCode::Char('h') => {
                self.move_cursor_left()?;
                Ok(())
            },
            KeyCode::Char('j') => {
                self.move_cursor_down()?;
                Ok(())
            },
            KeyCode::Char('k') => {
                self.move_cursor_up()?;
                Ok(())
            },
            KeyCode::Char('l') => {
                self.move_cursor_right()?;
                Ok(())
            },
            _ => Ok(())
        }
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
                if let Some(tree) = &mut self.file_tree {
                    tree.toggle_visible();
                    if tree.visible {
                        self.previous_mode = self.mode;
                        self.mode = Mode::FileTree;
                    } else {
                        self.mode = self.previous_mode;
                    }
                }
                Ok(())
            },
            KeyCode::Char('v') => {
                self.open_shell(false)
            },
            KeyCode::Char('h') => {
                self.open_shell(true)
            },
            KeyCode::Char('w') => {
                self.cycle_window()
            },
            KeyCode::Char('q') => {
                self.close_window()
            },
            KeyCode::Char('x') => {
                self.close_current_buffer()
            },
            KeyCode::Tab => {
                self.tab_manager.switch_to_next_tab()
            },
            KeyCode::BackTab => {
                self.tab_manager.switch_to_prev_tab()
            },
            _ => Ok(()),
        }
    }

    fn execute_command(&mut self) -> Result<()> {
        let cmd = self.command_line.trim();
        match cmd {
            "q" | "quit" => {
                self.quit = true;
                Ok(())
            },
            "w" | "write" => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.save()?;
                }
                Ok(())
            },
            "wq" => {
                if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
                    buffer.save()?;
                }
                self.quit = true;
                Ok(())
            },
            "help" => {
                self.previous_mode = self.mode;
                self.mode = Mode::Help;
                Ok(())
            },
            _ => Ok(()) // Unknown command just returns Ok
        }
    }

    fn move_cursor_left(&mut self) -> Result<()> {
        if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
            if buffer.cursor_x > 0 {
                buffer.cursor_x -= 1;
            }
        }
        Ok(())
    }

    fn move_cursor_right(&mut self) -> Result<()> {
        if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
            let line_len = buffer.document.lines.get(buffer.cursor_y)
                .map(|line| line.len())
                .unwrap_or(0);
            if buffer.cursor_x < line_len {
                buffer.cursor_x += 1;
            }
        }
        Ok(())
    }

    fn move_cursor_up(&mut self) -> Result<()> {
        if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
            if buffer.cursor_y > 0 {
                buffer.cursor_y -= 1;
            }
        }
        Ok(())
    }

    fn move_cursor_down(&mut self) -> Result<()> {
        if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
            if buffer.cursor_y < buffer.document.lines.len().saturating_sub(1) {
                buffer.cursor_y += 1;
            }
        }
        Ok(())
    }

    fn move_to_next_word_start(&mut self) -> Result<()> {
        // Implementation coming soon
        Ok(())
    }

    fn move_to_next_word_end(&mut self) -> Result<()> {
        // Implementation coming soon
        Ok(())
    }

    fn move_to_prev_word_start(&mut self) -> Result<()> {
        // Implementation coming soon
        Ok(())
    }

    fn cycle_window(&mut self) -> Result<()> {
        if !self.windows.is_empty() {
            self.active_window = (self.active_window + 1) % self.windows.len();
        }
        Ok(())
    }

    fn close_window(&mut self) -> Result<()> {
        if self.windows.len() > 1 {
            self.windows.remove(self.active_window);
            if self.active_window >= self.windows.len() {
                self.active_window = self.windows.len() - 1;
            }
        }
        Ok(())
    }

    fn process_tab_switcher_mode(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                Ok(())
            },
            KeyCode::Tab => self.tab_manager.switch_to_next_tab(),
            KeyCode::BackTab => self.tab_manager.switch_to_prev_tab(),
            _ => Ok(())
        }
    }

    fn process_mouse_event(&mut self, event: event::MouseEvent) -> Result<()> {
        match event.kind {
            event::MouseEventKind::Down(button) => {
                // Handle mouse clicks
                let (x, y) = (event.column as usize, event.row as usize);
                match button {
                    event::MouseButton::Left => self.handle_left_click(x, y)?,
                    _ => {}
                }
            },
            _ => {}
        }
        Ok(())
    }

    fn handle_left_click(&mut self, x: usize, y: usize) -> Result<()> {
        // Update cursor position based on click
        if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
            buffer.cursor_x = x;
            buffer.cursor_y = y;
        }
        Ok(())
    }

    fn fuzzy_find_files(&mut self) -> Result<()> {
        let input = &self.command_line[2..]; // Skip ":f "
        self.fuzzy_results.clear();
        
        if let Some(tree) = &self.file_tree {
            for entry in &tree.entries {
                if let Some(score) = self.fuzzy_matcher.fuzzy_match(&entry.name, input) {
                    self.fuzzy_results.push((entry.name.clone(), score));
                }
            }
        }
        
        self.fuzzy_results.sort_by_key(|(_, score)| -score);
        Ok(())
    }
    
    fn show_command_palette(&mut self) -> Result<()> {
        let input = &self.command_line[1..]; // Skip ":"
        self.fuzzy_results.clear();
        
        for cmd in &self.command_palette_items {
            if let Some(score) = self.fuzzy_matcher.fuzzy_match(cmd, input) {
                self.fuzzy_results.push((cmd.clone(), score));
            }
        }
        
        self.fuzzy_results.sort_by_key(|(_, score)| -score);
        Ok(())
    }
    
    fn draw_help_screen(&mut self) -> Result<()> {
        let help_text = vec![
            "RVim Help",
            "=========",
            "",
            "Normal Mode:",
            "  h/j/k/l - Move cursor",
            "  i - Enter insert mode",
            "  v - Enter visual mode",
            "  : - Enter command mode",
            "  q - Quit",
            "",
            "Leader Commands (Space):",
            "  e - Toggle file tree",
            "  v - Open vertical shell",
            "  h - Open horizontal shell",
            "  w - Cycle windows",
            "  q - Close window",
            "  x - Close buffer",
            "",
            "Press any key to close help"
        ];

        // Clear screen first
        execute!(
            io::stdout(),
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )?;

        // Calculate center position
        let start_y = self.terminal_height.saturating_sub(help_text.len()) / 2;
        
        for (idx, line) in help_text.iter().enumerate() {
            let start_x = self.terminal_width.saturating_sub(line.len()) / 2;
            execute!(
                io::stdout(),
                cursor::MoveTo(start_x as u16, (start_y + idx) as u16)
            )?;
            print!("{}", line);
        }

        io::stdout().flush()?;
        Ok(())
    }

    fn process_help_mode(&mut self, key: KeyEvent) -> Result<()> {
        // Any key press exits help mode
        self.mode = self.previous_mode;
        Ok(())
    }

    // Delete the entire line at the cursor
    fn delete_current_line(&mut self) -> Result<()> {
        if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
            let row = buffer.cursor_y;
            if row < buffer.document.lines.len() {
                buffer.document.lines.remove(row);
                buffer.document.modified = true;
                // clamp cursor
                if buffer.cursor_y >= buffer.document.lines.len() && !buffer.document.lines.is_empty() {
                    buffer.cursor_y = buffer.document.lines.len() - 1;
                }
                buffer.cursor_x = 0;
            }
        }
        Ok(())
    }

    // Delete the character under the cursor
    fn delete_char_under_cursor(&mut self) -> Result<()> {
        if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
            if buffer.document.delete_char(buffer.cursor_y, buffer.cursor_x) {
                // clamp cursor_x to line length
                let line_len = buffer.document.lines[buffer.cursor_y].len();
                if buffer.cursor_x > line_len {
                    buffer.cursor_x = line_len;
                }
            }
        }
        Ok(())
    }
}