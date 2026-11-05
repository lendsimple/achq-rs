#[derive(achq::Model)]
#[achq(form)]
struct Request {
    #[achq(form = "Amount")]
    amount: String,
    description: Option<String>,
}

fn main() {}
