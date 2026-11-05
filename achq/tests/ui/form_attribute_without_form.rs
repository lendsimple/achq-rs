#[derive(achq::Model)]
struct Request {
    #[achq(form = "Amount")]
    amount: String,
}

fn main() {}
