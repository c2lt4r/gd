#![allow(clippy::must_use_candidate)]

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
pub mod process;
pub mod project;
pub mod resource_parser;
pub mod scene;
pub mod type_inference;
pub mod workspace_index;
