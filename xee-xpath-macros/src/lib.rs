extern crate proc_macro;

#[macro_use]
mod error;
mod convert;
mod parse;
mod wrapper;

use quote::quote;
use syn::parse_macro_input;

use parse::XPathFnOptions;
use wrapper::{strip_injection_attrs, xpath_fn_wrapper};

/// Wrap a Rust function so it can be used as an XPath function.
///
/// # Crate-rename limitation
///
/// The macro's generated code references types by absolute path,
/// rooted at `::xee_interpreter` (e.g.
/// `::xee_interpreter::context::DynamicContext`). This means the
/// consumer crate must have `xee-interpreter` available in its
/// dependency namespace under the default name `xee_interpreter`.
/// Renaming the dependency in `Cargo.toml` will break the macro.
#[proc_macro_attribute]
pub fn xpath_fn(
    attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let options = parse_macro_input!(attr as XPathFnOptions);
    let mut ast = parse_macro_input!(input as syn::ItemFn);
    let wrapper = xpath_fn_wrapper(&ast, &options).unwrap_or_else(|e| e.into_compile_error());
    // The macro consumes `#[xpath_context]` / `#[xpath_interpreter]`
    // parameter attributes during injection detection; strip them
    // from the re-emitted function so the compiler doesn't reject
    // them as unknown.
    strip_injection_attrs(&mut ast);
    quote!(
        #ast
        #wrapper
    )
    .into()
}
