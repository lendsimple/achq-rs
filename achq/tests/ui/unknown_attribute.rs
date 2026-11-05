#[derive(achq::Model)]
struct Request {
    #[achq(secret)]
    account_number: String,
}

fn main() {}
