use std::error::Error as StdError;
use std::fs;
use std::path::{Path, PathBuf};
use log::info;
use log::error;
use crate::error::{Error, Result};
use notify::{Watcher, RecursiveMode, RecommendedWatcher};
use std::sync::mpsc::{channel, Receiver};
use std::process::Command;
use std::collections::HashMap;

pub struct FileTreeEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_expanded: bool,
    pub level: usize,
    pub children: Vec<FileTreeEntry>,
}

pub struct FileTree {
    pub root: PathBuf,
    pub entries: Vec<FileTreeEntry>,
    pub cursor: usize,
    pub visible: bool,
    pub width: usize,
    watcher: Option<RecommendedWatcher>,
    fs_events: Option<Receiver<notify::Result<notify::Event>>>,
    git_statuses: HashMap<PathBuf, GitStatus>,
}

#[derive(Clone, PartialEq)]
enum GitStatus {
    Modified,
    Added,
    Deleted,
    Untracked,
    Clean,
}

impl From<notify::Error> for Error {
    fn from(err: notify::Error) -> Self {
        Error::Message(format!("File watch error: {}", err))
    }
}

impl FileTree {
    pub fn new(path: &Path) -> Result<Self> {
        let root = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        };

        info!("Initializing file tree at: {:?}", root);

