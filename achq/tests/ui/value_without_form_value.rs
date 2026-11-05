#[derive(achq::Model)]
#[achq(form)]
struct Request {
    #[achq(form = "SendEmailToCustomer")]
    send_email: bool,
}

fn main() {}
