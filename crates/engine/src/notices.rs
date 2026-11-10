//! Messages for the listener: results of jobs, problems worth knowing about.

/// How much a notice matters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

/// A message to show once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub level: NoticeLevel,
    pub text: String,
}

impl Notice {
    pub fn info(text: impl Into<String>) -> Self {
        Self { level: NoticeLevel::Info, text: text.into() }
    }

    pub fn warning(text: impl Into<String>) -> Self {
        Self { level: NoticeLevel::Warning, text: text.into() }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self { level: NoticeLevel::Error, text: text.into() }
    }
}
