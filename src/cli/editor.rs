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

// Editor modes
#[derive(Clone, Copy, Debug)]
enum Mode {
    Normal,
    Insert,
    Visual,
    Command,
    FileTree,
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
    document: Document,
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
        
        let mut editor = Self {
            document: Document::new(),
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
        self.document = Document::from_file(filename)?;
        self.cursor_x = 0;
        self.cursor_y = 0;
        
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
            _ => {
                if !self.windows.is_empty() {
                    let window = &self.windows[self.active_window];
                    let filetree_width = if let Some(tree) = &self.file_tree { 
                        if tree.visible { tree.width + 1 } else { 0 } 
                    } else { 0 };
                    
                    let screen_x = window.x + filetree_width + window.cursor_x.saturating_sub(window.offset_x);
                    let screen_y = window.y + window.cursor_y.saturating_sub(window.offset_y);
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
        
        for y in 0..effective_height {
            let file_row = y + window.offset_y;
            
            execute!(io::stdout(), cursor::MoveTo(content_x as u16, (content_y + y) as u16))?;
            
            if file_row >= self.document.lines.len() {
                if y == window.height / 3 && self.document.lines.len() == 1 && self.document.lines[0].is_empty() {
                    let welcome = format!("RVim - Version 0.1.0");
                    let padding = (effective_width - welcome.len()) / 2;
                    print!("~{}{}", " ".repeat(padding.saturating_sub(1)), welcome);
                } else {
                    print!("~");
                }
            } else {
                let line = &self.document.lines[file_row];
                let start = window.offset_x.min(line.len());
                let end = (window.offset_x + effective_width).min(line.len());
                if start < end {
                    print!("{}", &line[start..end]);
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
        };
        
        let filename = self.document.filename.clone().unwrap_or_else(|| "[No Name]".into());
        let modified = if self.document.modified { "[+]" } else { "" };
        let status_line = format!(
            "{}{}{} - {}/{}",
            status,
            filename,
            modified,
            self.cursor_y + 1,
            self.document.lines.len()
        );
        
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
                // Vertical split (side by side)
                self.split_window(SplitType::Vertical)?;
            },
            KeyCode::Char('h') => {
                // Horizontal split (one above the other)
                self.split_window(SplitType::Horizontal)?;
            },
            KeyCode::Char('w') => {
                // Cycle through windows
                self.cycle_window();
            },
            KeyCode::Char('q') => {
                // Close the current window
                self.close_window()?;
            },
            _ => {
                // Ignore other keys after space
            }
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
                            self.document = Document::from_file(path.to_str().unwrap())?;
                            self.cursor_x = 0;
                            self.cursor_y = 0;
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
    
    fn execute_command(&mut self) -> Result<(), Box<dyn Error>> {
        match self.command_line.as_str() {
            "w" => {
                if let Err(e) = self.document.save() {
                    self.command_line = format!("Error saving: {}", e);
                } else {
                    self.command_line = "File saved".to_string();
                }
            },
            "q" => self.quit = true,
            "wq" => {
                if let Err(e) = self.document.save() {
                    self.command_line = format!("Error saving: {}", e);
                } else {
                    self.quit = true;
                }
            },
            _ => {
                self.command_line = format!("Unknown command: {}", self.command_line);
            }
        }
        
        Ok(())
    }
    
    fn move_cursor_left(&mut self) {
        if self.cursor_x > 0 {
            self.cursor_x -= 1;
        }
    }
    
    fn move_cursor_right(&mut self) {
        if self.cursor_y < self.document.lines.len() {
            let line_len = self.document.lines[self.cursor_y].len();
            if self.cursor_x < line_len {
                self.cursor_x += 1;
            }
        }
    }
    
    fn move_cursor_up(&mut self) {
        if self.cursor_y > 0 {
            self.cursor_y -= 1;
            let line_len = self.document.lines[self.cursor_y].len();
            if self.cursor_x > line_len {
                self.cursor_x = line_len;
            }
        }
    }
    
    fn move_cursor_down(&mut self) {
        if self.cursor_y < self.document.lines.len() - 1 {
            self.cursor_y += 1;
            let line_len = self.document.lines[self.cursor_y].len();
            if self.cursor_x > line_len {
                self.cursor_x = line_len;
            }
        }
    }
    
    fn split_window(&mut self, split_type: SplitType) -> Result<(), Box<dyn Error>> {
        if self.windows.is_empty() {
            return Ok(());
        }
        
        // Get the current active window
        let current_window = self.windows[self.active_window].clone();
        
        // Clone split_type before passing it to avoid move issues
        let (window1, window2) = current_window.split(&split_type)?;
        
        // Replace the current window with the two new windows
        self.windows.remove(self.active_window);
        self.windows.insert(self.active_window, window1);
        self.windows.insert(self.active_window + 1, window2);
        
        // Set the second window as active
        self.active_window += 1;
        
        info!("Window split: {:?}", split_type);
        
        Ok(())
    }
    
    fn cycle_window(&mut self) {
        if self.windows.len() <= 1 {
            return;
        }
        
        // Deactivate current window
        self.windows[self.active_window].is_active = false;
        
        // Move to next window, wrapping around if necessary
        self.active_window = (self.active_window + 1) % self.windows.len();
        
        // Activate the new window
        self.windows[self.active_window].is_active = true;
        
        info!("Switched to window {}", self.active_window + 1);
    }
    
    fn close_window(&mut self) -> Result<(), Box<dyn Error>> {
        if self.windows.len() <= 1 {
            // Don't close the last window
            return Ok(());
        }
        
        // Remove the current window
        self.windows.remove(self.active_window);
        
        // Adjust the active window index if needed
        if self.active_window >= self.windows.len() {
            self.active_window = self.windows.len() - 1;
        }
        
        // Activate the new current window
        self.windows[self.active_window].is_active = true;
        
        info!("Closed window, now at window {}", self.active_window + 1);
        
        Ok(())
    }
    
    fn process_insert_mode(&mut self, key: KeyEvent) -> Result<(), Box<dyn Error>> {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Char(c) => {
                self.document.insert_char(self.cursor_y, self.cursor_x, c);
                self.move_cursor_right();
            },
            KeyCode::Backspace => {
                if self.cursor_x > 0 {
                    self.move_cursor_left();
                    self.document.delete_char(self.cursor_y, self.cursor_x);
                }
            },
            KeyCode::Enter => {
                // Handle enter in insert mode (split line)
                // This would need more complex implementation for actual split
                let new_line = String::new();
                self.document.lines.insert(self.cursor_y + 1, new_line);
                self.cursor_y += 1;
                self.cursor_x = 0;
            },
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
}
