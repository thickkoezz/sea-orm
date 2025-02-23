use super::util::GetMeta;
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote, quote_spanned, ToTokens};
use syn::{
    ext::IdentExt, punctuated::Punctuated, token::Comma, Data, DataStruct, DeriveInput, Fields,
    Generics, Meta,
};

#[derive(Debug)]
enum Error {
    InputNotStruct,
}

enum ItemType {
    Flat,
    Skip,
    Nested,
}

struct DeriveFromQueryResult {
    ident: syn::Ident,
    generics: Generics,
    fields: Vec<FromQueryResultItem>,
}

struct FromQueryResultItem {
    pub typ: ItemType,
    pub ident: Ident,
    pub alias: Option<String>,
}

/// Initially, we try to obtain the value for each field and check if it is an ordinary DB error
/// (which we return immediatly), or a null error.
///
/// ### Background
///
/// Null errors do not necessarily mean that the deserialization as a whole fails,
/// since structs embedding the current one might have wrapped the current one in an `Option`.
/// In this case, we do not want to swallow other errors, which are very likely to actually be
/// programming errors that should be noticed (and fixed).
struct TryFromQueryResultCheck<'a>(&'a FromQueryResultItem);

impl ToTokens for TryFromQueryResultCheck<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let FromQueryResultItem { ident, typ, alias } = self.0;

        match typ {
            ItemType::Flat => {
                let name = alias
                    .to_owned()
                    .unwrap_or_else(|| ident.unraw().to_string());
                tokens.extend(quote! {
                    let #ident = match row.try_get_nullable(pre, #name) {
                        Err(v @ sea_orm::TryGetError::DbErr(_)) => {
                            return Err(v);
                        }
                        v => v,
                    };
                });
            }
            ItemType::Skip => {
                tokens.extend(quote! {
                    let #ident = std::default::Default::default();
                });
            }
            ItemType::Nested => {
                tokens.extend(quote! {
                    let #ident = match sea_orm::FromQueryResult::from_query_result_nullable(row, pre) {
                        Err(v @ sea_orm::TryGetError::DbErr(_)) => {
                            return Err(v);
                        }
                        v => v,
                    };
                });
            }
        }
    }
}

struct TryFromQueryResultAssignment<'a>(&'a FromQueryResultItem);

impl ToTokens for TryFromQueryResultAssignment<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let FromQueryResultItem { ident, typ, .. } = self.0;

        match typ {
            ItemType::Flat | ItemType::Nested => {
                tokens.extend(quote! {
                    #ident: #ident?,
                });
            }
            ItemType::Skip => {
                tokens.extend(quote! {
                    #ident,
                });
            }
        }
    }
}

impl DeriveFromQueryResult {
    fn new(
        DeriveInput {
            ident,
            data,
            generics,
            ..
        }: DeriveInput,
    ) -> Result<Self, Error> {
        let parsed_fields = match data {
            Data::Struct(DataStruct {
                fields: Fields::Named(named),
                ..
            }) => named.named,
            _ => return Err(Error::InputNotStruct),
        };

        let mut fields = Vec::with_capacity(parsed_fields.len());
        for parsed_field in parsed_fields {
            let mut typ = ItemType::Flat;
            let mut alias = None;
            for attr in parsed_field.attrs.iter() {
                if !attr.path().is_ident("sea_orm") {
                    continue;
                }
                if let Ok(list) = attr.parse_args_with(Punctuated::<Meta, Comma>::parse_terminated)
                {
                    for meta in list.iter() {
                        if meta.exists("skip") {
                            typ = ItemType::Skip;
                        } else if meta.exists("nested") {
                            typ = ItemType::Nested;
                        }
                        alias = meta.get_as_kv("from_alias")
                    }
                }
            }
            let ident = format_ident!("{}", parsed_field.ident.unwrap().to_string());
            fields.push(FromQueryResultItem { typ, ident, alias });
        }

        Ok(Self {
            ident,
            generics,
            fields,
        })
    }

    fn expand(&self) -> syn::Result<TokenStream> {
        Ok(self.impl_from_query_result())
    }

    fn impl_from_query_result(&self) -> TokenStream {
        let Self {
            ident,
            generics,
            fields,
        } = self;

        let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

        let ident_try_init: Vec<_> = fields.iter().map(TryFromQueryResultCheck).collect();
        let ident_try_assign: Vec<_> = fields.iter().map(TryFromQueryResultAssignment).collect();

        quote!(
            #[automatically_derived]
            impl #impl_generics sea_orm::FromQueryResult for #ident #ty_generics #where_clause {
                fn from_query_result(row: &sea_orm::QueryResult, pre: &str) -> std::result::Result<Self, sea_orm::DbErr> {
                    Ok(Self::from_query_result_nullable(row, pre)?)
                }

                fn from_query_result_nullable(row: &sea_orm::QueryResult, pre: &str) -> std::result::Result<Self, sea_orm::TryGetError> {
                    #(#ident_try_init)*

                    Ok(Self {
                        #(#ident_try_assign)*
                    })
                }
            }
        )
    }
}

pub fn expand_derive_from_query_result(input: DeriveInput) -> syn::Result<TokenStream> {
    let ident_span = input.ident.span();

    match DeriveFromQueryResult::new(input) {
        Ok(partial_model) => partial_model.expand(),
        Err(Error::InputNotStruct) => Ok(quote_spanned! {
            ident_span => compile_error!("you can only derive `FromQueryResult` on named struct");
        }),
    }
}
