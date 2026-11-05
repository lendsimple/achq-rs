#[derive(achq::Model)]
#[achq(form)]
struct Request {
    #[achq(form = "SendEmailToCustomer", with = "true_false")]
    send_email: bool,
}

fn main() {}
