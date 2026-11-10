//! Reading a text field back is implemented per platform; this one has none yet.

pub struct Field;

pub fn open(_pid: u32, _tail_units: usize) -> Result<(Field, String), String> {
    Err("unsupported-platform".into())
}

pub fn before_cursor(_pid: u32, _max_units: usize) -> Option<String> {
    None
}

impl Field {
    pub fn route(&self) -> &'static str {
        ""
    }
    pub fn role(&self) -> &str {
        ""
    }
    pub fn units(&self) -> usize {
        0
    }
    pub fn is_focused(&self) -> bool {
        false
    }
    pub fn read(&self, _tail_units: usize) -> Option<String> {
        None
    }
    pub fn close(self) {}
}
