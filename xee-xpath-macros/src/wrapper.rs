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
            pub(crate) const WRAPPER: crate::function::StaticFunctionType = MakeWrapper::WRAPPER;
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
                const WRAPPER: crate::function::StaticFunctionType = #wrapper_name;
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
    if is_injected_arg(ast, adjust, "context", "DynamicContext")? {
        // Push the canonical name `context`, which is what the
        // generated wrapper actually binds. The user's variable name
        // is irrelevant — Rust binds positionally when we call their
        // function below.
        conversion_names.push(Ident::new("context", Span::call_site()));
        adjust += 1;
    }
    if is_injected_arg(ast, adjust, "interpreter", "Interpreter")? {
        conversion_names.push(Ident::new("interpreter", Span::call_site()));
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
        let arg = quote!(arguments[#i]);
        let fn_arg = &ast.sig.inputs[i + adjust];
        conversions.push(convert_sequence_type(
            &param.type_,
            fn_arg,
            name.to_token_stream(),
            arg,
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
        fn #wrapper_name(context: &crate::context::DynamicContext, interpreter: &mut crate::interpreter::Interpreter, arguments: &[crate::sequence::Sequence]) -> Result<crate::sequence::Sequence, crate::error::Error> {
        #body
    }))
}

/// Decide whether the argument at `index` in `ast.sig.inputs` should
/// be treated as the macro-injected `context` / `interpreter` slot.
/// `module` and `type_name` identify the expected type (e.g. `context`
/// and `DynamicContext`).
///
/// Matches the *type*, not the variable name, so `ctx: &DynamicContext`
/// works just as well as `context: &DynamicContext`. The match is
/// deliberately strict on the module qualifier: a bare type name
/// (`&DynamicContext`) is accepted, and qualified forms are only
/// accepted when the second-to-last path segment matches the expected
/// module (so `&context::DynamicContext`,
/// `&crate::context::DynamicContext`, and
/// `&xee_interpreter::context::DynamicContext` all qualify, but
/// `&my_app::DynamicContext` is rejected).
fn is_injected_arg(
    ast: &ItemFn,
    index: usize,
    module: &str,
    type_name: &str,
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
    Ok(matches_xee_type(&pat_type.ty, module, type_name))
}

/// Return true iff `ty` is `&T` or `&mut T` where `T` is either the
/// bare ident `type_name` or a path whose last two segments are
/// `module::type_name`.
fn matches_xee_type(ty: &syn::Type, module: &str, type_name: &str) -> bool {
    let syn::Type::Reference(type_ref) = ty else {
        return false;
    };
    let syn::Type::Path(type_path) = &*type_ref.elem else {
        return false;
    };
    let segments = &type_path.path.segments;
    let Some(last) = segments.last() else {
        return false;
    };
    if last.ident != type_name {
        return false;
    }
    match segments.len() {
        // Bare `DynamicContext` / `Interpreter`.
        1 => true,
        // Qualified path: insist the second-to-last segment is the
        // expected Xee internal module name. Rejects `&my_app::T`.
        n => segments[n - 2].ident == module,
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
    fn test_wrapper_context_injection_by_type_not_name() {
        // The first arg's *type* is &DynamicContext, but its variable
        // is named `ctx`. The macro should still inject the dynamic
        // context — historically it required the literal name `context`.
        let options =
            parse_str::<XPathFnOptions>(r#""fn:foo($x as xs:int) as xs:string""#).unwrap();
        let ast = parse_str::<ItemFn>(
            r#"fn foo(ctx: &DynamicContext, x: &i64) -> String { format!("{}", x) }"#,
        )
        .unwrap();
        assert_debug_snapshot!(xpath_fn_wrapper(&ast, &options).unwrap().to_string());
    }

    #[test]
    fn test_wrapper_interpreter_injection_by_type_not_name() {
        // Same idea for interpreter injection.
        let options =
            parse_str::<XPathFnOptions>(r#""fn:foo($x as xs:int) as xs:string""#).unwrap();
        let ast = parse_str::<ItemFn>(
            r#"fn foo(interp: &mut Interpreter, x: &i64) -> String { format!("{}", x) }"#,
        )
        .unwrap();
        assert_debug_snapshot!(xpath_fn_wrapper(&ast, &options).unwrap().to_string());
    }

    #[test]
    fn test_wrapper_path_prefixed_context_type_still_matches() {
        // &xee_interpreter::context::DynamicContext should also work —
        // path prefix doesn't matter, only the last segment.
        let options = parse_str::<XPathFnOptions>(r#""fn:foo() as xs:string""#).unwrap();
        let ast = parse_str::<ItemFn>(
            r#"fn foo(context: &xee_interpreter::context::DynamicContext) -> String { String::new() }"#,
        )
        .unwrap();
        assert_debug_snapshot!(xpath_fn_wrapper(&ast, &options).unwrap().to_string());
    }

    #[test]
    fn test_wrapper_named_context_with_wrong_type_treated_as_regular() {
        // `context: &str` should NOT trigger injection. With type-based
        // detection it becomes a regular signature arg.
        let options =
            parse_str::<XPathFnOptions>(r#""fn:foo($context as xs:string) as xs:string""#).unwrap();
        let ast = parse_str::<ItemFn>(
            r#"fn foo(context: &str) -> String { context.to_string() }"#,
        )
        .unwrap();
        // Should succeed — `context` is just a regular parameter.
        assert_debug_snapshot!(xpath_fn_wrapper(&ast, &options).unwrap().to_string());
    }

    #[test]
    fn test_wrapper_unrelated_dynamic_context_not_injected() {
        // `&my_app::DynamicContext` shares the last path segment but
        // not the module qualifier — must not be silently injected
        // as Xee's context. Here the user supplies their own type;
        // the macro should treat it as a regular signature arg, which
        // means the arity check fires (the signature declares zero
        // params but the Rust fn has one).
        let options =
            parse_str::<XPathFnOptions>(r#""fn:foo() as xs:string""#).unwrap();
        let ast = parse_str::<ItemFn>(
            r#"fn foo(ctx: &my_app::DynamicContext) -> String { String::new() }"#,
        )
        .unwrap();
        assert_debug_snapshot!(xpath_fn_wrapper(&ast, &options).unwrap_err().to_string());
    }
}
