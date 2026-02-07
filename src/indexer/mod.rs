//! Indexer pipeline: parse source files, extract symbols, and build the code graph.

pub mod embedder;
pub mod extractor;
pub mod parser;
pub mod pipeline;

pub use embedder::EmbeddingEngine;
pub use extractor::Extractor;
pub use parser::CodeParser;
pub use pipeline::{IndexOptions, IndexResult, IndexingPipeline};
