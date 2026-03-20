use anyhow::{Context, Result};
use tree_sitter::{Parser, Tree};

/// Parse a TypeScript source string and return the syntax tree.
pub fn parse_typescript(source: &str) -> Result<Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .context("Failed to set tree-sitter TypeScript language")?;

    parser
        .parse(source, None)
        .context("Failed to parse TypeScript source")
}

/// Parse a TSX source string and return the syntax tree.
pub fn parse_tsx(source: &str) -> Result<Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
        .context("Failed to set tree-sitter TSX language")?;

    parser
        .parse(source, None)
        .context("Failed to parse TSX source")
}

/// Parse a file based on its extension (.tsx → TSX, otherwise TypeScript).
pub fn parse_file(source: &str, path: &str) -> Result<Tree> {
    if path.ends_with(".tsx") {
        parse_tsx(source)
    } else {
        parse_typescript(source)
    }
}
