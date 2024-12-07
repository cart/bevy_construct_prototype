// FIXME(15321): solve CI failures, then replace with `#![expect()]`.
#![allow(missing_docs, reason = "Not all docs are written yet, see #3492.")]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

use bevy_macro_utils::BevyManifest;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, Fields, Index, Path};

extern crate proc_macro;

#[proc_macro_derive(Construct, attributes(prop))]
pub fn derive_construct(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let manifest = BevyManifest::default();
    let ecs_path = manifest.get_path("bevy_ecs");
    let construct_path = manifest.get_path("bevy_construct_prototype");

    let struct_name = &ast.ident;
    let (impl_generics, type_generics, where_clause) = &ast.generics.split_for_impl();

    let props_type = format_ident!("{struct_name}Props");

    let tokens = match &ast.data {
        Data::Struct(data_struct) => {
            let StructImpl {
                is_named,
                from_props_fields,
                props_fields,
                props_fields_defaults,
            } = struct_impl(&data_struct.fields, &construct_path, false);
            let props_type_declaration = if is_named {
                quote! {
                    #[allow(missing_docs)]
                    #[derive(Clone, Reflect)]
                    pub struct #props_type #impl_generics #where_clause {
                        #(#props_fields)*
                    }

                    impl #impl_generics Default for #props_type #type_generics #where_clause {
                        fn default() -> Self {
                            Self {
                                #(#props_fields_defaults)*
                            }
                        }
                    }
                }
            } else {
                quote! {
                    #[allow(missing_docs)]
                    #[derive(Clone, Reflect)]
                    pub struct #props_type #impl_generics (#(#props_fields)*) #where_clause;

                    impl #impl_generics Default for #props_type #type_generics #where_clause {
                        fn default() -> Self {
                            Self(#(#props_fields_defaults)*)
                        }
                    }
                }
            };
            quote! {
                #props_type_declaration

                impl #impl_generics #construct_path::Construct for #struct_name #type_generics #where_clause {
                    type Props = #props_type #type_generics #where_clause;

                    fn construct(
                        _context: &mut #ecs_path::world::EntityWorldMut,
                        props: Self::Props,
                    ) -> Result<Self, #construct_path::ConstructError> {
                        Ok(Self {
                            #(#from_props_fields)*
                        })
                    }
                }
            }
        }
        Data::Enum(data_enum) => {
            let mut variant_props_entries = Vec::new();
            let mut variant_from_props_match = Vec::new();
            let mut variant_apply_props = Vec::new();

            let mut first_variant_default_ident = None;
            for variant in &data_enum.variants {
                let StructImpl {
                    is_named,
                    from_props_fields,
                    props_fields,
                    ..
                } = struct_impl(&variant.fields, &construct_path, true);
                let ident = &variant.ident;
                // Props will always default to the first variant with all None
                let variant_name_lower = variant.ident.to_string().to_lowercase();
                let variant_default_name = format_ident!("default_{}", variant_name_lower);
                if first_variant_default_ident.is_none() {
                    first_variant_default_ident = Some(variant_default_name.clone());
                }
                if variant.fields.is_empty() {
                    variant_props_entries.push(quote! {#ident});
                    variant_from_props_match.push(quote! {
                        #props_type::#ident => #struct_name::#ident,
                    });
                    variant_apply_props.push(quote! {
                        #props_type::#ident => {},
                    });
                } else {
                    let destructure_fields =
                        variant.fields.iter().enumerate().map(|(i, f)| {
                            f.ident.clone().unwrap_or_else(|| format_ident!("t{}", i))
                        });
                    if is_named {
                        variant_props_entries.push(quote! {#ident {
                            #(#props_fields)*
                        }});
                        variant_from_props_match.push(quote! {
                                #props_type::#ident { #(#destructure_fields,)* } => #struct_name::#ident { #(#from_props_fields)* },
                            });
                    } else {
                        variant_props_entries.push(quote! {#ident(#(#props_fields)*)});
                        variant_from_props_match.push(quote! {
                                #props_type::#ident(#(#destructure_fields,)*) => #struct_name::#ident(#(#from_props_fields)*),
                            });
                    }
                }
            }

            quote! {
                #[allow(missing_docs)]
                #[derive(Clone, Reflect)]
                pub enum #props_type #type_generics #where_clause {
                    #(#variant_props_entries,)*
                }

                impl #impl_generics #construct_path::Construct for #struct_name #type_generics #where_clause {
                    type Props = #props_type #type_generics #where_clause;

                    fn construct(
                        _context: &mut #ecs_path::world::EntityWorldMut,
                        props: Self::Props,
                    ) -> Result<Self, #construct_path::ConstructError> {
                        Ok(match props {
                            #(#variant_from_props_match)*
                        })
                    }
                }
            }
        }
        Data::Union(_) => todo!("Union types are not supported yet."),
    };

    TokenStream::from(tokens)
}

