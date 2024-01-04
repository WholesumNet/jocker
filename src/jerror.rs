use std::fmt;

// A simple error object that includes image-name and job-id
#[derive(Debug)]
pub struct JError {
    pub who: String,
    pub message: String,
}

impl fmt::Display for JError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
            "{}: {}",
            self.who,
            self.message
        )
    }
}
