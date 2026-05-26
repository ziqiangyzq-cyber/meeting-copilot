pub mod engine;
pub mod prompt;

pub use engine::{SuggestionEngine, TriggerType};
pub use prompt::MeetingMeta;

#[cfg(test)]
mod tests;
