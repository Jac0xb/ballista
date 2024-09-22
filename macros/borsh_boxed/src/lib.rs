#![recursion_limit = "128"]
extern crate proc_macro;

use proc_macro::TokenStream;
// use proc_macro::TokenStream;
// use quote::quote;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DataEnum, DeriveInput, Fields, Ident, Type, Variant};
// use syn::{parse_macro_input, Data, DataEnum, DeriveInput, Fields, Ident, Type, Variant};

// deserialize

#[proc_macro_derive(BorshDeserializeBoxed)]
pub fn borsh_deserialize_boxed_derive(input: TokenStream) -> TokenStream {
    let DeriveInput { ident, data, .. } = parse_macro_input!(input);

    let gen = match data {
        Data::Enum(data_enum) => impl_borsh_deserialize_for_enum(ident, data_enum),
        _ => unimplemented!("BorshDeserializeBoxed can only be derived for enums"),
    };

    gen.into()
}

fn impl_borsh_deserialize_for_enum(name: Ident, data_enum: DataEnum) -> proc_macro2::TokenStream {
    let variants = data_enum
        .variants
        .into_iter()
        .enumerate()
        .map(|(index, variant)| impl_deserialize_for_variant(&name, index as u8, variant));

    quote! {
        impl borsh::BorshDeserialize for #name {
            fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
                let tag = u8::deserialize_reader(reader)?;
                match tag {
                    #(#variants),*,
                    _ => Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Invalid tag",
                    ))
                }
            }
        }
    }
}

fn impl_deserialize_for_variant(
    enum_name: &Ident,
    tag: u8,
    variant: Variant,
) -> proc_macro2::TokenStream {
    let variant_ident = variant.ident;

    match variant.fields {
        Fields::Unnamed(fields) => {
            let field_deserializers = fields.unnamed.into_iter().map(|field| {
                let ty = field.ty;
                deserialize_field(ty)
            });

            quote! {
                #tag => {
                    Ok(#enum_name::#variant_ident(
                        #(#field_deserializers),*
                    ))
                }
            }
        }
        Fields::Named(fields) => {
            let field_deserializers = fields.named.into_iter().map(|field| {
                let field_name = field.ident.unwrap();
                let ty = field.ty;
                let deserializer = deserialize_field(ty);
                quote! {
                    #field_name: #deserializer
                }
            });

            quote! {
                #tag => {
                    Ok(#enum_name::#variant_ident {
                        #(#field_deserializers),*
                    })
                }
            }
        }
        Fields::Unit => {
            quote! {
                #tag => Ok(#enum_name::#variant_ident)
            }
        }
    }
}

fn deserialize_field(ty: Type) -> proc_macro2::TokenStream {
    if is_box_type(&ty) {
        quote! {
            Box::new(borsh::BorshDeserialize::deserialize_reader(reader)?)
        }
    } else {
        quote! {
            borsh::BorshDeserialize::deserialize_reader(reader)?
        }
    }
}

fn is_box_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.first() {
            return segment.ident == "Box";
        }
    }
    false
}

// serialize
#[proc_macro_derive(BorshSerializeBoxed)]
pub fn borsh_serialize_boxed_derive(input: TokenStream) -> TokenStream {
    let DeriveInput { ident, data, .. } = parse_macro_input!(input);

    let gen = match data {
        Data::Enum(data_enum) => impl_borsh_serialize_for_enum(ident, data_enum),
        _ => unimplemented!("BorshSerializeBoxed can only be derived for enums"),
    };

    gen.into()
}

fn impl_borsh_serialize_for_enum(name: Ident, data_enum: DataEnum) -> proc_macro2::TokenStream {
    let variants = data_enum
        .variants
        .iter()
        .enumerate()
        .map(|(index, variant)| impl_serialize_for_variant(&name, index as u8, variant));

    quote! {
        impl borsh::BorshSerialize for #name {
            fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
                match self {
                    #(#variants),*
                }
            }
        }
    }
}

fn impl_serialize_for_variant(
    enum_name: &Ident,
    tag: u8,
    variant: &Variant,
) -> proc_macro2::TokenStream {
    let variant_ident = &variant.ident;
    let tag_lit = tag;

    match &variant.fields {
        Fields::Unnamed(fields) => {
            let field_pats: Vec<_> = fields
                .unnamed
                .iter()
                .enumerate()
                .map(|(i, _)| format_ident!("field{}", i))
                .collect();

            let field_serializers = fields.unnamed.iter().enumerate().map(|(i, field)| {
                let field_ident = &field_pats[i];
                let ty = &field.ty;
                serialize_field(quote! { #field_ident }, ty)
            });

            quote! {
                #enum_name::#variant_ident(#(#field_pats),*) => {
                    borsh::BorshSerialize::serialize(&#tag_lit, writer)?;
                    #(#field_serializers;)*
                    Ok(())
                }
            }
        }
        Fields::Named(fields) => {
            let field_pats: Vec<_> = fields
                .named
                .iter()
                .map(|f| f.ident.as_ref().unwrap())
                .collect();

            let field_serializers = fields.named.iter().map(|field| {
                let field_ident = field.ident.as_ref().unwrap();
                let ty = &field.ty;
                serialize_field(quote! { #field_ident }, ty)
            });

            quote! {
                #enum_name::#variant_ident { #(#field_pats),* } => {
                    borsh::BorshSerialize::serialize(&#tag_lit, writer)?;
                    #(#field_serializers;)*
                    Ok(())
                }
            }
        }
        Fields::Unit => {
            quote! {
                #enum_name::#variant_ident => {
                    borsh::BorshSerialize::serialize(&#tag_lit, writer)?;
                    Ok(())
                }
            }
        }
    }
}

fn serialize_field(field_access: proc_macro2::TokenStream, ty: &Type) -> proc_macro2::TokenStream {
    if is_box_type(ty) {
        quote! {
            borsh::BorshSerialize::serialize(&**#field_access, writer)?
        }
    } else {
        quote! {
            borsh::BorshSerialize::serialize(&#field_access, writer)?
        }
    }
}

// fn is_box_type(ty: &Type) -> bool {
//     if let Type::Path(type_path) = ty {
//         if let Some(segment) = type_path.path.segments.first() {
//             return segment.ident == "Box";
//         }
//     }
//     false
// }
