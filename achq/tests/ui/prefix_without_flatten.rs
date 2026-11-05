#[derive(achq::Model)]
#[achq(form)]
struct Request {
    #[achq(form = "City", prefix = "Billing_")]
    city: String,
}

fn main() {}
