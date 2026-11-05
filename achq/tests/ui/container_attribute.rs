#[derive(achq::Model)]
#[achq(sensitive)]
struct Request {
    name: String,
}

fn main() {}
