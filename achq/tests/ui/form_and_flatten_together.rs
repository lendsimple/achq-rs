#[derive(achq::Model)]
#[achq(form)]
struct Billing {
    #[achq(form = "City")]
    city: String,
}

#[derive(achq::Model)]
#[achq(form)]
struct Request {
    #[achq(form = "Billing", flatten)]
    billing: Billing,
}

fn main() {}
