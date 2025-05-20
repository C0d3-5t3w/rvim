use std::error::Error;
use std::path::PathBuf;

#[derive(Clone, PartialEq, Debug)]
pub enum SplitType {
    Horizontal,
    Vertical,
}

#[derive(Clone)]
pub struct Window {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub offset_x: usize,
    pub offset_y: usize,
    pub file_path: Option<PathBuf>,
    pub is_active: bool,
}

impl Window {
    pub fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Self {
            x,
            y,
            width,
            height,
            cursor_x: 0,
            cursor_y: 0,
            offset_x: 0,
            offset_y: 0,
            file_path: None,
            is_active: true,
        }
    }

    pub fn split(&self, split_type: &SplitType) -> Result<(Window, Window), Box<dyn Error>> {
        match split_type {
            SplitType::Horizontal => {
                // Split window horizontally (one above, one below)
                let top_height = self.height / 2;
                let bottom_height = self.height - top_height;
                
                let top = Window::new(self.x, self.y, self.width, top_height);
                let mut bottom = Window::new(self.x, self.y + top_height, self.width, bottom_height);
                
                // Copy current window's file path to both windows
                let file_path = self.file_path.clone();
                bottom.file_path = file_path;
                
                Ok((top, bottom))
            },
            SplitType::Vertical => {
                // Split window vertically (one left, one right)
                let left_width = self.width / 2;
                let right_width = self.width - left_width;
                
                let left = Window::new(self.x, self.y, left_width, self.height);
                let mut right = Window::new(self.x + left_width, self.y, right_width, self.height);
                
                // Copy current window's file path to both windows
                let file_path = self.file_path.clone();
                right.file_path = file_path;
                
                Ok((left, right))
            }
        }
    }
}
