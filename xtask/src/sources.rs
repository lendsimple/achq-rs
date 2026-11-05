//! Where the vendored files come from, and how each is cut out of its page.
//!
//! ACHQ publishes no single spec. Each API reference page embeds an OpenAPI
//! fragment for its one operation, and every page is served as Markdown at its URL
//! with `.md` appended, as listed in `llms.txt`.

/// The documentation's index of every page.
pub const INDEX_URL: &str = "https://developers.achq.com/llms.txt";

/// Reference pages that describe no operation.
pub const NOT_API: &[&str] = &[
    "introduction",
    "authentication-1",
    "response-codes",
    "run-in-postman",
    "industry-categories",
];

/// What a vendored file holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The page's OpenAPI fragment: its first ```` ```json ```` block.
    Json,
    /// The page's Markdown, without its front matter.
    Markdown,
}

/// A vendored file: its path under `schema/`, its page, and what it holds.
pub struct Source {
    pub file: &'static str,
    pub page: &'static str,
    pub kind: Kind,
}

impl Source {
    pub fn url(&self) -> String {
        format!("https://developers.achq.com/reference/{}.md", self.page)
    }

    /// The gateway operation's name, for a gateway fragment.
    pub fn gateway_op(&self) -> Option<&'static str> {
        self.file
            .strip_prefix("gateway/")
            .and_then(|f| f.strip_suffix(".json"))
    }
}

macro_rules! sources {
    ($($file:literal from $page:literal as $kind:ident,)+) => {
        &[$(Source { file: $file, page: $page, kind: Kind::$kind },)+]
    };
}

pub const SOURCES: &[Source] = sources![
    "gateway/banktoken.json" from "banktoken" as Json,
    "gateway/tokenizedpayment.json" from "tokenizedpayment" as Json,
    "gateway/bankpayment.json" from "bankpayment" as Json,
    "gateway/updatepayment.json" from "updatepayment" as Json,
    "gateway/cancelpayment.json" from "cancelpayment" as Json,
    "gateway/holdpayment.json" from "holdpayment" as Json,
    "gateway/refundpayment.json" from "refundpayment" as Json,
    "gateway/get-settlements.json" from "get-settlements" as Json,
    "gateway/settlement-details.json" from "settlement-details" as Json,
    "gateway/eventmonitor.json" from "eventmonitor" as Json,
    "gateway/bank-account-status.json" from "bank-account-status" as Json,
    "gateway/bankaccountid.json" from "bankaccountid" as Json,
    "onboarding/merchant-onboarding.json" from "merchant-onboarding" as Json,
    "onboarding/post_onboarding-fees.json" from "post_onboarding-fees" as Json,
    "industry-categories.md" from "industry-categories" as Markdown,
];

pub fn download(url: &str) -> Result<String, String> {
    ureq::get(url)
        .call()
        .and_then(|mut response| response.body_mut().read_to_string())
        .map_err(|e| format!("GET {url}: {e}"))
}

/// Cuts the vendored content out of a page.
pub fn extract(kind: Kind, page: &str) -> Result<String, String> {
    match kind {
        Kind::Json => {
            let start = page
                .find("```json\n")
                .ok_or("the page has no ```json block")?
                + "```json\n".len();
            let len = page[start..]
                .find("\n```")
                .ok_or("the ```json block is not closed")?;
            let json = &page[start..start + len];
            serde_json::from_str::<serde_json::Value>(json)
                .map_err(|e| format!("the ```json block does not parse: {e}"))?;
            Ok(format!("{json}\n"))
        }
        Kind::Markdown => {
            let body = match page.strip_prefix("---\n") {
                Some(rest) => {
                    let end = rest
                        .find("\n---\n")
                        .ok_or("the front matter is not closed")?;
                    &rest[end + "\n---\n".len()..]
                }
                None => page,
            };
            Ok(format!("{}\n", body.trim()))
        }
    }
}

/// The reference pages `llms.txt` lists, by name (`banktoken`).
pub fn reference_pages(index: &str) -> Vec<&str> {
    const PREFIX: &str = "https://developers.achq.com/reference/";
    let mut pages = Vec::new();
    let mut rest = index;
    while let Some(start) = rest.find(PREFIX) {
        rest = &rest[start + PREFIX.len()..];
        if let Some(end) = rest.find(".md)") {
            pages.push(&rest[..end]);
            rest = &rest[end..];
        }
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "---\nupdatedAt: 2025-04-03T21:39:53.000Z\n---\n\n# Cancel a payment\n\n\
                        # OpenAPI definition\n\n```json\n{\n  \"openapi\": \"3.1.0\"\n}\n```\n";

    #[test]
    fn extracts_the_json_block() {
        assert_eq!(
            extract(Kind::Json, PAGE).unwrap(),
            "{\n  \"openapi\": \"3.1.0\"\n}\n"
        );
        assert!(extract(Kind::Json, "# no block").is_err());
        assert!(extract(Kind::Json, "```json\n{oops}\n```").is_err());
    }

    #[test]
    fn strips_the_front_matter() {
        let body = extract(Kind::Markdown, PAGE).unwrap();
        assert!(body.starts_with("# Cancel a payment\n"), "{body}");
        assert!(!body.contains("updatedAt"));
        assert_eq!(extract(Kind::Markdown, "# Plain\n").unwrap(), "# Plain\n");
    }

    #[test]
    fn lists_reference_pages() {
        let index = "## API Reference: Payments\n\
                     - [Create bank token](https://developers.achq.com/reference/banktoken.md)\n\
                     - [Guide](https://developers.achq.com/docs/create-a-token.md)\n\
                     - [Hold](https://developers.achq.com/reference/holdpayment.md): Hold it\n";
        assert_eq!(reference_pages(index), ["banktoken", "holdpayment"]);
    }

    #[test]
    fn gateway_ops_are_named_by_file() {
        let ops: Vec<&str> = SOURCES.iter().filter_map(Source::gateway_op).collect();
        assert_eq!(ops.len(), 12);
        assert_eq!(ops[0], "banktoken");
        assert_eq!(
            SOURCES[0].url(),
            "https://developers.achq.com/reference/banktoken.md"
        );
    }
}
