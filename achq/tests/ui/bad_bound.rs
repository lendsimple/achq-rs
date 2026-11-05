#[derive(achq::Model)]
struct Request {
    #[achq(min = "zero")]
    status: i32,
}

fn main() {}
