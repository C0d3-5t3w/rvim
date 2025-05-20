use crate::cli::shell::Shell;
use crate::error::{Error, Result};
use crate::lsp::{self, get_language_id_from_extension, get_language};  // Add explicit imports
use ropey::Rope;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tree_sitter::{Language, Parser as TsParser, Tree};

// Add error conversion for tree-sitter language errors
impl From<tree_sitter::LanguageError> for Error {
    fn from(err: tree_sitter::LanguageError) -> Self {
        Error::Message(format!("Tree-sitter language error: {}", err))
    }
}

#[derive(Clone)]
pub struct Buffer {
    pub document: Document,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub offset_x: usize,
    pub offset_y: usize,
    pub is_shell: bool,
    pub shell: Option<Shell>,
    pub filename: Option<String>,
    parser: Option<Arc<TsParser>>, // Wrap Parser in Arc for Clone
    tree: Option<Tree>,
    language: Option<Language>,
}

#[derive(Clone)]
pub struct Document {
    pub rope: Rope,  // Use ropey's Rope for efficient text storage
    pub lines: Vec<String>, // Cache for line display
    pub filename: Option<String>,
    pub modified: bool,
    pub undo_tree: UndoTree,
}

#[derive(Clone)]
struct UndoTree {
    // Add fields for undo/redo functionality
    history: Vec<(usize, String)>, // (position, content)
    current: usize,
}

impl UndoTree {
    fn new() -> Self {
        Self {
            history: Vec::new(),
            current: 0,
        }
    }
}

impl Buffer {
    pub fn new() -> Self {
        let parser = TsParser::new();
        Self {
            document: Document::new(),
            cursor_x: 0,
            cursor_y: 0,
            offset_x: 0,
            offset_y: 0,
            is_shell: false,
            shell: None,
            filename: None,
            parser: Some(Arc::new(parser)),
            tree: None,
            language: None,
        }
    }

    pub fn from_file(filename: &str) -> Result<Self> {
        let content = fs::read_to_string(filename)
            .map_err(|e| Error::Io(e))?;

        let mut parser = TsParser::new();
        let mut buffer = Self {
            document: Document::from_file(filename)?,
            cursor_x: 0,
            cursor_y: 0,
            offset_x: 0,
            offset_y: 0,
            is_shell: false,
            shell: None,
            filename: Some(filename.to_string()),
            parser: Some(Arc::new(parser)),
            tree: None,
            language: None,
        };

        // Initialize language if available
        let ext = Path::new(filename)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");

        if let Some(lang_id) = get_language_id_from_extension(ext) {
            if let Some(lang) = get_language(lang_id) {
                let mut new_parser = TsParser::new();
                new_parser.set_language(lang)?;
                buffer.parser = Some(Arc::new(new_parser));
                buffer.language = Some(lang);
            }
        }

        Ok(buffer)
    }

    pub fn from_shell(is_horizontal: bool) -> Self {
        Self {
            document: Document::new(),
            cursor_x: 0,
            cursor_y: 0,
            offset_x: 0,
            offset_y: 0,
            is_shell: true,
            shell: Some(Shell::new(is_horizontal)),
            filename: None,
            parser: None,
            tree: None,
            language: None,
        }
    }

    pub fn save(&mut self) -> Result<()> {
        if self.is_shell {
            return Err(Error::Message("Cannot save shell buffer".into()));
        }
        self.document.save()
    }
    
    pub fn set_language(&mut self, lang: Language) -> Result<()> {
        // Create a new parser since we can't modify through Arc
        let mut new_parser = TsParser::new();
        new_parser.set_language(lang)?;
        self.parser = Some(Arc::new(new_parser));
        self.language = Some(lang);
        self.update_syntax_tree()?;
        Ok(())
    }
    
    fn update_syntax_tree(&mut self) -> Result<()> {
        if let (Some(_), Some(_)) = (&self.parser, &self.language) {
            let text = self.document.rope.to_string();
            // Create a new parser instance since we can't mutably borrow from Arc
            let mut parser_instance = TsParser::new();
            if let Some(lang) = &self.language {
                parser_instance.set_language(*lang)?;
                if let Some(tree) = parser_instance.parse(&text, None) {
                    self.tree = Some(tree);
                }
            }
        }
        Ok(())
    }
}

impl Document {
    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
            lines: vec![String::new()],
            filename: None,
            modified: false,
            undo_tree: UndoTree::new(),
        }
    }

    pub fn from_file(filename: &str) -> Result<Self> {
        let content = fs::read_to_string(filename)
            .map_err(|e| Error::Io(e))?;
            
        Ok(Self {
            rope: Rope::from_str(&content),
            lines: content.lines().map(String::from).collect(),
            filename: Some(filename.to_string()),
            modified: false,
            undo_tree: UndoTree::new(),
        })
    }

    pub fn save(&mut self) -> Result<()> {
        if let Some(filename) = &self.filename {
            let content = self.lines.join("\n");
            fs::write(filename, content)
                .map_err(|e| Error::Io(e))?;
            self.modified = false;
            Ok(())
        } else {
            Err(Error::Message("No filename specified".into()))
        }
    }

    pub fn insert_char(&mut self, row: usize, col: usize, c: char) {
        if row >= self.lines.len() {
            return;
        }
        
        let line = &mut self.lines[row];
        if col > line.len() {
            line.push(c);
        } else {
            line.insert(col, c);
        }
        
        // Update rope
        let pos = self.get_char_position(row, col);
        self.rope.insert_char(pos, c);
        self.modified = true;
    }

    pub fn delete_char(&mut self, row: usize, col: usize) -> bool {
        if row >= self.lines.len() {
            return false;
        }
        
        let line = &mut self.lines[row];
        if col < line.len() {
            line.remove(col);
            // Update rope
            let pos = self.get_char_position(row, col);
            self.rope.remove(pos..pos+1);
            self.modified = true;
            true
        } else {
            false
        }
    }

    // Helper method to convert row/col to rope position
    fn get_char_position(&self, row: usize, col: usize) -> usize {
        let mut pos = 0;
        for i in 0..row {
            pos += self.lines[i].len() + 1; // +1 for newline
        }
        pos + col
    }
}
