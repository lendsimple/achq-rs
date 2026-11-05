//! `cargo xtask conformance`: the hand-written request types against the vendored
//! gateway fragments.
//!
//! The fragments are too thin and too often wrong to generate the types from, so
//! the types are written by hand from the fragments and the guides together. This
//! check keeps them honest: every difference between a type's form fields (as its
//! `ToForm::form_fields` describes them) and its fragment's request body must be
//! listed in `schema/overrides.toml` with a reason, and every listed difference
//! must still exist.

use std::collections::{BTreeMap, BTreeSet};

use achq::Operation;
use achq::form::FieldMeta;
#[cfg(test)]
use achq::form::ToForm;
use serde_json::Value;

use crate::overrides::{OpOverrides, Overrides};

/// The request body a fragment documents: each property and whether it is
/// required.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment {
    pub properties: BTreeMap<String, bool>,
}

impl Fragment {
    pub fn parse(json: &str) -> Result<Self, String> {
        let spec: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let ops: Vec<&Value> = spec["paths"]
            .as_object()
            .into_iter()
            .flat_map(|p| p.values())
            .flat_map(|methods| methods.as_object().into_iter().flat_map(|m| m.values()))
            .collect();
        let [op] = ops[..] else {
            return Err(format!("expected one operation, found {}", ops.len()));
        };
        let schema = &op["requestBody"]["content"]["application/x-www-form-urlencoded"]["schema"];
        let required: BTreeSet<&str> = schema["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let properties = schema["properties"]
            .as_object()
            .ok_or("the operation has no form request body")?
            .keys()
            .map(|name| (name.clone(), required.contains(name.as_str())))
            .collect();
        Ok(Self { properties })
    }
}

/// The form fields of each gateway operation, by fragment name.
///
/// `ECheck.ProcessPayment` has two fragments, one per kind of payment source;
/// each is compared with the operation's fields for its kind.
pub fn registry() -> Vec<(&'static str, Vec<FieldMeta>)> {
    use achq::operations::{events, insights, payments, settlements, tokens};

    const TOKEN_FIELDS: &[&str] = &["AccountToken", "TokenSource"];
    const BANK_FIELDS: &[&str] = &[
        "RoutingNumber",
        "AccountNumber",
        "AccountType",
        "CheckType",
        "Create_ACHQToken",
    ];
    vec![
        ("banktoken", op::<tokens::CreateToken>()),
        (
            "bankpayment",
            without(op::<payments::CreatePayment>(), TOKEN_FIELDS),
        ),
        (
            "tokenizedpayment",
            without(op::<payments::CreatePayment>(), BANK_FIELDS),
        ),
        ("updatepayment", op::<payments::UpdatePayment>()),
        ("cancelpayment", op::<payments::CancelPayment>()),
        ("holdpayment", op::<payments::HoldPayment>()),
        ("refundpayment", op::<payments::RefundPayment>()),
        ("get-settlements", op::<settlements::ListSettlements>()),
        ("settlement-details", op::<settlements::SettlementDetails>()),
        ("eventmonitor", op::<events::TrackEvents>()),
        ("bank-account-status", op::<insights::VerifyAccount>()),
        ("bankaccountid", op::<insights::VerifyOwner>()),
    ]
}

/// The form fields an operation writes: its ID fields, then its body's.
fn op<O: Operation>() -> Vec<FieldMeta> {
    let mut out = Vec::new();
    O::form_fields(&mut out);
    out
}

fn without(fields: Vec<FieldMeta>, names: &[&str]) -> Vec<FieldMeta> {
    fields
        .into_iter()
        .filter(|field| !names.contains(&field.wire.as_str()))
        .collect()
}

/// The form fields `T` writes.
#[cfg(test)]
pub fn fields<T: ToForm>() -> Vec<FieldMeta> {
    let mut out = Vec::new();
    T::form_fields("", &mut out);
    out
}

/// Every difference not accounted for, and every override that no longer applies.
pub fn check(
    fragments: &BTreeMap<String, Fragment>,
    registry: &[(&str, Vec<FieldMeta>)],
    overrides: &Overrides,
) -> Vec<String> {
    let mut problems = Vec::new();
    let registered: BTreeMap<&str, &Vec<FieldMeta>> =
        registry.iter().map(|(op, fields)| (*op, fields)).collect();
    if registered.len() != registry.len() {
        problems.push("an operation is registered twice".into());
    }

    let known = |op: &str| fragments.contains_key(op);
    for op in registered.keys().filter(|op| !known(op)) {
        problems.push(format!("{op}: registered, but there is no fragment for it"));
    }
    for op in overrides.unregistered.iter().filter(|op| !known(op)) {
        problems.push(format!(
            "{op}: listed in `unregistered`, but there is no fragment for it"
        ));
    }
    for op in overrides.gateway.keys().filter(|op| !known(op)) {
        problems.push(format!(
            "{op}: has [gateway.{op}] overrides, but there is no fragment for it"
        ));
    }

    for (op, fragment) in fragments {
        let unregistered = overrides.unregistered.iter().any(|u| u == op);
        match (registered.get(op.as_str()), unregistered) {
            (Some(_), true) => problems.push(format!(
                "{op}: registered, so remove it from `unregistered`"
            )),
            (None, false) => problems.push(format!(
                "{op}: no request type is registered; register one or list it in `unregistered`"
            )),
            (None, true) => {
                if overrides.gateway.contains_key(op) {
                    problems.push(format!(
                        "{op}: has [gateway.{op}] overrides, but no request type is registered"
                    ));
                }
            }
            (Some(fields), false) => {
                let empty = OpOverrides::default();
                let accepted = overrides.gateway.get(op).unwrap_or(&empty);
                compare(op, fragment, fields, accepted, &mut problems);
            }
        }
    }
    problems
}

fn compare(
    op: &str,
    fragment: &Fragment,
    fields: &[FieldMeta],
    accepted: &OpOverrides,
    problems: &mut Vec<String>,
) {
    let mut ours: BTreeMap<&str, bool> = BTreeMap::new();
    for field in fields {
        if ours.insert(&field.wire, field.required).is_some() {
            problems.push(format!("{op}: the type writes {} twice", field.wire));
        }
    }
    // The overrides that matched a difference, as (kind, wire name).
    let mut used: BTreeSet<(&str, String)> = BTreeSet::new();
    let mut account =
        |kind: &'static str, wire: &str, map: &BTreeMap<String, String>, problem: String| {
            if map.contains_key(wire) {
                used.insert((kind, wire.to_owned()));
            } else {
                problems.push(problem);
            }
        };

