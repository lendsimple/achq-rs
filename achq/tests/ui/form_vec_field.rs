#[derive(achq::Model)]
#[achq(form)]
struct Request {
    #[achq(form = "Tags")]
    tags: Vec<String>,
}

fn main() {}
