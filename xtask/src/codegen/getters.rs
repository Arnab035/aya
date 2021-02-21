use std::collections::HashMap;

use proc_macro2::{Span, TokenStream};
use quote::{quote, TokenStreamExt};
use syn::{
    self, Fields, FieldsNamed, Generics, Ident, Item, ItemStruct, ItemUnion, Path, Type, TypePath,
    Visibility,
};

pub struct GetterList<'a> {
    slf: Ident,
    item_fields: HashMap<Ident, (&'a Item, &'a FieldsNamed)>,
}

impl<'a> GetterList<'a> {
    pub fn new(items: &'a [Item]) -> GetterList<'a> {
        let item_fields = items
            .iter()
            .filter_map(|item| {
                unpack_item(item).map(|(ident, _generics, fields)| (ident.clone(), (item, fields)))
            })
            .collect();
        GetterList {
            slf: Ident::new("self", Span::call_site()),
            item_fields,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&'a Item, Vec<Getter<'_>>)> {
        self.item_fields
            .values()
            .map(move |(item, fields)| (*item, self.getters(&self.slf, fields)))
    }

    fn getters(&self, ident: &'a Ident, fields: &'a FieldsNamed) -> Vec<Getter<'a>> {
        let mut getters = Vec::new();
        for field in &fields.named {
            if field.vis == Visibility::Inherited {
                continue;
            }

            let field_ident = field.ident.as_ref().unwrap();
            let field_s = field_ident.to_string();

            // FIXME: bindgen generates fields named `_bitfield_N` for bitfields. If a type T has
            // two or more unions with bitfields, the getters for the bitfields - generated in impl
            // T - will clash. To avoid that we skip getters for bitfields altogether for now.
            // See sk_reuseport_md for an example where the clash happens.
            if field_s.starts_with("_bitfield") {
                continue;
            }

            if field_s.starts_with("__bindgen_anon") {
                let field_ty_ident = match &field.ty {
                    Type::Path(TypePath {
                        path: Path { segments, .. },
                        ..
                    }) => &segments.first().unwrap().ident,
                    _ => panic!(),
                };
                let sub_fields = self
                    .item_fields
                    .get(field_ty_ident)
                    .expect(&field_ident.to_string())
                    .1;
                getters.extend(self.getters(field_ident, sub_fields).drain(..).map(
                    |mut getter| {
                        getter.prefix.insert(0, ident);
                        getter
                    },
                ));
            } else {
                getters.push(Getter {
                    ident: field_ident,
                    prefix: vec![ident],
                    ty: &field.ty,
                });
            }
        }

        getters
    }
}

pub fn generate_getters_for_items(
    items: &[Item],
    gen_getter: fn(&Getter<'_>) -> TokenStream,
) -> TokenStream {
    let mut tokens = TokenStream::new();
    tokens.append_all(GetterList::new(&items).iter().map(|(item, getters)| {
        let getters = getters.iter().map(gen_getter);
        let (ident, generics, _) = unpack_item(item).unwrap();
        quote! {
            impl#generics #ident#generics {
                #(#getters)*
            }
        }
    }));

    tokens
}

#[derive(Debug)]
pub struct Getter<'a> {
    pub ident: &'a Ident,
    pub prefix: Vec<&'a Ident>,
    pub ty: &'a Type,
}

fn unpack_item(item: &Item) -> Option<(&Ident, &Generics, &FieldsNamed)> {
    match item {
        Item::Struct(ItemStruct {
            ident,
            generics,
            fields: Fields::Named(fields),
            ..
        })
        | Item::Union(ItemUnion {
            ident,
            generics,
            fields,
            ..
        }) => Some((ident, generics, fields)),
        _ => None,
    }
}
