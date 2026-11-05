//! The onboarding API's request types against the vendored schemas in
//! `schema/onboarding/`: every key the types write must be a property of its
//! schema, and every other difference (a property never written, or a required
//! property the type can leave out) must be listed below with its reason.
//!
//! The schemas are not part of the published crate, so outside the repository
//! this skips.
#![cfg(feature = "onboarding")]

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use achq::onboarding::{
    AuthorizedRepresentative, BankAccounts, FeeChoice, FeeSchedule, FundingAccount,
    MerchantApplication, PaymentProfile,
};
use achq::types::FeeProfileId;
use achq::{Decimal, dec};
use serde_json::Value;

/// (schema, property): why the types never write it.
const NEVER_WRITTEN: &[(&str, &str, &str)] = &[];

/// (schema, property): why a property the schema requires can be left out.
const OPTIONAL_HERE: &[(&str, &str, &str)] = &[(
    "MerchantOnboardingRequest",
    "authorizedRepresentative",
    "The schema's description calls the representative optional (open item).",
)];

/// (schema, property): why the types always write a property the schema leaves
/// optional.
const REQUIRED_HERE: &[(&str, &str, &str)] = &[
    (
        "PaymentProfile",
        "chargeDollarsMonth",
        "Written with the charge limits, which enable charges: the schema requires them then.",
    ),
    (
        "PaymentProfile",
        "chargesPerMonth",
        "Written with the charge limits, which enable charges: the schema requires them then.",
    ),
    (
        "PaymentProfile",
        "chargesMaxAmount",
        "Written with the charge limits, which enable charges: the schema requires them then.",
    ),
    (
        "PaymentProfile",
        "enablePPD",
        "Written from the list of SEC codes, of which the schema requires one.",
    ),
    (
        "PaymentProfile",
        "enableCCD",
        "Written from the list of SEC codes, of which the schema requires one.",
    ),
    (
        "PaymentProfile",
        "enableWEB",
        "Written from the list of SEC codes, of which the schema requires one.",
    ),
    (
        "PaymentProfile",
        "enableTEL",
        "Written from the list of SEC codes, of which the schema requires one.",
    ),
    (
        "MerchantOnboardingRequest",
        "feeProfileId",
        "One of feeProfileId and feeProfile is required; the minimal application uses an ID.",
    ),
];

fn schemas(file: &str) -> Option<BTreeMap<String, Value>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../schema/onboarding")
        .join(file);
    let text = std::fs::read_to_string(path).ok()?;
    let spec: Value = serde_json::from_str(&text).unwrap();
    let schemas = spec["components"]["schemas"].as_object().unwrap().clone();
    Some(schemas.into_iter().collect())
}

/// The schema each written object is checked against, by its path.
fn schema_of(path: &str) -> &'static str {
    match path {
        "" => "MerchantOnboardingRequest",
        "businessEntity" => "BusinessEntity",
        "principalOwner" => "PrincipalOwner",
        "authorizedRepresentative" => "AuthorizedRepresentative",
        "paymentProfile" => "PaymentProfile",
        "bankAccounts" => "BankAccounts",
        "bankAccounts.transfers" => "Transfers",
        "bankAccounts.billing" => "Billing",
        "onboardFunctions" => "OnboardFunctions",
        "feeProfile" => "FeeProfile",
        other => panic!("no schema for {other}"),
    }
}

/// The keys written in each object, by schema.
fn keys(value: &Value, path: &str, out: &mut BTreeMap<&'static str, BTreeSet<String>>) {
    let Value::Object(map) = value else { return };
    let entry = out.entry(schema_of(path)).or_default();
    entry.extend(map.keys().cloned());
    for (key, child) in map {
        if child.is_object() {
            let child_path = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            keys(child, &child_path, out);
        }
    }
}

fn written(applications: &[MerchantApplication]) -> BTreeMap<&'static str, BTreeSet<String>> {
    let mut out = BTreeMap::new();
    for application in applications {
        keys(&serde_json::to_value(application).unwrap(), "", &mut out);
    }
    out
}

/// The smallest valid application: no optional field set.
fn minimal() -> MerchantApplication {
    let mut application = common::application();
    application.business_entity.industry = None;
    application.business_entity.mcc = None;
    application.business_entity.address2 = None;
    application.business_entity.external_id = None;
    application.principal_owner.address2 = None;
    application.authorized_representative = None;
    application.payment_profile = PaymentProfile::builder()
        .charges(achq::onboarding::Limits::new(1, 1, 1))
        .consumers(true)
        .sec_codes(vec![achq::payments::SecCode::Web])
        .statement_descriptor("X")
        .customer_service_phone("5558675309")
        .customer_service_email("x@example.com")
        .service_use_case("x")
        .build();
    application.bank_accounts = BankAccounts::builder().build();
    application.fees = FeeChoice::Profile(FeeProfileId::new("fee-1"));
    application
}