struct StructImpl {
    is_named: bool,
    from_props_fields: Vec<proc_macro2::TokenStream>,
    props_fields: Vec<proc_macro2::TokenStream>,
    props_fields_defaults: Vec<proc_macro2::TokenStream>,
}

const PROP: &str = "prop";

fn struct_impl(fields: &Fields, construct_path: &Path, is_enum: bool) -> StructImpl {
    let mut from_props_fields = Vec::new();
    let mut props_fields = Vec::new();
    let mut props_fields_defaults = Vec::new();
    let is_named = matches!(fields, Fields::Named(_));
    for (index, field) in fields.iter().enumerate() {
        let ident = &field.ident;
        let ty = &field.ty;
        let field_index = Index::from(index);
        let is_prop = field
            .attrs
            .iter()
            .find(|a| a.path().is_ident(PROP))
            .is_some();
        let is_pub = matches!(field.vis, syn::Visibility::Public(_));
        let maybe_pub = if is_pub { quote!(pub) } else { quote!() };
        if is_named {
            if is_prop {
                props_fields.push(quote! {
                    #maybe_pub #ident: #construct_path::ConstructProp<#ty>,
                });
                props_fields_defaults.push(quote! {
                    #ident: #construct_path::ConstructProp::Props(Default::default()),
                });

                if is_enum {
                    from_props_fields.push(quote! {
                        #ident: match #ident {
                            #construct_path::ConstructProp::Props(p) => #construct_path::Construct::construct(_context, p)?,
                            #construct_path::ConstructProp::Value(v) => v,
                        },
                    });
                } else {
                    from_props_fields.push(quote! {
                        #ident: match props.#ident {
                            #construct_path::ConstructProp::Props(p) => #construct_path::Construct::construct(_context, p)?,
                            #construct_path::ConstructProp::Value(v) => v,
                        },
                    });
                }
            } else {
                props_fields.push(quote! {
                    #maybe_pub #ident: #ty,
                });
                props_fields_defaults.push(quote! {
                    #ident: Default::default(),
                });

                if is_enum {
                    from_props_fields.push(quote! {
                        #ident: #ident,
                    });
                } else {
                    from_props_fields.push(quote! {
                        #ident: props.#ident,
                    });
                }
            }
        } else {
            if is_prop {
                props_fields.push(quote! {
                    #maybe_pub #construct_path::ConstructProp<#ty>,
                });

                props_fields_defaults.push(quote! {
                    #construct_path::ConstructProp::Props(Default::default()),
                });

                if is_enum {
                    let enum_tuple_ident = format_ident!("t{}", index);
                    from_props_fields.push(
                        quote! {
                            match #enum_tuple_ident {
                                #construct_path::ConstructProp::Props(p) => #construct_path::Construct::construct(_context, p)?,
                                #construct_path::ConstructProp::Value(v) => v,
                            },
                        }
                    );
                } else {
                    from_props_fields.push(
                        quote! {
                            #field_index: match props.#field_index {
                                #construct_path::ConstructProp::Props(p) => #construct_path::Construct::construct(_context, p)?,
                                #construct_path::ConstructProp::Value(v) => v,
                            },
                        }
                    );
                }
            } else {
                props_fields.push(quote! {
                    #maybe_pub #ty,
                });

                props_fields_defaults.push(quote! {
                    Default::default(),
                });

                if is_enum {
                    let enum_tuple_ident = format_ident!("t{}", index);
                    from_props_fields.push(quote! { #enum_tuple_ident, });
                } else {
                    from_props_fields.push(quote! { props.#field_index, });
                }
            }
        }
    }

    StructImpl {
        is_named,
        from_props_fields,
        props_fields,
        props_fields_defaults,
    }
}
