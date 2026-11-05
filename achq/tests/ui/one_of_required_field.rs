#[derive(achq::Model)]
#[achq(one_of = "account_number | account_token")]
struct Payment {
    account_number: String,
    account_token: Option<String>,
}

fn main() {}
