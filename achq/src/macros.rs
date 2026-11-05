//! Declarative macros for the API's string enums and ID newtypes.

/// An enum of the string values the API documents, plus `Other` for values it does
/// not, so that a new value from the API never fails a response.
///
/// Values match exactly, or ignoring ASCII case with a leading `case_insensitive;`
/// (the API spells some values inconsistently, such as `In-process` and
/// `In-Process`). Either way the variant keeps the documented spelling.
///
/// ```ignore
/// string_enum! {
///     /// Who the money moves from.
///     pub enum PaymentDirection {
///         /// An ACH debit: the merchant collects from the customer.
///         FromCustomer = "FromCustomer",
///         ToCustomer = "ToCustomer",
///     }
/// }
/// ```
// Used by the request and response types from Stage 3 on; only tests use it now.
#[allow(unused_macros)]
macro_rules! string_enum {
    (
        @define $matching:ident;
        $(#[$meta:meta])*
        pub enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident = $value:literal
            ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        ///
        /// A value the API does not document is kept in `Other`. Match unknown values
        /// with a guard on [`as_str`](Self::as_str) rather than on `Other`: a later
        /// release may give the value its own variant.
        #[non_exhaustive]
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $(
                $(#[$variant_meta])*
                #[doc = concat!("`\"", $value, "\"`")]
                $variant,
            )+
            /// A value the API does not document.
            Other(::std::string::String),
        }

        impl $name {
            /// The value as sent over the wire.
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $value,)+
                    Self::Other(value) => value,
                }
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::core::convert::From<&str> for $name {
            fn from(value: &str) -> Self {
                $crate::macros::string_enum!(@match $matching value; $($variant = $value),+)
            }
        }

        impl ::core::convert::From<::std::string::String> for $name {
            fn from(value: ::std::string::String) -> Self {
                match Self::from(value.as_str()) {
                    Self::Other(_) => Self::Other(value),
                    known => known,
                }
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                <::std::string::String as ::serde::Deserialize>::deserialize(deserializer).map(Self::from)
            }
        }

        impl $crate::form::FormValue for $name {
            fn form_value(&self) -> ::std::borrow::Cow<'_, str> {
                ::std::borrow::Cow::Borrowed(self.as_str())
            }
        }

        $crate::validate::no_constraints!($name);
    };
    (@match exact $input:ident; $($variant:ident = $value:literal),+) => {
        match $input {
            $($value => Self::$variant,)+
            other => Self::Other(other.into()),
        }
    };
    (@match case_insensitive $input:ident; $($variant:ident = $value:literal),+) => {{
        $(
            if $input.eq_ignore_ascii_case($value) {
                return Self::$variant;
            }
        )+
        Self::Other($input.into())
    }};
    // The public forms come last, so that the internal `@` forms above never fall
    // into the catch-all.
    (case_insensitive; $($rest:tt)+) => {
        $crate::macros::string_enum!(@define case_insensitive; $($rest)+);
    };
    ($($rest:tt)+) => {
        $crate::macros::string_enum!(@define exact; $($rest)+);
    };
}

/// A newtype for an ID that is text. Read with surrounding whitespace trimmed, as
/// the API sometimes pads IDs (`" 2001"`).
///
/// With a leading `sensitive:`, `Debug` hides the value, for IDs that stand in for
/// a bank account.
macro_rules! string_id {
    (sensitive: $($(#[$meta:meta])* $name:ident;)+) => {$(
        $crate::macros::string_id!(@define $(#[$meta])* $name);

        impl ::core::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.debug_tuple(stringify!($name))
                    .field(&$crate::__private::Redacted::new(true))
                    .finish()
            }
        }
    )+};
    (@define $(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(::std::string::String);

        impl $name {
            /// Wraps an ID.
            pub fn new(id: impl ::core::convert::Into<::std::string::String>) -> Self {
                Self(id.into())
            }

            /// The ID's value.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl ::core::convert::From<&str> for $name {
            fn from(id: &str) -> Self {
                Self(id.into())
            }
        }

        impl ::core::convert::From<::std::string::String> for $name {
            fn from(id: ::std::string::String) -> Self {
                Self(id)
            }
        }

        impl ::core::convert::AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let id = <::std::string::String as ::serde::Deserialize>::deserialize(deserializer)?;
                let trimmed = id.trim();
                Ok(if trimmed.len() == id.len() {
                    Self(id)
                } else {
                    Self(trimmed.into())
                })
            }
        }

        impl $crate::form::FormValue for $name {
            fn form_value(&self) -> ::std::borrow::Cow<'_, str> {
                ::std::borrow::Cow::Borrowed(&self.0)
            }
        }

        $crate::validate::no_constraints!($name);
    };
    ($($(#[$meta:meta])* $name:ident;)+) => {$(
        $crate::macros::string_id!(@define $(#[$meta])* $name);

        impl ::core::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.debug_tuple(stringify!($name)).field(&self.0).finish()
            }
        }
    )+};
}

