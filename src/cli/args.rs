use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "rust-anki", about = "Vocabulary study CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Import vocabulary data.
    Upload {
        #[command(subcommand)]
        target: UploadTarget,
    },
    /// Study vocabulary.
    Study {
        #[command(subcommand)]
        target: StudyTarget,
    },
    /// Edit an existing vocabulary entry.
    Edit {
        #[command(subcommand)]
        target: EditTarget,
    },
    /// Delete a vocabulary entry and its reviews.
    Delete {
        #[command(subcommand)]
        target: DeleteTarget,
    },
}

#[derive(Debug, Subcommand)]
pub enum DeleteTarget {
    /// Delete a vocabulary entry by its study word.
    Vocabulary { word_study: String },
}

#[derive(Debug, Subcommand)]
pub enum EditTarget {
    /// Edit a vocabulary entry by its study word.
    Vocabulary { word_study: String },
}

#[derive(Debug, Subcommand)]
pub enum UploadTarget {
    /// Import vocabulary from a CSV file.
    Vocabulary {
        /// Path to the UTF-8 CSV file.
        csv_path: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum StudyTarget {
    /// Study vocabulary using typed answers.
    Vocabulary,
}
