use proc_macro2::{Ident, Span};
use quote::{format_ident, quote, ToTokens};
use syn::spanned::Spanned;
use syn::{ItemFn, LitStr, Type};

use xee_xpath_ast::ast::Signature;
use xot::xmlname::NameStrInfo;

use crate::convert::convert_sequence_type;
use crate::parse::XPathFnOptions;

pub(crate) fn xpath_fn_wrapper(
    ast: &ItemFn,
    options: &XPathFnOptions,
) -> syn::Result<proc_macro2::TokenStream> {
    let name = &ast.sig.ident;
    let wrapper_name = format_ident!("wrapper_{}", name);
    let wrapper = make_wrapper(name, &wrapper_name, ast, &options.signature)?;

    let vis = &ast.vis;
    let signature_string = LitStr::new(&options.signature_string, Span::call_site());
    let kind = if let Some(kind) = &options.kind {
        LitStr::new(kind, Span::call_site())
    } else {
        LitStr::new("", Span::call_site())
    };
    Ok(quote! {
        // create a module with the same name as the function - this way `use
        // <the function> will bring both the function and module into scope.
        // This module contains information about the wrapper function
        // we access with the wrap_xpath_fn! macro.
        #[doc(hidden)]
        #vis mod #name {
            pub(crate) struct MakeWrapper;
            pub(crate) const WRAPPER: ::xee_interpreter::function::StaticFunctionType =
                MakeWrapper::WRAPPER;
            // We store the signature as a string; this means we need to
            // reparse it again later during registration, but it's a lot
            // easier than trying to serialize a data structure, so it will
            // do for now.
            pub(crate) const SIGNATURE: &str = #signature_string;
            pub(crate) const KIND: &str = #kind;
        }

        // Generate the function inside of the same scope at the original
        // function (but in an isolated block), so that it can easily call the
        // original function. Using `super` isn't useful for that, as the
        // original function may be inside of a function body.
        const _: () = {
            // This is a trick to ensure we can get it into the module defined
            // above
            impl #name::MakeWrapper {
                const WRAPPER: ::xee_interpreter::function::StaticFunctionType = #wrapper_name;
            }
            #vis #wrapper
        };
    })
}

fn make_wrapper(
    name: &Ident,
    wrapper_name: &Ident,
    ast: &ItemFn,
    signature: &Signature,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut conversions = Vec::new();
    let mut conversion_names = Vec::new();
    let mut adjust = 0;
    // The wrapper's parameters are hardcoded under prefixed names
    // (`__xpath_fn_context`, `__xpath_fn_interpreter`,
    // `__xpath_fn_arguments`) so an XPath signature parameter named
    // `$context` or `$arguments` can never shadow them. Injection
    // simply forwards these prefixed locals into the user's function.
    let ctx_local: Ident = Ident::new("__xpath_fn_context", Span::call_site());
    let interp_local: Ident = Ident::new("__xpath_fn_interpreter", Span::call_site());
    let args_local: Ident = Ident::new("__xpath_fn_arguments", Span::call_site());

    if is_injected_arg(ast, adjust, "context", "xpath_context")? {
        conversion_names.push(ctx_local.clone());
        adjust += 1;
    }
    if is_injected_arg(ast, adjust, "interpreter", "xpath_interpreter")? {
        conversion_names.push(interp_local.clone());
        adjust += 1;
    }

    let expected_inputs = signature.params.len() + adjust;
    if ast.sig.inputs.len() != expected_inputs {
        let sig_count = signature.params.len();
        let rust_user_count = ast.sig.inputs.len().saturating_sub(adjust);
        let injected_note = match adjust {
            0 => String::new(),
            n => format!(" (after subtracting {n} injected `context`/`interpreter` argument(s))"),
        };
        bail_spanned!(
            ast.sig.ident.span() =>
            format!(
                "#[xpath_fn] arity mismatch: signature declares {sig_count} XPath parameter(s), \
                 but the Rust function takes {rust_user_count}{injected_note}"
            )
        );
    }

    for (i, param) in signature.params.iter().enumerate() {
        let name = Ident::new(param.name.local_name(), Span::call_site());
        conversion_names.push(name.clone());
        let arg = quote!(#args_local[#i]);
        let fn_arg = &ast.sig.inputs[i + adjust];
        conversions.push(convert_sequence_type(
            &param.type_,
            fn_arg,
            name.to_token_stream(),
            arg,
            &interp_local,
        )?);
    }

    let body = if is_result(ast) {
        quote!(#(#conversions)*;
        let value = #name(#(#conversion_names),*);
        value.map(|v| v.into()))
    } else {
        quote!(#(#conversions)*;
        let value = #name(#(#conversion_names),*);
        Ok(value.into()))
    };

    Ok(quote!(
        fn #wrapper_name(
            #ctx_local: &::xee_interpreter::context::DynamicContext,
            #interp_local: &mut ::xee_interpreter::interpreter::Interpreter,
            #args_local: &[::xee_interpreter::sequence::Sequence],
        ) -> ::std::result::Result<
            ::xee_interpreter::sequence::Sequence,
            ::xee_interpreter::error::Error,
        > {
            #body
        }
    ))
}

