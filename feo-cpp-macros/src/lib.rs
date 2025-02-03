// Copyright 2025 Accenture.
//
// SPDX-License-Identifier: Apache-2.0

//! Macros supporting building of C++ components for FEO

extern crate proc_macro;

use proc_macro::{TokenStream, TokenTree};

const SEPARATOR: char = ',';

#[proc_macro]
pub fn make_fn(item: TokenStream) -> TokenStream {
    let (ident, params, ret_type) = parse(item);
    assert!(!ident.is_empty(), "missing function identifier");
    assert!(params.is_some(), "missing parameters group");
    assert!(ret_type.is_some(), "missing return-type group");
    let def_string = format!(
        "pub fn {} {} -> {};",
        ident,
        params.unwrap(),
        ret_type.unwrap()
    );
    def_string.parse().unwrap()
}

#[proc_macro]
pub fn make_fn_call(item: TokenStream) -> TokenStream {
    let (ident, params, dummy) = parse(item);
    assert!(!ident.is_empty(), "missing function identifier");
    assert!(params.is_some(), "missing parameters group");
    assert!(
        dummy.is_none(),
        "did not expect additional group, but found {}",
        dummy.unwrap()
    );
    let call_string = format!("{} {}", ident, params.unwrap(),);
    call_string.parse().unwrap()
}

/// Parse the input token stream into a concatenated identifier and up to two groups
fn parse(item: TokenStream) -> (String, Option<String>, Option<String>) {
    let mut ident = String::new();
    let mut expect_separator = false;
    let mut expect_ident = true;

    let mut group1: Option<String> = None;
    let mut group2: Option<String> = None;

    for token in item.into_iter() {
        // If a separator is expected, ensure that we find it, then continue
        if expect_separator {
            if let TokenTree::Punct(punct) = &token {
                assert_eq!(
                    punct.as_char(),
                    SEPARATOR,
                    "expected '{SEPARATOR}', but found '{token}'"
                );
                expect_separator = false;
                continue;
            } else {
                panic!("expected '{}', but found '{}'", SEPARATOR, token);
            }
        }

        // Otherwise handle other tokens
        match token {
            // If token is part of the identifier and is still acceptable, append it to
            // the concatenated identifier collected so far, then expect a separator
            TokenTree::Ident(part) => {
                assert!(
                    expect_ident,
                    "did not expect to find part of an identifier, but found '{}'",
                    part
                );
                ident.push_str(&part.to_string());
                expect_separator = true;
            }

            // If the token is a group and we do not yet have two groups stored,
            // store it as one of the groups. Then expect a separator and indicate
            // that we do no longer accept parts of the identifier.
            TokenTree::Group(grp) => {
                if group1.is_none() {
                    group1 = Some(grp.to_string());
                } else if group2.is_none() {
                    group2 = Some(grp.to_string());
                } else {
                    panic!("did not expect additional group, but found {}", grp);
                }
                expect_ident = false;
                expect_separator = true;
            }
            other => {
                panic!("did not expect {}", other);
            }
        }
    }
    (ident, group1, group2)
}
