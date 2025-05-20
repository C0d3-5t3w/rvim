use std::error::Error;
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

// Editor modes
#[derive(Clone, Copy, Debug)]
enum Mode {
    Normal,
    Insert,
    Visual,
    Command,
    FileTree,
    Shell,
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

    fn from_file(filename: &str) -> Result<Self, Box<dyn Error>> {
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

    fn save(&mut self) -> Result<(), Box<dyn Error>> {
        if self.is_shell {
            return Err("Cannot save shell buffer".into());
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

    fn from_file(filename: &str) -> Result<Self, Box<dyn Error>> {
        let content = fs::read_to_string(filename)?;
        let lines: Vec<String> = content.lines().map(String::from).collect();
        let lines = if lines.is_empty() { vec![String::new()] } else { lines };
        
        Ok(Self {
            lines,
            filename: Some(filename.to_string()),
            modified: false,
        })
    }

    fn save(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(filename) = &self.filename {
            let content = self.lines.join("\n");
            fs::write(filename, content)?;
            self.modified = false;
            Ok(())
        } else {
            Err("No filename specified".into())
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
    pub fn new(config_path: PathBuf) -> Result<Self, Box<dyn Error>> {
        // Initialize terminal
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        
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
    
    pub fn open_file(&mut self, filename: &str) -> Result<(), Box<dyn Error>> {
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
    
    fn open_shell(&mut self, is_horizontal: bool) -> Result<(), Box<dyn Error>> {
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
    
    fn close_current_buffer(&mut self) -> Result<(), Box<dyn Error>> {
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
    
    fn load_config(&mut self) -> Result<(), Box<dyn Error>> {
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
    
    fn register_api(&mut self) -> Result<(), Box<dyn Error>> {
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
    
    pub fn run(&mut self) -> Result<(), Box<dyn Error>> {
        self.refresh_screen()?;
        
        while !self.quit {
            self.process_keypress()?;
            self.refresh_screen()?;
        }
        
        // Cleanup terminal on exit
        execute!(io::stdout(), LeaveAlternateScreen)?;
        terminal::disable_raw_mode()?;
        
        Ok(())
    }
    
    fn refresh_screen(&mut self) -> Result<(), Box<dyn Error>> {
        execute!(
            io::stdout(),
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )?;
        
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
        
        self.draw_status_line()?;
        self.draw_message_line()?;
        
        // Position cursor based on mode
        match self.mode {
            Mode::FileTree => {
                if let Some(tree) = &self.file_tree {
                    let tree_cursor_y = tree.cursor.min(self.terminal_height - 3);
                    execute!(io::stdout(), cursor::MoveTo(2, tree_cursor_y as u16))?;
                }
            },
            Mode::Shell => {
                if let Some(buffer) = self.buffers.get(self.active_buffer) {
                    if let Some(shell) = &buffer.shell {
                        let content_y = if self.windows.len() > 1 { 
                            self.windows[self.active_window].y + 1 
                        } else { 
                            0 
                        };
                        
                        let filetree_width = if let Some(tree) = &self.file_tree { 
                            if tree.visible { tree.width + 1 } else { 0 } 
                        } else { 0 };
                        
                        let content_x = if self.windows.len() > 1 { 
                            self.windows[self.active_window].x + filetree_width + 1 
                        } else { 
                            filetree_width 
                        };
                        
                        let shell_lines_count = shell.lines.len();
                        let cursor_pos = shell.cursor_pos + 2; // + 2 for "$ " prefix
                        
                        execute!(io::stdout(), cursor::MoveTo(
                            (content_x + cursor_pos) as u16, 
                            (content_y + shell_lines_count) as u16
                        ))?;
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
    
    fn draw_file_tree(&self) -> Result<(), Box<dyn Error>> {
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
    
    fn draw_window_borders(&self, window: &Window, adjusted_x: usize, is_active: bool) -> Result<(), Box<dyn Error>> {
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
    
    fn draw_window_content(&self, window: &Window, adjusted_x: usize) -> Result<(), Box<dyn Error>> {
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
            if let Some(shell) = &buffer.shell {
                let mut line_counter = 0;
                for (idx, line) in shell.lines.iter().enumerate() {
                    if line_counter >= effective_height {
                        break;
                    }
                    
                    execute!(io::stdout(), cursor::MoveTo(content_x as u16, (content_y + line_counter) as u16))?;
                    
                    if line.len() > effective_width {
                        print!("{}", &line[0..effective_width]);
                    } else {
                        print!("{}", line);
                    }
                    
                    line_counter += 1;
                }
                
                // Draw the current input line
                if line_counter < effective_height {
                    execute!(io::stdout(), cursor::MoveTo(content_x as u16, (content_y + line_counter) as u16))?;
                    if 2 + shell.input_line.len() > effective_width {
                        print!("$ {}", &shell.input_line[0..effective_width-2]);
                    } else {
                        print!("$ {}", shell.input_line);
                    }
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
    
    fn draw_status_line(&self) -> Result<(), Box<dyn Error>> {
        let status = match self.mode {
            Mode::Normal => " NORMAL ",
            Mode::Insert => " INSERT ",
            Mode::Visual => " VISUAL ",
            Mode::Command => " COMMAND ",
            Mode::FileTree => " FILE TREE ",
            Mode::Shell => " SHELL ",
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
    
    fn draw_message_line(&self) -> Result<(), Box<dyn Error>> {
        execute!(
            io::stdout(),
            cursor::MoveTo(0, self.terminal_height as u16 - 1),
            terminal::Clear(ClearType::CurrentLine)
        )?;
        
        if let Mode::Command = self.mode {
            print!(":{}", self.command_line);
        }
        
        Ok(())
    }
    
    fn process_keypress(&mut self) -> Result<(), Box<dyn Error>> {
        if let Event::Key(key_event) = event::read()? {
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
            }
        }
        
        Ok(())
    }
    
    fn process_second_key(&mut self, key: KeyEvent) -> Result<(), Box<dyn Error>> {
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
    
    fn process_normal_mode(&mut self, key: KeyEvent) -> Result<(), Box<dyn Error>> {
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
            _ => {}
        }
        
        Ok(())
    }
    
    fn process_visual_mode(&mut self, key: KeyEvent) -> Result<(), Box<dyn Error>> {
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
    
    fn process_command_mode(&mut self, key: KeyEvent) -> Result<(), Box<dyn Error>> {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                self.execute_command()?;
                self.mode = Mode::Normal;
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
    
    fn process_file_tree_mode(&mut self, key: KeyEvent) -> Result<(), Box<dyn Error>> {
        if let Some(tree) = &mut self.file_tree {
            match key.code {
                KeyCode::Esc => {
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
                    if let Some(path) = tree.get_selected_path() {
                        if path.is_dir() {
                            tree.toggle_expand()?;
                        } else {
                            // Open the selected file
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
                },
                KeyCode::Char('h') => {
                    tree.toggle_expand()?;
                },
                _ => {}
            }
        }
        
        Ok(())
    }
    
    fn save(&mut self) -> Result<(), Box<dyn Error>> {
        if self.is_shell {
            return Err("Cannot save shell buffer".into());
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

    fn from_file(filename: &str) -> Result<Self, Box<dyn Error>> {
        let content = fs::read_to_string(filename)?;
        let lines: Vec<String> = content.lines().map(String::from).collect();
        let lines = if lines.is_empty() { vec![String::new()] } else { lines };
        
        Ok(Self {
            lines,
            filename: Some(filename.to_string()),
            modified: false,
        })
    }

    fn save(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(filename) = &self.filename {
            let content = self.lines.join("\n");
            fs::write(filename, content)?;
            self.modified = false;
            Ok(())
        } else {
            Err("No filename specified".into())
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
    pub fn new(config_path: PathBuf) -> Result<Self, Box<dyn Error>> {
        // Initialize terminal
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        
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
    
    pub fn open_file(&mut self, filename: &str) -> Result<(), Box<dyn Error>> {
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
    
    fn open_shell(&mut self, is_horizontal: bool) -> Result<(), Box<dyn Error>> {
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
    
    fn close_current_buffer(&mut self) -> Result<(), Box<dyn Error>> {
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
    
    fn load_config(&mut self) -> Result<(), Box<dyn Error>> {
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
    
    fn register_api(&mut self) -> Result<(), Box<dyn Error>> {
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
    
    pub fn run(&mut self) -> Result<(), Box<dyn Error>> {
        self.refresh_screen()?;
        
        while !self.quit {
            self.process_keypress()?;
            self.refresh_screen()?;
        }
        
        // Cleanup terminal on exit
        execute!(io::stdout(), LeaveAlternateScreen)?;
        terminal::disable_raw_mode()?;
        
        Ok(())
    }
    
    fn refresh_screen(&mut self) -> Result<(), Box<dyn Error>> {
        execute!(
            io::stdout(),
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )?;
        
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
        
        self.draw_status_line()?;
        self.draw_message_line()?;
        
        // Position cursor based on mode
        match self.mode {
            Mode::FileTree => {
                if let Some(tree) = &self.file_tree {
                    let tree_cursor_y = tree.cursor.min(self.terminal_height - 3);
                    execute!(io::stdout(), cursor::MoveTo(2, tree_cursor_y as u16))?;
                }
            },
            Mode::Shell => {
                if let Some(buffer) = self.buffers.get(self.active_buffer) {
                    if let Some(shell) = &buffer.shell {
                        let content_y = if self.windows.len() > 1 { 
                            self.windows[self.active_window].y + 1 
                        } else { 
                            0 
                        };
                        
                        let filetree_width = if let Some(tree) = &self.file_tree { 
                            if tree.visible { tree.width + 1 } else { 0 } 
                        } else { 0 };
                        
                        let content_x = if self.windows.len() > 1 { 
                            self.windows[self.active_window].x + filetree_width + 1 
                        } else { 
                            filetree_width 
                        };
                        
                        let shell_lines_count = shell.lines.len();
                        let cursor_pos = shell.cursor_pos + 2; // + 2 for "$ " prefix
                        
                        execute!(io::stdout(), cursor::MoveTo(
                            (content_x + cursor_pos) as u16, 
                            (content_y + shell_lines_count) as u16
                        ))?;
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
    
    fn draw_file_tree(&self) -> Result<(), Box<dyn Error>> {
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
    
    fn draw_window_borders(&self, window: &Window, adjusted_x: usize, is_active: bool) -> Result<(), Box<dyn Error>> {
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
    
    fn draw_window_content(&self, window: &Window, adjusted_x: usize) -> Result<(), Box<dyn Error>> {
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
            if let Some(shell) = &buffer.shell {
                let mut line_counter = 0;
                for (idx, line) in shell.lines.iter().enumerate() {
                    if line_counter >= effective_height {
                        break;
                    }
                    
                    execute!(io::stdout(), cursor::MoveTo(content_x as u16, (content_y + line_counter) as u16))?;
                    
                    if line.len() > effective_width {
                        print!("{}", &line[0..effective_width]);
                    } else {
                        print!("{}", line);
                    }
                    
                    line_counter += 1;
                }
                
                // Draw the current input line
                if line_counter < effective_height {
                    execute!(io::stdout(), cursor::MoveTo(content_x as u16, (content_y + line_counter) as u16))?;
                    if 2 + shell.input_line.len() > effective_width {
                        print!("$ {}", &shell.input_line[0..effective_width-2]);
                    } else {
                        print!("$ {}", shell.input_line);
                    }
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
    
    fn draw_status_line(&self) -> Result<(), Box<dyn Error>> {
        let status = match self.mode {
            Mode::Normal => " NORMAL ",
            Mode::Insert => " INSERT ",
            Mode::Visual => " VISUAL ",
            Mode::Command => " COMMAND ",
            Mode::FileTree => " FILE TREE ",
            Mode::Shell => " SHELL ",
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
    
    fn draw_message_line(&self) -> Result<(), Box<dyn Error>> {
        execute!(
            io::stdout(),
            cursor::MoveTo(0, self.terminal_height as u16 - 1),
            terminal::Clear(ClearType::CurrentLine)
        )?;
        
        if let Mode::Command = self.mode {
            print!(":{}", self.command_line);
        }
        
        Ok(())
    }
    
    fn process_keypress(&mut self) -> Result<(), Box<dyn Error>> {
        if let Event::Key(key_event) = event::read()? {
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
            }
        }
        
        Ok(())
    }
    
    fn process_second_key(&mut self, key: KeyEvent) -> Result<(), Box<dyn Error>> {
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
    
    fn process_normal_mode(&mut self, key: KeyEvent) -> Result<(), Box<dyn Error>> {
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
            _ => {}
        }
        
        Ok(())
    }
    
    fn process_visual_mode(&mut self, key: KeyEvent) -> Result<(), Box<dyn Error>> {
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
    
    fn process_command_mode(&mut self, key: KeyEvent) -> Result<(), Box<dyn Error>> {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                self.execute_command()?;
                self.mode = Mode::Normal;
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
    
    fn process_file_tree_mode(&mut self, key: KeyEvent) -> Result<(), Box<dyn Error>> {
        if let Some(tree) = &mut self.file_tree {
            match key.code {
                KeyCode::Esc => {
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
                    if let Some(path) = tree.get_selected_path() {
                        if path.is_dir() {
                            tree.toggle_expand()?;
                        } else {
                            // Open the selected file
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
                },
                KeyCode::Char('h') => {
                    tree.toggle_expand()?;
                },
                _ => {}
            }
        }
        
        Ok(())
    }
    
    fn save(&mut self) -> Result<(), Box<dyn Error>> {
        if self.is_shell {
            return Err("Cannot save shell buffer".into());
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

    fn from_file(filename: &str) -> Result<Self, Box<dyn Error>> {
        let content = fs::read_to_string(filename)?;
        let lines: Vec<String> = content.lines().map(String::from).collect();
        let lines = if lines.is_empty() { vec![String::new()] } else { lines };
        
        Ok(Self {
            lines,
            filename: Some(filename.to_string()),
            modified: false,
        })
    }

    fn save(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(filename) = &self.filename {
            let content = self.lines.join("\n");
            fs::write(filename, content)?;
            self.modified = false;
            Ok(())
        } else {
            Err("No filename specified".into())
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
    pub fn new(config_path: PathBuf) -> Result<Self, Box<dyn Error>> {
        // Initialize terminal
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        
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
    
    pub fn open_file(&mut self, filename: &str) -> Result<(), Box<dyn Error>> {
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
    
    fn open_shell(&mut self, is_horizontal: bool) -> Result<(), Box<dyn Error>> {
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
    
    fn close_current_buffer(&mut self) -> Result<(), Box<dyn Error>> {
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
    
    fn load_config(&mut self) -> Result<(), Box<dyn Error>> {
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
    
    fn register_api(&mut self) -> Result<(), Box<dyn Error>> {
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
    
    pub fn run(&mut self) -> Result<(), Box<dyn Error>> {
        self.refresh_screen()?;
        
        while !self.quit {
            self.process_keypress()?;
            self.refresh_screen()?;
        }
        
        // Cleanup terminal on exit
        execute!(io::stdout(), LeaveAlternateScreen)?;
        terminal::disable_raw_mode()?;
        
        Ok(())
    }
    
    fn refresh_screen(&mut self) -> Result<(), Box<dyn Error>> {
        execute!(
            io::stdout(),
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )?;
        
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
        
        self.draw_status_line()?;
        self.draw_message_line()?;
        
        // Position cursor based on mode
        match self.mode {
            Mode::FileTree => {
                if let Some(tree) = &self.file_tree {
                    let tree_cursor_y = tree.cursor.min(self.terminal_height - 3);
                    execute!(io::stdout(), cursor::MoveTo(2, tree_cursor_y as u16))?;
                }
            },
            Mode::Shell => {
                if let Some(buffer) = self.buffers.get(self.active_buffer) {
                    if let Some(shell) = &buffer.shell {
                        let content_y = if self.windows.len() > 1 { 
                            self.windows[self.active_window].y + 1 
                        } else { 
                            0 
                        };
                        
                        let filetree_width = if let Some(tree) = &self.file_tree { 
                            if tree.visible { tree.width + 1 } else { 0 } 
                        } else { 0 };
                        
                        let content_x = if self.windows.len() > 1 { 
                            self.windows[self.active_window].x + filetree_width + 1 
                        } else { 
                            filetree_width 
                        };
                        
                        let shell_lines_count = shell.lines.len();
                        let cursor_pos = shell.cursor_pos + 2; // + 2 for "$ " prefix
                        
                        execute!(io::stdout(), cursor::MoveTo(
                            (content_x + cursor_pos) as u16, 
                            (content_y + shell_lines_count) as u16
                        ))?;
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
    
    fn draw_file_tree(&self) -> Result<(), Box<dyn Error>> {
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
    
    fn draw_window_borders(&self, window: &Window, adjusted_x: usize, is_active: bool) -> Result<(), Box<dyn Error>> {
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
    
    fn draw_window_content(&self, window: &Window, adjusted_x: usize) -> Result<(), Box<dyn Error>> {
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
            if let Some(shell) = &buffer.shell {
                let mut line_counter = 0;
                for (idx, line) in shell.lines.iter().enumerate() {
                    if line_counter >= effective_height {
                        break;
                    }
                    
                    execute!(io::stdout(), cursor::MoveTo(content_x as u16, (content_y + line_counter) as u16))?;
                    
                    if line.len() > effective_width {
                        print!("{}", &line[0..effective_width]);
                    } else {
                        print!("{}", line);
                    }
                    
                    line_counter += 1;
                }
                
                // Draw the current input line
                if line_counter < effective_height {
                    execute!(io::stdout(), cursor::MoveTo(content_x as u16, (content_y + line_counter) as u16))?;
                    if 2 + shell.input_line.len() > effective_width {
                        print!("$ {}", &shell.input_line[0..effective_width-2]);
                    } else {
                        print!("$ {}", shell.input_line);
                    }
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
    
    fn draw_status_line(&self) -> Result<(), Box<dyn Error>> {
        let status = match self.mode {
            Mode::Normal => " NORMAL ",
            Mode::Insert => " INSERT ",
            Mode::Visual => " VISUAL ",
            Mode::Command => " COMMAND ",
            Mode::FileTree => " FILE TREE ",
            Mode::Shell => " SHELL ",
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
    
    fn draw_message_line(&self) -> Result<(), Box<dyn