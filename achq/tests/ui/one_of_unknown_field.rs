#[derive(achq::Model)]
#[achq(one_of = "account_number | account_tokn")]
struct Payment {
    account_number: Option<String>,
    account_token: Option<String>,
}

fn main() {}
