//! Every gateway command, by resource: one type per command, such as
//! [`payments::CreatePayment`] or [`events::TrackEvents`].
//!
//! Each type implements [`Operation`](crate::Operation), so it can be encoded with
//! [`protocol::encode`](crate::protocol::encode) and sent with any HTTP client.
//! The types are generated from `operation_table!`, the crate's list of commands.

/// The gateway's commands, by resource. Passes them to `$callback`, which
/// generates code for each: [`impl_operations!`] the operation types, and the
/// clients their resource handles.
///
/// Each command names:
/// - `command`: the gateway's `Command` value;
/// - `scope`: `Merchant` (needs a merchant's credentials) or `Platform`;
/// - `extra`: fields every request of the command sends;
/// - `fields`: ID fields, which the operation type takes before its body;
/// - `body`: the request type, if any;
/// - `response`: what the command returns.
macro_rules! operation_table {
    ($callback:ident) => {
        $callback! {
            /// Tokens for bank accounts.
            tokens => Tokens {
                /// Creates an [`AchqToken`](crate::types::AchqToken) for a bank
                /// account, to charge or pay it later without its numbers.
                create => CreateToken {
                    command: "ECheck.CreateACHQToken",
                    scope: Merchant,
                    extra: [],
                    fields: {},
                    body: [$crate::tokens::TokenRequest],
                    response: $crate::tokens::TokenCreated,
                }
            }

            /// Payments: creating, changing, cancelling, holding and refunding
            /// them.
            payments => Payments {
                /// Creates a payment from or to a bank account, or the account a
                /// token stands for.
                create => CreatePayment {
                    command: "ECheck.ProcessPayment",
                    scope: Merchant,
                    extra: [],
                    fields: {},
                    body: [$crate::payments::PaymentRequest],
                    response: $crate::payments::PaymentCreated,
                }
                /// Changes a scheduled payment. The receipt's ID identifies the
                /// update, not the payment.
                update => UpdatePayment {
                    command: "ECheck.Update",
                    scope: Merchant,
                    extra: [],
                    fields: { payment_id: $crate::types::PaymentId = "Transact_ReferenceID", },
                    body: [$crate::payments::PaymentUpdate],
                    response: $crate::payments::CommandReceipt,
                }
                /// Cancels a scheduled payment.
                cancel => CancelPayment {
                    command: "ECheck.Void",
                    scope: Merchant,
                    extra: [],
                    fields: { payment_id: $crate::types::PaymentId = "Transact_ReferenceID", },
                    body: [],
                    response: $crate::payments::CommandReceipt,
                }
                /// Holds a scheduled payment, keeping it from being sent until it is
                /// updated or cancelled.
                hold => HoldPayment {
                    command: "ECheck.Hold",
                    scope: Merchant,
                    extra: [],
                    fields: { payment_id: $crate::types::PaymentId = "Transact_ReferenceID", },
                    body: [],
                    response: $crate::payments::CommandReceipt,
                }
                /// Refunds all or part of a cleared payment, as a new credit with a
                /// payment ID of its own.
                refund => RefundPayment {
                    command: "ECheck.Refund",
                    scope: Merchant,
                    extra: [],
                    // The only command whose documentation spells it `TransAct_`.
                    fields: { payment_id: $crate::types::PaymentId = "TransAct_ReferenceID", },
                    body: [$crate::payments::RefundRequest],
                    response: $crate::payments::PaymentCreated,
                }
            }

            /// Bank account verification.
            insights => Insights {
                /// Checks that a bank account is open and in good standing.
                account_status => VerifyAccount {
                    command: "ECheck.VerifyPlus",
                    scope: Merchant,
                    extra: [],
                    fields: {},
                    body: [$crate::insights::AccountStatusRequest],
                    response: $crate::insights::AccountVerification,
                }
                /// Checks a bank account's status, then that the person or business
                /// named owns it.
                account_owner => VerifyOwner {
                    command: "ECheck.VerifyPlus",
                    scope: Merchant,
                    extra: [("OwnerAuth", "On")],
                    fields: {},
                    body: [$crate::insights::OwnerAuthRequest],
                    response: $crate::insights::OwnerVerification,
                }
            }

            /// Settlement reports, across the platform's merchants.
            settlements => Settlements {
                /// Lists the settlements of a date range.
                list => ListSettlements {
                    command: "ECheckReports.GetSettlements",
                    scope: Platform,
                    extra: [],
                    fields: {},
                    body: [$crate::settlements::SettlementQuery],
                    response: ::std::vec::Vec<$crate::settlements::Settlement>,
                }
                /// Lists a page of the payments in a settlement.
                details => SettlementDetails {
                    command: "ECheckReports.GetSettlementItems",
                    scope: Platform,
                    extra: [],
                    fields: {},
                    body: [$crate::settlements::SettlementDetailsQuery],
                    response: $crate::settlements::SettlementDetailsPage,
                }
            }

            /// Event tracking, across the platform's merchants.
            events => Events {
                /// Lists the payment events of a day.
                query => TrackEvents {
                    command: "ECheckReports.StatusTrackingQuery",
                    scope: Platform,
                    extra: [],
                    fields: {},
                    body: [$crate::events::EventQuery],
                    response: $crate::events::EventBatch,
                }
            }
        }
    };
}

