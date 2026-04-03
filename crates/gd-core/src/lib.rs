#![allow(clippy::must_use_candidate)]

pub mod ast_owned;
pub mod cfg;
#[allow(dead_code)]
pub mod color;
pub mod config;
pub mod debug_types;
pub mod fs;
#[allow(dead_code)]
pub mod gd_ast;
pub mod lint_types;
pub mod parser;
#[allow(dead_code)]
pub mod printer;
pub mod process;
pub mod project;
pub mod resource_parser;
#[allow(dead_code)]
pub mod rewriter;
pub mod scene;
pub mod ssr;
pub mod type_inference;
pub mod workspace_index;
