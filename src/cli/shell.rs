use crate::error::{Error, Result};
use std::io::{self, Write, BufReader, BufRead};
use std::process::{Command, Stdio, Child, ChildStdin, ChildStdout, ChildStderr};
use std::thread;
use std::sync::mpsc::{self, Sender, Receiver, TryRecvError};
use std::time::Duration;
use log::info;
use std::env;
use std::sync::{Arc, Mutex};

enum ShellOutput {
    Line(String),
    Terminated,
}

#[derive(Clone)] // Add this line before Shell struct definition
pub struct Shell {
    pub lines: Vec<String>,
    pub input_line: String,
    pub cursor_pos: usize,
    pub is_horizontal: bool, // For RVim's layout, not the shell's behavior
    pub running: bool,       // RVim's flag to indicate if this shell mode is active
    pub command_history: Vec<String>,
    pub history_position: usize,

    child: Arc<Mutex<Option<Child>>>,
    child_stdin: Arc<Mutex<Option<ChildStdin>>>,
    output_receiver: Arc<Mutex<Option<Receiver<ShellOutput>>>>,
    // Keep track of the reader threads to join them on drop
    reader_thread_handles: Arc<Mutex<Vec<thread::JoinHandle<()>>>>,
}

impl Shell {
    pub fn new(is_horizontal: bool) -> Self {
        info!("Creating new interactive shell: {}", if is_horizontal { "horizontal" } else { "vertical" });
        let mut shell_instance = Self {
            lines: vec!["RVim Interactive Shell".to_string(), "Spawning system shell...".to_string()],
            input_line: String::new(),
            cursor_pos: 0,
            is_horizontal,
            running: true,
            command_history: Vec::new(),
            history_position: 0,
            child: Arc::new(Mutex::new(None)),
            child_stdin: Arc::new(Mutex::new(None)),
            output_receiver: Arc::new(Mutex::new(None)),
            reader_thread_handles: Arc::new(Mutex::new(Vec::new())),
        };

        if let Err(e) = shell_instance.spawn_system_shell() {
            shell_instance.lines.push(format!("Error spawning shell: {}", e));
            shell_instance.running = false; // Can't run if spawn failed
        } else {
            shell_instance.lines.push("System shell spawned. Type 'exit' in the shell to close it.".to_string());
        }
        shell_instance.lines.push("".to_string()); // Initial empty line for prompt

        shell_instance
    }

