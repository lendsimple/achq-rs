#[derive(achq::Model)]
struct Request {
    #[achq(max_length = 10)]
    page_size: u16,
}

fn main() {}
