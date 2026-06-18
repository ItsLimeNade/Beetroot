use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, LitStr, parse_macro_input};

#[proc_macro_attribute]
pub fn track_analytics(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(item as ItemFn);

    let command_name = if !args.is_empty() {
        match syn::parse::<LitStr>(args) {
            Ok(lit) => lit.value(),
            Err(_) => input.sig.ident.to_string(),
        }
    } else {
        input.sig.ident.to_string()
    };

    let block = &input.block;

    let new_block = quote! {
        {
            let __start = std::time::Instant::now();

            let result = async { #block }.await;

            let __duration = __start.elapsed().as_millis() as u64;
            let __db = ctx.data().database.clone();
            let __user_id = ctx.author().id.get();
            let __cmd_name = #command_name.to_string();

            tokio::spawn(async move {
                if let Err(e) = __db.log_command_execution(&__cmd_name, __user_id, __duration).await {
                    tracing::error!("Analytics Error [{}]: {}", __cmd_name, e);
                }
            });

            result
        }
    };

    input.block = syn::parse2(new_block).expect("Failed to parse wrapped block");
    TokenStream::from(quote!(#input))
}