    for (wire, &required) in &fragment.properties {
        match ours.get(wire.as_str()) {
            None => account(
                "missing",
                wire,
                &accepted.missing,
                format!("{op}: {wire} is in the fragment but not in the type"),
            ),
            Some(&ours_required) if ours_required != required => account(
                "required",
                wire,
                &accepted.required,
                format!(
                    "{op}: {wire} is {} in the fragment but {} in the type",
                    optionality(required),
                    optionality(ours_required)
                ),
            ),
            Some(_) => {}
        }
    }
    for wire in ours
        .keys()
        .filter(|w| !fragment.properties.contains_key(**w))
    {
        account(
            "extra",
            wire,
            &accepted.extra,
            format!("{op}: {wire} is in the type but not in the fragment"),
        );
    }

    for (kind, map) in [
        ("missing", &accepted.missing),
        ("extra", &accepted.extra),
        ("required", &accepted.required),
    ] {
        for wire in map.keys() {
            if !used.contains(&(kind, wire.clone())) {
                problems.push(format!(
                    "{op}: gateway.{op}.{kind}.{wire:?} no longer matches a difference; remove it"
                ));
            }
        }
    }
}

fn optionality(required: bool) -> &'static str {
    if required { "required" } else { "optional" }
}

#[cfg(test)]
mod tests {
    use achq::Model;

    use super::*;

    #[derive(Model)]
    #[achq(form)]
    struct Cancel {
        #[achq(form = "Transact_ReferenceID")]
        reference_id: String,
        #[achq(form = "Provider_TransactionID")]
        provider_transaction_id: Option<String>,
    }

    fn fragment(properties: &[(&str, bool)]) -> BTreeMap<String, Fragment> {
        let properties = properties
            .iter()
            .map(|(name, required)| ((*name).to_owned(), *required))
            .collect();
        BTreeMap::from([("cancelpayment".to_owned(), Fragment { properties })])
    }

