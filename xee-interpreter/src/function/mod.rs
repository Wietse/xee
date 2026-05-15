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
// `StaticFunctionDescription` is the host-facing registry handle: a
// host builds one (via `wrap_xpath_fn!`) and passes it to
// `StaticContextBuilder::add_function`. `ExtensionFunctions` is the
// resulting per-context table — internal, never named by a host.
pub use static_function::StaticFunctionDescription;
pub(crate) use static_function::{ExtensionFunctions, StaticFunction, StaticFunctions};