    fn spawn_system_shell(&mut self) -> Result<()> {
        let shell_cmd = env::var("SHELL").unwrap_or_else(|_| {
            if cfg!(windows) { "cmd.exe".to_string() } else { "sh".to_string() }
        });

        info!("Spawning shell: {}", shell_cmd);

        let mut child_process = Command::new(&shell_cmd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::ShellSpawnError(format!("Failed to spawn shell: {}", e)))?;

        let child_stdout = child_process.stdout.take()
            .ok_or_else(|| Error::ShellSpawnError("Failed to capture stdout".to_string()))?;
        let child_stderr = child_process.stderr.take()
            .ok_or_else(|| Error::ShellSpawnError("Failed to capture stderr".to_string()))?;
        
        {
            let mut child_lock = self.child.lock().unwrap();
            *child_lock = Some(child_process);
        }

        let (tx, rx) = mpsc::channel();
        {
            let mut receiver_lock = self.output_receiver.lock().unwrap();
            *receiver_lock = Some(rx);
        }

        let stdout_tx = tx.clone();
        let stdout_handle = thread::spawn(move || {
            let reader = BufReader::new(child_stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if stdout_tx.send(ShellOutput::Line(l)).is_err() {
                            break; // Receiver dropped
                        }
                    }
                    Err(_) => break, // Stream error
                }
            }
            let _ = stdout_tx.send(ShellOutput::Terminated); // Signal stdout termination
        });
        self.reader_thread_handles.lock().unwrap().push(stdout_handle);

        let stderr_tx = tx;
        let stderr_handle = thread::spawn(move || {
            let reader = BufReader::new(child_stderr);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if stderr_tx.send(ShellOutput::Line(l)).is_err() {
                            break; // Receiver dropped
                        }
                    }
                    Err(_) => break, // Stream error
                }
            }
            // Note: We don't send Terminated from stderr to avoid duplicate signals
            // if stdout also terminates. The main child process termination check is more reliable.
        });
        self.reader_thread_handles.lock().unwrap().push(stderr_handle);
        
        Ok(())
    }
    
    pub fn poll_output(&mut self) {
        if let Ok(rx_guard) = self.output_receiver.lock() {
            if let Some(rx) = &*rx_guard {
                loop {
                    match rx.try_recv() {
                        Ok(ShellOutput::Line(line)) => {
                            self.lines.push(line);
                        }
                        Ok(ShellOutput::Terminated) => {
                            info!("A shell output stream terminated.");
                        }
                        Err(TryRecvError::Empty) => {
                            break; 
                        }
                        Err(TryRecvError::Disconnected) => {
                            info!("Shell output channel disconnected. Shell likely terminated.");
                            self.running = false;
                            {
                                let mut receiver_lock = self.output_receiver.lock().unwrap();
                                *receiver_lock = None; 
                            }
                            break;
                        }
                    }
                }
            }
        }

        if self.running { // Only check if we think it's running
            if let Some(child) = &mut *self.child.lock().unwrap() {
                match child.try_wait() {
                    Ok(Some(status)) => { 
                        info!("Shell process exited with status: {}", status);
                        self.running = false;
                        {
                            let mut child_lock = self.child.lock().unwrap();
                            *child_lock = None; 
                        }
                    }
                    Ok(None) => { 
                    }
                    Err(e) => { 
                        info!("Error waiting for shell process: {}", e);
                        self.running = false;
                        {
                            let mut child_lock = self.child.lock().unwrap();
                            *child_lock = None;
                        }
                    }
                }
            } else if self.output_receiver.lock().unwrap().is_none() { 
                 self.running = false;
            }
        }
    }


    pub fn execute_command(&mut self) -> Result<()> {
        self.poll_output(); 

        let command_trimmed = self.input_line.trim();

        // RVim's own "exit" command to leave shell mode in RVim
        // This will also attempt to tell the underlying system shell to exit.
        if command_trimmed == "exit" {
             info!("RVim 'exit' command detected. Attempting to close system shell and exit RVim shell mode.");
             if let Some(stdin) = &mut *self.child_stdin.lock().unwrap() {
                 // Send "exit" command to the actual shell
                 if writeln!(stdin, "exit").is_ok() {
                     let _ = stdin.flush();
                     info!("Sent 'exit' command to system shell.");
                 } else {
                     info!("Failed to send 'exit' to system shell stdin. It might already be closed.");
                 }
             }
             // Even if sending "exit" fails, we mark RVim's shell as not running.
             // The Drop impl or subsequent poll_output will handle actual child process termination if needed.
             self.running = false; 
             self.input_line.clear();
             self.cursor_pos = 0;
             return Ok(());
        }


        if let Some(stdin) = &mut *self.child_stdin.lock().unwrap() {
            if !self.input_line.is_empty() {
                if !self.input_line.trim().is_empty() {
                    self.command_history.push(self.input_line.clone());
                    self.history_position = self.command_history.len();
                }

                writeln!(stdin, "{}", self.input_line)
                    .map_err(|e| Error::ShellInputError(format!("Failed to write to shell: {}", e)))?;
                stdin.flush()
                    .map_err(|e| Error::ShellInputError(format!("Failed to flush shell stdin: {}", e)))?;
            } else {
                writeln!(stdin, "")
                    .map_err(|e| Error::ShellInputError(format!("Failed to write newline: {}", e)))?;
                stdin.flush()
                    .map_err(|e| Error::ShellInputError(format!("Failed to flush shell stdin: {}", e)))?;
            }
        } else {
            self.lines.push("Shell not running or stdin unavailable.".to_string());
            self.running = false;
        }
        
        self.input_line.clear();
        self.cursor_pos = 0;
        
        // Give a short moment for the shell to process and output a prompt
        // This is a bit of a hack; ideally, prompt detection would be more robust.
        thread::sleep(Duration::from_millis(50)); 
        self.poll_output(); // Poll again to catch immediate output like a new prompt
        
        Ok(())
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

impl Drop for Shell {
    fn drop(&mut self) {
        info!("Dropping Shell instance.");
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let child_id = child.id();
            info!("Terminating child shell process (PID: {}).", child_id);

            drop(self.child_stdin.lock().unwrap().take());

            match child.try_wait() {
                Ok(Some(_)) => { 
                    info!("Child shell process (PID: {}) already exited.", child_id);
                }
                Ok(None) => { 
                    info!("Child shell process (PID: {}) still running. Attempting to kill.", child_id);
                    if let Err(e) = child.kill() {
                        info!("Failed to kill child shell process (PID: {}): {}", child_id, e);
                    } else {
                        // Replace wait_timeout with wait since it's not available
                        match child.wait() {
                            Ok(status) => info!("Killed child shell (PID: {}) exited with status: {}", child_id, status),
                            Err(e) => info!("Error waiting for killed child shell (PID: {}): {}", child_id, e),
                        }
                    }
                }
                Err(e) => { 
                    info!("Error checking child shell process (PID: {}) status during drop: {}", child_id, e);
                }
            }
        }
        // Join reader threads
        while let Some(handle) = self.reader_thread_handles.lock().unwrap().pop() {
            if let Err(e) = handle.join() {
                info!("Error joining reader thread: {:?}", e);
            }
        }
    }
}