#[allow(unused_imports)]
pub(crate) use string_enum;
pub(crate) use string_id;

#[cfg(test)]
mod tests {
    use crate::form::FormValue;

    string_enum! {
        /// Who the money moves from.
        pub enum PaymentDirection {
            FromCustomer = "FromCustomer",
            ToCustomer = "ToCustomer",
        }
    }

    string_enum! {
        case_insensitive;
        /// A payment's status.
        pub enum Status {
            InProcess = "In-Process",
            Cleared = "Cleared",
        }
    }

    string_id! {
        /// A merchant.
        MerchantId;
    }

    #[test]
    fn ids_wrap_and_expose_their_value() {
        let id = MerchantId::new("2001");
        assert_eq!(id.as_str(), "2001");
    }

    string_id! {
        sensitive:
        /// A bank account token.
        Token;
    }

    #[test]
    fn known_values_read_as_their_variant() {
        let value: PaymentDirection = serde_json::from_str("\"ToCustomer\"").unwrap();
        assert_eq!(value, PaymentDirection::ToCustomer);
        assert_eq!(serde_json::to_string(&value).unwrap(), "\"ToCustomer\"");
        assert_eq!(value.form_value(), "ToCustomer");
    }

    #[test]
    fn unknown_values_read_as_other_and_round_trip() {
        let value: PaymentDirection = serde_json::from_str("\"Sideways\"").unwrap();
        assert_eq!(value, PaymentDirection::Other("Sideways".into()));
        assert_eq!(value.as_str(), "Sideways");
        assert_eq!(serde_json::to_string(&value).unwrap(), "\"Sideways\"");
    }

    #[test]
    fn exact_enums_match_case() {
        assert_eq!(
            PaymentDirection::from("fromcustomer"),
            PaymentDirection::Other("fromcustomer".into())
        );
    }

    #[test]
    fn case_insensitive_enums_keep_the_documented_spelling() {
        let value = Status::from("In-process");
        assert_eq!(value, Status::InProcess);
        assert_eq!(value.as_str(), "In-Process");
        assert_eq!(Status::from("CLEARED"), Status::Cleared);
        assert_eq!(Status::from("Held"), Status::Other("Held".into()));
    }

    #[test]
    fn from_string_prefers_a_listed_variant() {
        assert_eq!(
            PaymentDirection::from(String::from("FromCustomer")),
            PaymentDirection::FromCustomer
        );
        assert_eq!(PaymentDirection::FromCustomer.to_string(), "FromCustomer");
    }

    #[test]
    fn ids_are_read_trimmed() {
        let id: MerchantId = serde_json::from_str("\" 2001\"").unwrap();
        assert_eq!(id.as_str(), "2001");
        assert_eq!(serde_json::to_string(&id).unwrap(), "\"2001\"");
        assert_eq!(format!("{id:?}"), "MerchantId(\"2001\")");
        assert_eq!(id.form_value(), "2001");
    }

    #[test]
    fn sensitive_ids_hide_their_value_in_debug() {
        let token = Token::new("achq-sandbox-3ff6d-b64fd-5a8a5");
        assert_eq!(format!("{token:?}"), "Token([REDACTED])");
        assert_eq!(token.to_string(), "achq-sandbox-3ff6d-b64fd-5a8a5");
        assert_eq!(token.as_str(), "achq-sandbox-3ff6d-b64fd-5a8a5");
    }
}
