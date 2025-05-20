use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use log::info;

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
}

impl FileTree {
    pub fn new(path: &Path) -> Result<Self, Box<dyn Error>> {
        let root = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        };

        info!("Initializing file tree at: {:?}", root);

        let mut tree = Self {
            root: root.clone(),
            entries: vec![],
            cursor: 0,
            visible: false,
            width: 30, // Default width
        };

        tree.refresh()?;
        Ok(tree)
    }

    pub fn refresh(&mut self) -> Result<(), Box<dyn Error>> {
        self.entries.clear();
        self.load_entries(&self.root.clone(), 0)?;
        Ok(())
    }

    fn load_entries(&mut self, dir: &Path, level: usize) -> Result<(), Box<dyn Error>> {
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

    pub fn toggle_expand(&mut self) -> Result<(), Box<dyn Error>> {
        if self.entries.is_empty() {
            return Ok(());
        }

        let cursor = self.cursor;
        if self.entries[cursor].is_dir {
            // Toggle expansion
            let is_expanded = self.entries[cursor].is_expanded;
            self.entries[cursor].is_expanded = !is_expanded;
            
            // Reload the tree to show expanded directories
            self.refresh()?;
        }
        
        Ok(())
    }

    pub fn get_selected_path(&self) -> Option<PathBuf> {
        if self.entries.is_empty() {
            return None;
        }
        
        Some(self.entries[self.cursor].path.clone())
    }
}
