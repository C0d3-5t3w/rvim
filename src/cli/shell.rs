use std::error::Error;
use std::io::{self, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::sync::mpsc;
use std::time::Duration;
use log::info;

pub struct Shell {
    pub lines: Vec<String>,
    pub input_line: String,
    pub cursor_pos: usize,
    pub is_horizontal: bool,
    pub running: bool,
    pub command_history: Vec<String>,
    pub history_position: usize,
}

impl Shell {
    pub fn new(is_horizontal: bool) -> Self {
        info!("Creating new shell: {}", if is_horizontal { "horizontal" } else { "vertical" });
        Self {
            lines: vec!["Welcome to RVim Shell".to_string(), "Type commands and press Enter to execute".to_string(), "$ ".to_string()],
            input_line: String::new(),
            cursor_pos: 0,
            is_horizontal,
            running: true,
            command_history: Vec::new(),
            history_position: 0,
        }
    }

    pub fn execute_command(&mut self) -> Result<(), Box<dyn Error>> {
        if self.input_line.trim().is_empty() {
            self.lines.push("$ ".to_string());
            return Ok(());
        }

        let command = self.input_line.clone();
        self.lines.push(format!("$ {}", command));
        
        // Add to history
        self.command_history.push(command.clone());
        self.history_position = self.command_history.len();
        
        // Execute the command
        match self.run_command(&command) {
            Ok(output) => {
                for line in output.lines() {
                    self.lines.push(line.to_string());
                }
            },
            Err(e) => {
                self.lines.push(format!("Error: {}", e));
            }
        }
        
        self.lines.push("$ ".to_string());
        self.input_line.clear();
        self.cursor_pos = 0;
        
        Ok(())
    }

    fn run_command(&self, command: &str) -> Result<String, Box<dyn Error>> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(String::new());
        }

        let program = parts[0];
        let args = &parts[1..];

        #[cfg(target_os = "windows")]
        let output = Command::new("cmd")
            .args(&["/C", program])
            .args(args)
            .output()?;

        #[cfg(not(target_os = "windows"))]
        let output = Command::new(program)
            .args(args)
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !stderr.is_empty() {
            Ok(format!("{}{}", stdout, stderr))
        } else {
            Ok(stdout)
        }
    }

    pub fn input_char(&mut self, c: char) {
        if self.cursor_pos == self.input_line.len() {
            self.input_line.push(c);
        } else {
            self.input_line.insert(self.cursor_pos, c);
        }
        self.cursor_pos += 1;
    }

    pub fn input_backspace(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.input_line.remove(self.cursor_pos);
        }
    }

    pub fn input_delete(&mut self) {
        if self.cursor_pos < self.input_line.len() {
            self.input_line.remove(self.cursor_pos);
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_pos < self.input_line.len() {
            self.cursor_pos += 1;
        }
    }

    pub fn history_up(&mut self) {
        if !self.command_history.is_empty() && self.history_position > 0 {
            self.history_position -= 1;
            self.input_line = self.command_history[self.history_position].clone();
            self.cursor_pos = self.input_line.len();
        }
    }

    pub fn history_down(&mut self) {
        if !self.command_history.is_empty() && self.history_position < self.command_history.len() - 1 {
            self.history_position += 1;
            self.input_line = self.command_history[self.history_position].clone();
            self.cursor_pos = self.input_line.len();
        } else if self.history_position == self.command_history.len() - 1 {
            self.history_position = self.command_history.len();
            self.input_line.clear();
            self.cursor_pos = 0;
        }
    }
}
