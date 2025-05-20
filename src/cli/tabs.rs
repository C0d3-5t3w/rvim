use std::collections::HashMap;
use crate::error::{Error, Result};
use crate::cli::buffer::Buffer;

pub struct Tab {
    id: usize,
    name: String,
    buffer: Buffer,
}

pub struct TabManager {
    tabs: Vec<Tab>,
    current_tab: usize,
    tab_map: HashMap<String, usize>,
    next_id: usize,
}

impl TabManager {
    pub fn new() -> Self {
        TabManager {
            tabs: Vec::new(),
            current_tab: 0,
            tab_map: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn create_tab(&mut self, name: String, buffer: Buffer) -> Result<usize> {
        if self.tab_map.contains_key(&name) {
            return Err(Error::TabExists(name));
        }

        let id = self.next_id;
        self.next_id += 1;

        let tab = Tab { id, name: name.clone(), buffer };
        self.tabs.push(tab);
        self.tab_map.insert(name, id);
        Ok(id)
    }

    pub fn switch_to_next_tab(&mut self) -> Result<()> {
        if self.tabs.is_empty() {
            return Err(Error::TabError("No tabs available".to_string()));
        }
        self.current_tab = (self.current_tab + 1) % self.tabs.len();
        Ok(())
    }

    pub fn switch_to_prev_tab(&mut self) -> Result<()> {
        if self.tabs.is_empty() {
            return Err(Error::TabError("No tabs available".to_string()));
        }
        self.current_tab = if self.current_tab == 0 {
            self.tabs.len() - 1
        } else {
            self.current_tab - 1
        };
        Ok(())
    }

    pub fn switch_to_tab(&mut self, idx: usize) -> Result<()> {
        if idx < self.tabs.len() {
            self.current_tab = idx;
            Ok(())
        } else {
            Err(Error::TabNotFound(idx))
        }
    }

    pub fn current_buffer(&self) -> Result<&Buffer> {
        self.tabs.get(self.current_tab)
            .map(|tab| &tab.buffer)
            .ok_or_else(|| Error::TabError("No active tab".to_string()))
    }

    pub fn current_buffer_mut(&mut self) -> Result<&mut Buffer> {
        self.tabs.get_mut(self.current_tab)
            .map(|tab| &mut tab.buffer)
            .ok_or_else(|| Error::TabError("No active tab".to_string()))
    }

    pub fn current_tab(&self) -> usize {
        self.current_tab
    }

    pub fn get_current_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.current_tab)
    }

    pub fn get_current_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.current_tab)
    }

    pub fn tab_list(&self) -> Vec<(usize, &str)> {
        self.tabs.iter()
            .map(|tab| (tab.id, tab.name.as_str()))
            .collect()
    }
}
