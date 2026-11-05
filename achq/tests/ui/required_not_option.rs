#[derive(achq::Model)]
struct Request {
    #[achq(required)]
    amount: String,
}

fn main() {}
