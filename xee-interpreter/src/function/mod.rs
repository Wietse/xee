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
// `StaticFunctionType` and `FunctionKind` are macro-support surface:
// external code shouldn't name them, but `#[xpath_fn]` /
// `wrap_xpath_fn!` expand to absolute paths that reference them. Both
// are `#[doc(hidden)]` at their declaration.
pub use static_function::{FunctionKind, StaticFunctionType};
// Public registry API: a host builds `StaticFunctionDescription`s (via
// `wrap_xpath_fn!`) and registers them; `ExtensionFunctions` is the
// resulting per-context table.
pub use static_function::{ExtensionFunctions, StaticFunctionDescription};
pub(crate) use static_function::{StaticFunction, StaticFunctions};
