#![allow(dead_code)]

#[macro_use]
extern crate num_derive;

// Make this crate reachable by its own name in absolute paths. The
// `#[xpath_fn]` macro emits paths like `::xee_interpreter::context::
// DynamicContext` so that the same emitted code resolves in both the
// internal library (here) and any external consumer crate. Without
// this alias, the `::xee_interpreter::*` paths fail to resolve inside
// the crate itself.
extern crate self as xee_interpreter;

pub mod atomic;
pub mod context;
pub mod declaration;
pub mod error;
pub mod function;
pub mod interpreter;
mod library;
pub mod occurrence;
pub mod pattern;
pub mod sequence;
pub mod span;
pub mod stack;
pub mod string;
pub mod xml;

pub use xee_name::{Name, Namespaces, VariableNames};
