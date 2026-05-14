/// XPath can be extended with both static functions as well as user defined
/// functions.
mod array;
mod function_core;
mod inline_function;
mod map;
mod signature;
mod static_function;

pub use array::Array;
pub use function_core::Function;
pub use function_core::{
    InlineFunctionData, InlineFunctionId, StaticFunctionData, StaticFunctionId,
};
pub use inline_function::{CastType, InlineFunction, Name};
pub use map::Map;
pub use signature::Signature;

pub use static_function::FunctionRule;
// The next three are part of the macro-support surface: external code
// shouldn't name them directly, but the `#[xpath_fn]` and
// `wrap_xpath_fn!` macros expand to absolute paths that reference
// them. Each is marked `#[doc(hidden)]` at its declaration.
pub use static_function::StaticFunctionType;
pub use static_function::{FunctionKind, StaticFunctionDescription};
pub(crate) use static_function::{StaticFunction, StaticFunctions};