        // Setup file watching
        let (tx, rx) = channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })?;
        watcher.watch(path, RecursiveMode::Recursive)?;

        let mut tree = Self {
            root: root.clone(),
            entries: vec![],
            cursor: 0,
            visible: false,
            width: 30, // Default width
            watcher: Some(watcher),
            fs_events: Some(rx),
            git_statuses: HashMap::new(),
        };

        tree.refresh()?;
        tree.update_git_status()?;

        Ok(tree)
    }

    pub fn refresh(&mut self) -> Result<()> {
        self.entries.clear();
        self.load_entries(&self.root.clone(), 0)?;
        Ok(())
    }

    fn load_entries(&mut self, dir: &Path, level: usize) -> Result<()> {
        let entries = fs::read_dir(dir)?;

        // First collect all entries to sort them
        let mut dirs = Vec::new();
        let mut files = Vec::new();

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            
            // Skip hidden files/directories (optional)
            if name.starts_with('.') && name != ".." && name != "." {
                continue;
            }

            let is_dir = path.is_dir();
            if is_dir {
                dirs.push(FileTreeEntry {
                    name,
                    path,
                    is_dir,
                    is_expanded: false,
                    level,
                    children: vec![],
                });
            } else {
                files.push(FileTreeEntry {
                    name,
                    path,
                    is_dir,
                    is_expanded: false,
                    level,
                    children: vec![],
                });
            }
        }

        // Sort directories and files alphabetically
        dirs.sort_by(|a, b| a.name.cmp(&b.name));
        files.sort_by(|a, b| a.name.cmp(&b.name));

        // Add directories first, then files
        self.entries.extend(dirs);
        self.entries.extend(files);

        Ok(())
    }

    pub fn toggle_visible(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            info!("File browser opened");
        } else {
            info!("File browser closed");
        }
    }

    pub fn move_cursor_up(&mut self) {
        if !self.entries.is_empty() && self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_cursor_down(&mut self) {
        if !self.entries.is_empty() && self.cursor < self.entries.len() - 1 {
            self.cursor += 1;
        }
    }
    
    pub fn is_directory_expanded(&self, path: PathBuf) -> bool {
        for entry in &self.entries {
            if entry.path == path && entry.is_dir {
                return entry.is_expanded;
            }
        }
        false
    }
    
    pub fn move_to_parent(&mut self) -> Result<()> {
        if self.cursor < self.entries.len() {
            let current_entry = &self.entries[self.cursor];
            
            // If it's already at level 0, can't go to parent
            if current_entry.level == 0 {
                return Ok(());
            }
            
            // Find the parent entry (entry with level = current level - 1)
            let target_level = current_entry.level - 1;
            let mut parent_idx = self.cursor;
            
            // Go backwards to find the parent
            while parent_idx > 0 {
                parent_idx -= 1;
                if self.entries[parent_idx].level == target_level {
                    // Found parent, move cursor to it
                    self.cursor = parent_idx;
                    break;
                }
            }
        }
        
        Ok(())
    }
    
    pub fn toggle_expand(&mut self) -> Result<()> {
        if self.entries.is_empty() || self.cursor >= self.entries.len() {
            return Ok(());
        }
        
        if self.entries[self.cursor].is_dir {
            let path = self.entries[self.cursor].path.clone();
            let current_level = self.entries[self.cursor].level;
            
            // Toggle expanded state
            self.entries[self.cursor].is_expanded = !self.entries[self.cursor].is_expanded;
            
            if self.entries[self.cursor].is_expanded {
                // If now expanded, load subdirectories and files
                let mut new_entries = Vec::new();
                self.load_directory_entries(&path, current_level + 1, &mut new_entries)?;
                
                // Insert the new entries after the current entry
                if !new_entries.is_empty() {
                    let insert_position = self.cursor + 1;
                    for (i, entry) in new_entries.into_iter().enumerate() {
                        self.entries.insert(insert_position + i, entry);
                    }
                }
            } else {
                // If now collapsed, remove all entries that are children of this directory
                let cursor_idx = self.cursor;
                
                // Define a closure to check if an entry is a child of current dir
                let mut remove_indices = Vec::new();
                for i in (cursor_idx + 1)..self.entries.len() {
                    if self.entries[i].level > current_level {
                        remove_indices.push(i);
                    } else {
                        break; // Stop when we reach an entry at the same or higher level
                    }
                }
                
                // Remove entries in reverse order to maintain correct indices
                for idx in remove_indices.into_iter().rev() {
                    self.entries.remove(idx);
                }
            }
        }
        
        Ok(())
    }
    
    fn load_directory_entries(&self, dir: &Path, level: usize, entries: &mut Vec<FileTreeEntry>) -> Result<()> {
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            
            // Skip hidden files/directories (optional)
            if name.starts_with('.') && name != ".." && name != "." {
                continue;
            }
            
            let is_dir = path.is_dir();
            if is_dir {
                dirs.push(FileTreeEntry {
                    name,
                    path,
                    is_dir,
                    is_expanded: false,
                    level,
                    children: vec![],
                });
            } else {
                files.push(FileTreeEntry {
                    name,
                    path,
                    is_dir,
                    is_expanded: false,
                    level,
                    children: vec![],
                });
            }
        }
        
        // Sort alphabetically
        dirs.sort_by(|a, b| a.name.cmp(&b.name));
        files.sort_by(|a, b| a.name.cmp(&b.name));
        
        // Add directories first, then files
        entries.extend(dirs);
        entries.extend(files);
        
        Ok(())
    }
    
    pub fn get_selected_path(&self) -> Option<PathBuf> {
        if self.entries.is_empty() {
            return None;
        }
        
        Some(self.entries[self.cursor].path.clone())
    }
    
    // Clone event before using it
    pub fn handle_fs_event(&mut self, event: notify::Event) -> Result<()> {
        let event_kind = event.kind;
        let paths = event.paths.clone();
        
        match event_kind {
            notify::EventKind::Create(_) |
            notify::EventKind::Remove(_) |
            notify::EventKind::Modify(_) => {
                self.refresh()?;
                self.update_git_status()?;
            }
            _ => {}
        }
        Ok(())
    }
    
    fn update_git_status(&mut self) -> Result<()> {
        let output = Command::new("git")
            .args(&["status", "--porcelain"])
            .current_dir(&self.root)
            .output()
            .map_err(|e| Error::Message(format!("Git error: {}", e)))?;
            
        if output.status.success() {
            self.git_statuses.clear();
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                if line.len() < 3 { continue; }
                let status = &line[0..2];
                let path = self.root.join(&line[3..]);
                
                let git_status = match status.trim() {
                    "M" => GitStatus::Modified,
                    "A" => GitStatus::Added,
                    "D" => GitStatus::Deleted,
                    "??" => GitStatus::Untracked,
                    _ => GitStatus::Clean,
                };
                
                self.git_statuses.insert(path, git_status);
            }
        }
        Ok(())
    }
    
    pub fn check_file_updates(&mut self) -> Result<()> {
        let mut paths_to_update = Vec::new();
        if let Some(rx) = &self.fs_events {
            while let Ok(event_result) = rx.try_recv() {
                match event_result {
                    Ok(event) => {
                        paths_to_update.push((event.paths.clone(), event.kind));
                    }
                    Err(e) => {
                        return Err(Error::from(e));
                    }
                }
            }
        }
        
        // Process collected events after releasing the borrow on fs_events
        for (paths, kind) in paths_to_update {
            self.handle_fs_event_impl(paths, kind)?;
        }
        
        Ok(())
    }

    // New helper method to handle events without cloning
    fn handle_fs_event_impl(&mut self, paths: Vec<PathBuf>, kind: notify::EventKind) -> Result<()> {
        match kind {
            notify::EventKind::Create(_) |
            notify::EventKind::Remove(_) |
            notify::EventKind::Modify(_) => {
                self.refresh()?;
                self.update_git_status()?;
            }
            _ => {}
        }
        Ok(())
    }
}