fn full_fees() -> FeeSchedule {
    let amount = dec!(1);
    FeeSchedule::builder()
        .name("all")
        .monthly_fee(amount)
        .transaction_fee(amount)
        .return_fee(amount)
        .dispute_fee(amount)
        .percent_fee(Decimal::ONE)
        .percent_cap(amount)
        .bank_status_fee(amount)
        .bank_id_fee(amount)
        .setup_fee(amount)
        .meta_data("k=v")
        .build()
}

/// Every optional field set, in each of its forms.
fn maximal() -> Vec<MerchantApplication> {
    let mut bank = common::application();
    bank.business_entity.mcc = Some("5734".into());
    bank.business_entity.address2 = Some("Suite 1".into());
    bank.business_entity.external_id = Some("m-1".into());
    bank.principal_owner.address2 = Some("Apt 2".into());
    bank.authorized_representative = Some(
        AuthorizedRepresentative::builder()
            .first_name("Lucille")
            .last_name("Bluth")
            .title("CFO")
            .email("lucille@example.com")
            .phone("5558675309")
            .ownership_percent(40)
            .build(),
    );
    bank.bank_accounts = BankAccounts::builder()
        .transfers(FundingAccount::bank("First Bank", "123123123", "1"))
        .billing(FundingAccount::bank("First Bank", "123123123", "2"))
        .build();
    bank.fees = FeeChoice::Schedule(full_fees());
    let mut tokens = bank.clone();
    tokens.bank_accounts = BankAccounts::builder()
        .transfers(FundingAccount::plaid("t1"))
        .billing(FundingAccount::plaid("t2"))
        .build();
    tokens.fees = FeeChoice::Profile(FeeProfileId::new("fee-1"));
    vec![bank, tokens]
}

fn listed(list: &[(&str, &str, &str)], schema: &str, property: &str) -> bool {
    list.iter()
        .any(|(s, p, reason)| *s == schema && *p == property && !reason.is_empty())
}

#[test]
fn the_application_matches_its_schemas() {
    let Some(schemas) = schemas("merchant-onboarding.json") else {
        eprintln!("skipping: schema/ is not here");
        return;
    };
    for application in maximal().iter().chain([&minimal()]) {
        achq::validate::Validate::validate(application).unwrap();
    }
    let all = written(&maximal());
    let required = written(&[minimal()]);
    let mut problems = Vec::new();
    let mut used = BTreeSet::new();

    for (schema, keys) in &all {
        let spec = &schemas[*schema];
        let properties: BTreeSet<String> = spec["properties"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let schema_required: BTreeSet<String> = spec["required"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        for key in keys.difference(&properties) {
            problems.push(format!(
                "{schema}: writes {key}, which the schema does not have"
            ));
        }
        for key in properties.difference(keys) {
            if listed(NEVER_WRITTEN, schema, key) {
                used.insert((*schema, key.clone()));
            } else {
                problems.push(format!("{schema}: never writes {key}"));
            }
        }
        let ours = required.get(schema).cloned().unwrap_or_default();
        for key in schema_required.difference(&ours) {
            if listed(OPTIONAL_HERE, schema, key) {
                used.insert((*schema, key.clone()));
            } else {
                problems.push(format!("{schema}: {key} is required, but can be left out"));
            }
        }
        for key in ours.difference(&schema_required) {
            if listed(REQUIRED_HERE, schema, key) {
                used.insert((*schema, key.clone()));
            } else {
                problems.push(format!("{schema}: always writes {key}, which is optional"));
            }
        }
    }
    for (schema, property, _) in NEVER_WRITTEN
        .iter()
        .chain(OPTIONAL_HERE)
        .chain(REQUIRED_HERE)
    {
        if !used.contains(&(*schema, (*property).to_owned())) {
            problems.push(format!(
                "{schema}.{property} is listed but no longer differs"
            ));
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn the_fee_schedule_matches_its_schema() {
    let Some(schemas) = schemas("post_onboarding-fees.json") else {
        eprintln!("skipping: schema/ is not here");
        return;
    };
    let properties: BTreeSet<String> = schemas["CreateFeeRequest"]["properties"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let written: BTreeSet<String> = serde_json::to_value(full_fees())
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(written, properties);
    // Every fee is optional.
    assert!(schemas["CreateFeeRequest"]["required"].is_null());
}
