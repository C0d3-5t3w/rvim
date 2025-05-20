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

// Editor modes
enum Mode {
    Normal,
    Insert,
    Visual,
    Command,
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
}

impl Editor {
    pub fn new(config_path: PathBuf) -> Result<Self, Box<dyn Error>> {
        // Initialize terminal
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        
        let (cols, rows) = terminal::size()?;
        
        // Initialize Lua
        let lua = Lua::new();
        
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
        };
        
        // Load Lua configuration
        editor.load_config()?;
        
        Ok(editor)
    }
    
    pub fn open_file(&mut self, filename: &str) -> Result<(), Box<dyn Error>> {
        self.document = Document::from_file(filename)?;
        self.cursor_x = 0;
        self.cursor_y = 0;
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
        
        self.draw_rows()?;
        self.draw_status_line()?;
        self.draw_message_line()?;
        
        // Position cursor
        let screen_x = self.cursor_x.saturating_sub(self.offset_x);
        let screen_y = self.cursor_y.saturating_sub(self.offset_y);
        execute!(io::stdout(), cursor::MoveTo(screen_x as u16, screen_y as u16))?;
        
        io::stdout().flush()?;
        
        Ok(())
    }
    
    fn draw_rows(&mut self) -> Result<(), Box<dyn Error>> {
        for y in 0..self.terminal_height.saturating_sub(2) {
            let file_row = y + self.offset_y;
            
            if file_row >= self.document.lines.len() {
                if y == self.terminal_height / 3 && self.document.lines.len() == 1 && self.document.lines[0].is_empty() {
                    let welcome = format!("RVim - Version 0.1.0");
                    let padding = (self.terminal_width - welcome.len()) / 2;
                    print!("~{}{}", " ".repeat(padding.saturating_sub(1)), welcome);
                } else {
                    print!("~");
                }
            } else {
                let line = &self.document.lines[file_row];
                let start = self.offset_x.min(line.len());
                let end = (self.offset_x + self.terminal_width).min(line.len());
                if start < end {
                    print!("{}", &line[start..end]);
                }
            }
            print!("\r\n");
        }
        
        Ok(())
    }
    
    fn draw_status_line(&self) -> Result<(), Box<dyn Error>> {
        let status = match self.mode {
            Mode::Normal => " NORMAL ",
            Mode::Insert => " INSERT ",
            Mode::Visual => " VISUAL ",
            Mode::Command => " COMMAND ",
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
                Mode::Normal => self.process_normal_mode(key_event)?,
                Mode::Insert => self.process_insert_mode(key_event)?,
                Mode::Visual => self.process_visual_mode(key_event)?,
                Mode::Command => self.process_command_mode(key_event)?,
            }
        }
        
        Ok(())
    }
    
    fn process_normal_mode(&mut self, key: KeyEvent) -> Result<(), Box<dyn Error>> {
        match key.code {
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
}