    fn run(fragments: &BTreeMap<String, Fragment>, overrides: &str) -> Vec<String> {
        let registry = [("cancelpayment", fields::<Cancel>())];
        check(fragments, &registry, &Overrides::parse(overrides).unwrap())
    }

    #[test]
    fn parses_a_fragment() {
        let json = r#"{"paths": {"/?cancel": {"post": {"requestBody": {"content":
            {"application/x-www-form-urlencoded": {"schema": {
                "required": ["Transact_ReferenceID"],
                "properties": {"Transact_ReferenceID": {}, "Amount": {}}}}}}}}}}"#;
        assert_eq!(
            Fragment::parse(json).unwrap().properties,
            BTreeMap::from([
                ("Amount".to_owned(), false),
                ("Transact_ReferenceID".to_owned(), true)
            ])
        );
        assert!(Fragment::parse(r#"{"paths": {}}"#).is_err());
    }

    #[test]
    fn a_matching_type_passes() {
        let fragments = fragment(&[
            ("Transact_ReferenceID", true),
            ("Provider_TransactionID", false),
        ]);
        assert_eq!(run(&fragments, ""), Vec::<String>::new());
    }

    #[test]
    fn reports_each_difference() {
        let fragments = fragment(&[("Transact_ReferenceID", false), ("Amount", false)]);
        assert_eq!(
            run(&fragments, ""),
            [
                "cancelpayment: Amount is in the fragment but not in the type",
                "cancelpayment: Transact_ReferenceID is optional in the fragment but required in the type",
                "cancelpayment: Provider_TransactionID is in the type but not in the fragment",
            ]
        );
    }

    #[test]
    fn overrides_accept_differences() {
        let fragments = fragment(&[("Transact_ReferenceID", false), ("Amount", false)]);
        let overrides = r#"
            [gateway.cancelpayment]
            missing = { Amount = "cancelling has no amount" }
            required = { Transact_ReferenceID = "the guide marks it required" }
            extra = { Provider_TransactionID = "the guide documents it" }
        "#;
        assert_eq!(run(&fragments, overrides), Vec::<String>::new());
    }

    #[test]
    fn reports_stale_overrides() {
        let fragments = fragment(&[
            ("Transact_ReferenceID", true),
            ("Provider_TransactionID", false),
        ]);
        let overrides = r#"
            [gateway.cancelpayment]
            extra = { Provider_TransactionID = "no longer extra" }
            missing = { Gone = "was removed" }
        "#;
        assert_eq!(
            run(&fragments, overrides),
            [
                "cancelpayment: gateway.cancelpayment.missing.\"Gone\" no longer matches a difference; remove it",
                "cancelpayment: gateway.cancelpayment.extra.\"Provider_TransactionID\" no longer matches a difference; remove it",
            ]
        );
    }

    #[test]
    fn every_fragment_is_registered_or_listed() {
        let fragments = fragment(&[("Transact_ReferenceID", true)]);
        let none = Overrides::default();
        assert_eq!(
            check(&fragments, &[], &none),
            [
                "cancelpayment: no request type is registered; register one or list it in `unregistered`"
            ]
        );
        let listed = Overrides::parse(r#"unregistered = ["cancelpayment"]"#).unwrap();
        assert!(check(&fragments, &[], &listed).is_empty());
        assert_eq!(
            run(&fragments, r#"unregistered = ["cancelpayment"]"#),
            ["cancelpayment: registered, so remove it from `unregistered`"]
        );
    }

    #[test]
    fn reports_unknown_operations() {
        let fragments = fragment(&[("Transact_ReferenceID", true)]);
        let overrides = Overrides::parse(
            r#"
            unregistered = ["cancelpayment", "voidpayment"]
            [gateway.refund]
            extra = { A = "b" }
            "#,
        )
        .unwrap();
        assert_eq!(
            check(&fragments, &[("hold", Vec::new())], &overrides),
            [
                "hold: registered, but there is no fragment for it",
                "voidpayment: listed in `unregistered`, but there is no fragment for it",
                "refund: has [gateway.refund] overrides, but there is no fragment for it",
            ]
        );
    }
}