/// Generates a module per resource, with each command's type and its
/// [`Operation`](crate::Operation) impl.
macro_rules! impl_operations {
    ($(
        $(#[doc = $rdoc:literal])*
        $resource:ident => $handle:ident {$(
            $(#[doc = $doc:literal])*
            $method:ident => $name:ident {
                command: $command:literal,
                scope: $scope:ident,
                extra: [$(($ename:literal, $evalue:literal)),* $(,)?],
                fields: {$( $field:ident: $fty:ty = $fwire:literal, )*},
                body: [$($body:ty)?],
                response: $response:ty,
            }
        )*}
    )*) => {$(
        $(#[doc = $rdoc])*
        pub mod $resource {$(
            $(#[doc = $doc])*
            #[doc = ""]
            #[doc = concat!("The gateway's `", $command, "` command.")]
            #[derive(Clone, Debug)]
            pub struct $name<'a> {
                $($field: $fty,)*
                $(body: &'a $body,)?
                marker: ::core::marker::PhantomData<&'a ()>,
            }

            impl<'a> $name<'a> {
                #[doc = concat!("The `", stringify!($resource), ".", stringify!($method), "` command.")]
                #[allow(clippy::new_without_default, clippy::redundant_field_names)]
                pub fn new(
                    $($field: impl ::core::convert::Into<$fty>,)*
                    $(body: &'a $body,)?
                ) -> Self {
                    Self {
                        $($field: $field.into(),)*
                        $(body: $crate::operations::impl_operations!(@id body $body),)?
                        marker: ::core::marker::PhantomData,
                    }
                }
            }

            impl $crate::operation::Operation for $name<'_> {
                type Response = $response;
                const NAME: &'static str = concat!(stringify!($resource), ".", stringify!($method));
                const COMMAND: &'static str = $command;
                const SCOPE: $crate::operation::Scope = $crate::operation::Scope::$scope;
                const EXTRA: &'static [(&'static str, &'static str)] = &[$(($ename, $evalue)),*];

                fn validate(&self) -> ::core::result::Result<(), $crate::validate::ValidationError> {
                    #[allow(unused_mut)]
                    let mut violations = ::std::vec::Vec::new();
                    $($crate::operation::check_id(&self.$field, stringify!($field), &mut violations);)*
                    $(
                        $crate::operations::impl_operations!(@check self violations $body);
                    )?
                    $crate::operation::finish(violations)
                }

                #[allow(unused_variables)]
                fn write_form(&self, form: &mut $crate::form::FormWriter) {
                    $(form.push($fwire, &$crate::form::FormValue::form_value(&self.$field));)*
                    $(
                        <$body as $crate::form::ToForm>::write_form(self.body, form, "");
                    )?
                }

                #[allow(unused_variables)]
                fn form_fields(out: &mut ::std::vec::Vec<$crate::form::FieldMeta>) {
                    $(out.push($crate::form::FieldMeta::new($fwire.into(), true, None, false));)*
                    $(<$body as $crate::form::ToForm>::form_fields("", out);)?
                }

                fn decode(
                    response: &$crate::transport::HttpResponse,
                ) -> ::core::result::Result<Self::Response, $crate::Error> {
                    <$response as $crate::operation::Decode>::decode(response)
                }
            }
        )*}
    )*};

    // Returns the caller's `body` binding, so that its hygiene matches.
    (@id $value:ident $body:ty) => { $value };
    (@check $self:ident $violations:ident $body:ty) => {
        $crate::operation::check_body::<$body>($self.body, &mut $violations)
    };
}

pub(crate) use impl_operations;
// The clients generate their resource handles from the table in Stage 4.
#[allow(unused_imports)]
pub(crate) use operation_table;

operation_table!(impl_operations);

#[cfg(test)]
mod tests {
    use crate::operation::{Operation, Scope};

    /// Every command's name, gateway command and scope.
    macro_rules! names {
        ($(
            $(#[doc = $rdoc:literal])*
            $resource:ident => $handle:ident {$(
                $(#[doc = $doc:literal])*
                $method:ident => $name:ident { $($rest:tt)* }
            )*}
        )*) => {
            vec![$($((
                <crate::operations::$resource::$name<'static> as Operation>::NAME,
                <crate::operations::$resource::$name<'static> as Operation>::COMMAND,
                <crate::operations::$resource::$name<'static> as Operation>::SCOPE,
            ),)*)*]
        };
    }

    #[test]
    fn the_table_lists_every_command_once() {
        let commands = operation_table!(names);
        assert_eq!(commands.len(), 11);
        let mut names: Vec<&str> = commands.iter().map(|(name, _, _)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 11);
        let platform: Vec<&str> = commands
            .iter()
            .filter(|(_, _, scope)| *scope == Scope::Platform)
            .map(|(name, _, _)| *name)
            .collect();
        assert_eq!(
            platform,
            ["settlements.list", "settlements.details", "events.query"]
        );
    }
}
