#[derive(achq::Model)]
struct Request {
    #[achq(rule = "no_emoji")]
    name: String,
}

fn main() {}
