//! MCP server — Model Context Protocol implementation over stdio and HTTP.
//!
//! Tool handler logic is split into modules by domain:
//! - [`tools_core`] — 14 core tools (query, dependencies, callers, etc.)
//! - [`tools_git`] — 9 git integration tools (blame, history, etc.)
//! - [`tools_security`] — 9 security scanning tools (OWASP, CWE, taint, etc.)
//! - [`tools_analysis`] — 7 repository & analysis tools (stats, imports, etc.)
//! - [`tools_dataflow`] — 6 call graph & data flow tools (find_path, complexity, etc.)
//! - [`server`] — deep_query tool (cross-encoder re-ranked search)
//! - [`tasks`] — MCP Tasks for async operations (indexing, etc.)
//! - [`http`] — HTTP transport (streamable HTTP via axum)
//!
//! Also exposes 3 MCP Prompts: review-security, explain-function, pre-refactor-check.

pub mod http;
pub mod registry;
pub mod server;
pub mod tasks;
pub mod tools_analysis;
pub mod tools_core;
pub mod tools_dataflow;
pub mod tools_git;
pub mod tools_security;