/// Decide whether the argument at `index` in `ast.sig.inputs` should
/// be treated as the macro-injected `context` / `interpreter` slot.
///
/// Two ways to opt in to injection at the given position:
///
/// 1. The argument carries an explicit parameter attribute named
///    `attr_name` (e.g. `#[xpath_context]` for the context slot).
///    The macro strips these helper attributes before re-emitting
///    the user's function — see [`strip_injection_attrs`].
///
/// 2. The argument is named with the literal fallback identifier
///    `fallback_name` (e.g. `context` for the context slot). This
///    preserves backwards compatibility with existing internal
///    library functions.
///
/// Type-based detection was tried earlier and rejected: a proc macro
/// only sees syntax, so any type-shape match is necessarily heuristic
/// (false positives for unrelated `&my_app::context::DynamicContext`,
/// false negatives for renamed imports or type aliases). Explicit
/// attribute opt-in is unambiguous.
fn is_injected_arg(
    ast: &ItemFn,
    index: usize,
    fallback_name: &str,
    attr_name: &str,
) -> syn::Result<bool> {
    if index >= ast.sig.inputs.len() {
        return Ok(false);
    }
    let arg = &ast.sig.inputs[index];
    let pat_type = match arg {
        syn::FnArg::Typed(pat_type) => pat_type,
        syn::FnArg::Receiver(r) => {
            bail_spanned!(r.span() => "XPath functions cannot take `self` as an argument");
        }
    };

    // Explicit attribute wins.
    if pat_type
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident(attr_name))
    {
        return Ok(true);
    }

    // Name-based fallback.
    if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
        if pat_ident.ident == fallback_name {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Walk the function's parameters and remove the helper attributes
/// (`#[xpath_context]`, `#[xpath_interpreter]`) the macro consumes.
/// Without this, the compiler would see them in the re-emitted user
/// function and complain that the attributes are unknown.
pub(crate) fn strip_injection_attrs(ast: &mut ItemFn) {
    for arg in &mut ast.sig.inputs {
        if let syn::FnArg::Typed(pat_type) = arg {
            pat_type.attrs.retain(|attr| {
                !attr.path().is_ident("xpath_context") && !attr.path().is_ident("xpath_interpreter")
            });
        }
    }
}

fn is_result(ast: &ItemFn) -> bool {
    let return_type = &ast.sig.output;
    match return_type {
        syn::ReturnType::Default => false,
        syn::ReturnType::Type(_, type_) => match type_.as_ref() {
            Type::Path(type_path) => {
                matches!(
                    type_path
                        .path
                        .segments
                        .last()
                        .unwrap()
                        .ident
                        .to_string()
                        .as_str(),
                    "Result"
                )
            }
            _ => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_debug_snapshot;
    use syn::parse_str;

    #[test]
    fn test_wrapper() {
        let options =
            parse_str::<XPathFnOptions>(r#""fn:foo($x as xs:int) as xs:string""#).unwrap();
        let ast = parse_str::<ItemFn>(
            r#"
            fn foo(x: &i64) -> String {
                format!("{}", x)
            }"#,
        )
        .unwrap();
        assert_debug_snapshot!(xpath_fn_wrapper(&ast, &options).unwrap().to_string());
    }

    #[test]
    fn test_wrapper_items_sequence_arg() {
        let options =
            parse_str::<XPathFnOptions>(r#""fn:foo($x as item()*) as xs:string""#).unwrap();
        let ast = parse_str::<ItemFn>(
            r#"
            fn foo(x: &crate::Sequence) -> String {
                "foo".to_string()
            }"#,
        )
        .unwrap();
        assert_debug_snapshot!(xpath_fn_wrapper(&ast, &options).unwrap().to_string());
    }

    #[test]
    fn test_wrapper_too_few_rust_args_errors() {
        // signature declares two XPath parameters but the Rust fn only
        // takes one — used to panic with index-out-of-bounds; now should
        // surface as a clean syn::Error pointing at the fn name.
        let options =
            parse_str::<XPathFnOptions>(r#""fn:foo($x as xs:int, $y as xs:int) as xs:string""#)
                .unwrap();
        let ast = parse_str::<ItemFn>(r#"fn foo(x: &i64) -> String { format!("{}", x) }"#).unwrap();
        assert_debug_snapshot!(xpath_fn_wrapper(&ast, &options).unwrap_err().to_string());
    }

    #[test]
    fn test_wrapper_too_many_rust_args_errors() {
        // Rust fn has one more arg than the signature declares.
        // The old code silently dropped the extra arg; we now reject.
        let options =
            parse_str::<XPathFnOptions>(r#""fn:foo($x as xs:int) as xs:string""#).unwrap();
        let ast = parse_str::<ItemFn>(r#"fn foo(x: &i64, y: &i64) -> String { format!("{}", x) }"#)
            .unwrap();
        assert_debug_snapshot!(xpath_fn_wrapper(&ast, &options).unwrap_err().to_string());
    }

    #[test]
    fn test_wrapper_arity_mismatch_with_injected_context() {
        // Even with context injection, arity is checked correctly:
        // signature 0 params + context injected = 1 Rust arg expected,
        // but Rust fn has 2.
        let options = parse_str::<XPathFnOptions>(r#""fn:foo() as xs:string""#).unwrap();
        let ast = parse_str::<ItemFn>(
            r#"fn foo(context: &DynamicContext, extra: &i64) -> String { String::new() }"#,
        )
        .unwrap();
        assert_debug_snapshot!(xpath_fn_wrapper(&ast, &options).unwrap_err().to_string());
    }

    #[test]
    fn test_wrapper_context_injection_by_explicit_attr() {
        // The variable is named `ctx`, not `context`, so name-based
        // detection does not fire. The `#[xpath_context]` parameter
        // attribute makes the injection explicit. The snapshot must
        // show the wrapper passing its `__xpath_fn_context` local to
        // the user's `foo`, *not* a name like `ctx` that the wrapper
        // never binds.
        let options =
            parse_str::<XPathFnOptions>(r#""fn:foo($x as xs:int) as xs:string""#).unwrap();
        let ast = parse_str::<ItemFn>(
            r#"fn foo(#[xpath_context] ctx: &DynamicContext, x: &i64) -> String { format!("{}", x) }"#,
        )
        .unwrap();
        assert_debug_snapshot!(xpath_fn_wrapper(&ast, &options).unwrap().to_string());
    }

    #[test]
    fn test_wrapper_interpreter_injection_by_explicit_attr() {
        let options =
            parse_str::<XPathFnOptions>(r#""fn:foo($x as xs:int) as xs:string""#).unwrap();
        let ast = parse_str::<ItemFn>(
            r#"fn foo(#[xpath_interpreter] interp: &mut Interpreter, x: &i64) -> String { format!("{}", x) }"#,
        )
        .unwrap();
        assert_debug_snapshot!(xpath_fn_wrapper(&ast, &options).unwrap().to_string());
    }

    #[test]
    fn test_wrapper_explicit_attr_overrides_arbitrary_type_name() {
        // The user signals injection explicitly via the attribute,
        // even though the Rust type isn't obviously a `DynamicContext`.
        // We trust the user; if the type doesn't actually match what
        // the wrapper passes in, rustc will catch the mismatch with a
        // clean error pointing at the user's code.
        let options = parse_str::<XPathFnOptions>(r#""fn:foo() as xs:string""#).unwrap();
        let ast = parse_str::<ItemFn>(
            r#"fn foo(#[xpath_context] dc: &my_app::DynamicContext) -> String { String::new() }"#,
        )
        .unwrap();
        // The macro expansion succeeds; the resulting wrapper relies on
        // rustc to verify that `&my_app::DynamicContext` is compatible
        // with the value the wrapper passes in.
        assert_debug_snapshot!(xpath_fn_wrapper(&ast, &options).unwrap().to_string());
    }

    #[test]
    fn test_wrapper_xpath_param_named_context_does_not_shadow_injection() {
        // The XPath signature has a parameter literally named
        // `$context`. The wrapper's injected local is now prefixed
        // (`__xpath_fn_context`), so the user's emitted
        // `let context = …;` for the regular param cannot shadow it.
        // The snapshot must show the call passing both locals
        // independently: `foo(__xpath_fn_context, context)`.
        let options =
            parse_str::<XPathFnOptions>(r#""fn:foo($context as xs:string) as xs:string""#).unwrap();
        let ast = parse_str::<ItemFn>(
            r#"fn foo(#[xpath_context] ctx: &DynamicContext, s: &str) -> String { format!("{}", s) }"#,
        )
        .unwrap();
        assert_debug_snapshot!(xpath_fn_wrapper(&ast, &options).unwrap().to_string());
    }

    #[test]
    fn test_wrapper_xpath_param_named_arguments_does_not_shadow_slice() {
        // The XPath signature has parameters literally named
        // `$arguments` and `$x`. The wrapper's slice parameter is now
        // `__xpath_fn_arguments`, so emitting `let arguments = …;` for
        // the first user param cannot shadow the slice when the second
        // user param's conversion is generated.
        let options = parse_str::<XPathFnOptions>(
            r#""fn:foo($arguments as xs:int, $x as xs:int) as xs:int""#,
        )
        .unwrap();
        let ast =
            parse_str::<ItemFn>(r#"fn foo(arguments: &i64, x: &i64) -> i64 { *arguments + *x }"#)
                .unwrap();
        assert_debug_snapshot!(xpath_fn_wrapper(&ast, &options).unwrap().to_string());
    }
}
