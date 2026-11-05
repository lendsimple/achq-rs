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
/// Each resource names the scope of its commands: `Merchant` resources are on a
/// client's [`Merchant`](crate::Merchant), `Platform` ones on the
/// [`Client`](crate::Client). Each command names:
/// - `command`: the gateway's `Command` value;
/// - `scope`: `Merchant` (needs a merchant's credentials) or `Platform`;
/// - `extra`: fields every request of the command sends;
/// - `fields`: ID fields, which the operation type takes before its body;
/// - `body`: the request type, if any: `form T` for the gateway, `json T` for the
///   API;
/// - `response`: what the command returns.
macro_rules! operation_table {
    ($callback:ident) => {
        $callback! {
            /// Tokens for bank accounts.
            tokens => Tokens in Merchant {
                /// Creates an [`AchqToken`](crate::types::AchqToken) for a bank
                /// account, to charge or pay it later without its numbers.
                create => CreateToken {
                    command: "ECheck.CreateACHQToken",
                    endpoint: gateway,
                    scope: Merchant,
                    extra: [],
                    fields: {},
                    body: [form $crate::tokens::TokenRequest],
                    response: $crate::tokens::TokenCreated,
                }
            }

            /// Payments: creating, changing, cancelling, holding and refunding
            /// them.
            payments => Payments in Merchant {
                /// Creates a payment from or to a bank account, or the account a
                /// token stands for.
                create => CreatePayment {
                    command: "ECheck.ProcessPayment",
                    endpoint: gateway,
                    scope: Merchant,
                    extra: [],
                    fields: {},
                    body: [form $crate::payments::PaymentRequest],
                    response: $crate::payments::PaymentCreated,
                }
                /// Changes a scheduled payment. The receipt's ID identifies the
                /// update, not the payment.
                update => UpdatePayment {
                    command: "ECheck.Update",
                    endpoint: gateway,
                    scope: Merchant,
                    extra: [],
                    fields: { payment_id: $crate::types::PaymentId = "Transact_ReferenceID", },
                    body: [form $crate::payments::PaymentUpdate],
                    response: $crate::payments::CommandReceipt,
                }
                /// Cancels a scheduled payment.
                cancel => CancelPayment {
                    command: "ECheck.Void",
                    endpoint: gateway,
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
                    endpoint: gateway,
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
                    endpoint: gateway,
                    scope: Merchant,
                    extra: [],
                    // The only command whose documentation spells it `TransAct_`.
                    fields: { payment_id: $crate::types::PaymentId = "TransAct_ReferenceID", },
                    body: [form $crate::payments::RefundRequest],
                    response: $crate::payments::PaymentCreated,
                }
            }

            /// Bank account verification.
            insights => Insights in Merchant {
                /// Checks that a bank account is open and in good standing.
                account_status => VerifyAccount {
                    command: "ECheck.VerifyPlus",
                    endpoint: gateway,
                    scope: Merchant,
                    extra: [],
                    fields: {},
                    body: [form $crate::insights::AccountStatusRequest],
                    response: $crate::insights::AccountVerification,
                }
                /// Checks a bank account's status, then that the person or business
                /// named owns it.
                account_owner => VerifyOwner {
                    command: "ECheck.VerifyPlus",
                    endpoint: gateway,
                    scope: Merchant,
                    extra: [("OwnerAuth", "On")],
                    fields: {},
                    body: [form $crate::insights::OwnerAuthRequest],
                    response: $crate::insights::OwnerVerification,
                }
            }

            /// Settlement reports, across the platform's merchants.
            settlements => Settlements in Platform {
                /// Lists the settlements of a date range.
                list => ListSettlements {
                    command: "ECheckReports.GetSettlements",
                    endpoint: gateway,
                    scope: Platform,
                    extra: [],
                    fields: {},
                    body: [form $crate::settlements::SettlementQuery],
                    response: ::std::vec::Vec<$crate::settlements::Settlement>,
                }
                /// Lists a page of the payments in a settlement.
                details => SettlementDetails {
                    command: "ECheckReports.GetSettlementItems",
                    endpoint: gateway,
                    scope: Platform,
                    extra: [],
                    fields: {},
                    body: [form $crate::settlements::SettlementDetailsQuery],
                    response: $crate::settlements::SettlementDetailsPage,
                }
            }

            /// The onboarding API: enrolling merchants with ACHQ.
            #[cfg(feature = "onboarding")]
            onboarding => Onboarding in Platform {
                /// Submits a merchant's application for an ACHQ account.
                create_merchant => CreateMerchant {
                    command: "POST /onboarding",
                    endpoint: api "/onboarding",
                    scope: Platform,
                    extra: [],
                    fields: {},
                    body: [json $crate::onboarding::MerchantApplication],
                    response: $crate::onboarding::MerchantOnboarded,
                }
                /// Creates a fee profile, to assign to merchants by its ID.
                create_fee_profile => CreateFeeProfile {
                    command: "POST /onboarding/fees",
                    endpoint: api "/onboarding/fees",
                    scope: Platform,
                    extra: [],
                    fields: {},
                    body: [json $crate::onboarding::FeeSchedule],
                    response: $crate::onboarding::FeeProfileCreated,
                }
            }

            /// Event tracking, across the platform's merchants.
            events => Events in Platform {
                /// Lists the payment events of a day.
                query => TrackEvents {
                    command: "ECheckReports.StatusTrackingQuery",
                    endpoint: gateway,
                    scope: Platform,
                    extra: [],
                    fields: {},
                    body: [form $crate::events::EventQuery],
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
        $(#[cfg($cfg:meta)])?
        $resource:ident => $handle:ident in $resource_scope:ident {$(
            $(#[doc = $doc:literal])*
            $method:ident => $name:ident {
                command: $command:literal,
                endpoint: $ekind:ident $($epath:literal)?,
                scope: $scope:ident,
                extra: [$(($ename:literal, $evalue:literal)),* $(,)?],
                fields: {$( $field:ident: $fty:ty = $fwire:literal, )*},
                body: [$($bkind:ident $body:ty)?],
                response: $response:ty,
            }
        )*}
    )*) => {$(
        $(#[doc = $rdoc])*
        $(#[cfg($cfg)])?
        $(#[cfg_attr(docsrs, doc(cfg($cfg)))])?
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
                const ENDPOINT: $crate::operation::Endpoint =
                    $crate::operations::impl_operations!(@endpoint $ekind $($epath)?);
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
                        $crate::operations::impl_operations!(@write_form $bkind $body, self.body, form);
                    )?
                }

                #[allow(unused_variables)]
                fn form_fields(out: &mut ::std::vec::Vec<$crate::form::FieldMeta>) {
                    $(out.push($crate::form::FieldMeta::new($fwire.into(), true, None, false));)*
                    $($crate::operations::impl_operations!(@form_fields $bkind $body, out);)?
                }

                fn json_body(
                    &self,
                ) -> ::core::option::Option<::core::result::Result<::std::vec::Vec<u8>, ::serde_json::Error>> {
                    $crate::operations::impl_operations!(@json_body self [$($bkind $body)?])
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
    (@endpoint gateway) => { $crate::operation::Endpoint::Gateway };
    (@endpoint api $path:literal) => { $crate::operation::Endpoint::Api($path) };
    (@write_form form $body:ty, $value:expr, $form:ident) => {
        <$body as $crate::form::ToForm>::write_form($value, $form, "")
    };
    (@write_form json $body:ty, $value:expr, $form:ident) => {};
    (@form_fields form $body:ty, $out:ident) => {
        <$body as $crate::form::ToForm>::form_fields("", $out)
    };
    (@form_fields json $body:ty, $out:ident) => {};
    (@json_body $self:ident []) => { ::core::option::Option::None };
    (@json_body $self:ident [form $body:ty]) => { ::core::option::Option::None };
    (@json_body $self:ident [json $body:ty]) => {
        ::core::option::Option::Some(::serde_json::to_vec($self.body))
    };
    (@check $self:ident $violations:ident $body:ty) => {
        $crate::operation::check_body::<$body>($self.body, &mut $violations)
    };
}

/// Generates the clients' resource handles: an accessor per resource on the
/// client's [`Merchant`](crate::Merchant) or [`Client`](crate::Client), by the
/// resource's scope, and a handle with a method per command.
macro_rules! resource_handles {
    (
        [$client:ident, $merchant:ident, $transport:path, $mode:ident]
        $(
            $(#[doc = $rdoc:literal])*
            $(#[cfg($cfg:meta)])?
            $resource:ident => $handle:ident in $scope:ident {$(
                $(#[doc = $doc:literal])*
                $method:ident => $name:ident {
                    command: $command:literal,
                    endpoint: $ekind:ident $($epath:literal)?,
                    scope: $op_scope:ident,
                    extra: [$($extra:tt)*],
                    fields: {$( $field:ident: $fty:ty = $fwire:literal, )*},
                    body: [$($bkind:ident $body:ty)?],
                    response: $response:ty,
                }
            )*}
        )*
    ) => {$(
        $(#[cfg($cfg)])?
        $crate::operations::resource_handles!(
            @accessor $scope [$client, $merchant, $transport] $resource $handle
        );

        $(#[doc = $rdoc])*
        $(#[cfg($cfg)])?
        $(#[cfg_attr(docsrs, doc(cfg($cfg)))])?
        #[doc = ""]
        #[doc = concat!(
            "From [`", $crate::operations::resource_handles!(@owner_name $scope $client $merchant),
            "::", stringify!($resource), "`]."
        )]
        pub struct $handle<'a, T> {
            owner: &'a $crate::operations::resource_handles!(@owner $scope $client $merchant T),
        }

        $(#[cfg($cfg)])?
        impl<T> ::core::clone::Clone for $handle<'_, T> {
            fn clone(&self) -> Self {
                *self
            }
        }

        $(#[cfg($cfg)])?
        impl<T> ::core::marker::Copy for $handle<'_, T> {}

        $(#[cfg($cfg)])?
        impl<T> ::core::fmt::Debug for $handle<'_, T> {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.debug_struct(stringify!($handle)).finish_non_exhaustive()
            }
        }

        $(#[cfg($cfg)])?
        impl<T: $transport> $handle<'_, T> {$(
            $crate::operations::resource_handles!(
                @method $mode
                [
                    $(#[doc = $doc])*
                    #[doc = ""]
                    #[doc = concat!(
                        "Sends [`", stringify!($resource), "::", stringify!($name),
                        "`](crate::operations::", stringify!($resource), "::",
                        stringify!($name), ")."
                    )]
                ]
                $resource $name $method [$($field: $fty),*] [$($body)?] $response
            );
        )*}
    )*};

    (@owner Merchant $client:ident $merchant:ident $t:ident) => { $merchant<$t> };
    (@owner Platform $client:ident $merchant:ident $t:ident) => { $client<$t> };
    (@owner_name Merchant $client:ident $merchant:ident) => { stringify!($merchant) };
    (@owner_name Platform $client:ident $merchant:ident) => { stringify!($client) };

    (@accessor Merchant [$client:ident, $merchant:ident, $transport:path] $resource:ident $handle:ident) => {
        impl<T: $transport> $merchant<T> {
            #[doc = concat!("The `", stringify!($resource), "` commands: see [`", stringify!($handle), "`].")]
            pub fn $resource(&self) -> $handle<'_, T> {
                $handle { owner: self }
            }
        }
    };
    (@accessor Platform [$client:ident, $merchant:ident, $transport:path] $resource:ident $handle:ident) => {
        impl<T: $transport> $client<T> {
            #[doc = concat!("The `", stringify!($resource), "` commands: see [`", stringify!($handle), "`].")]
            pub fn $resource(&self) -> $handle<'_, T> {
                $handle { owner: self }
            }
        }
    };

    (@method async [$($doc:tt)*] $resource:ident $name:ident $method:ident
        [$($field:ident: $fty:ty),*] [$($body:ty)?] $response:ty) => {
        $($doc)*
        pub async fn $method(
            &self,
            $($field: impl ::core::convert::Into<$fty>,)*
            $(body: &$body,)?
        ) -> ::core::result::Result<$response, $crate::Error> {
            let op = $crate::operations::$resource::$name::new(
                $($field,)*
                $($crate::operations::impl_operations!(@id body $body),)?
            );
            self.owner.execute(&op).await
        }
    };
    (@method blocking [$($doc:tt)*] $resource:ident $name:ident $method:ident
        [$($field:ident: $fty:ty),*] [$($body:ty)?] $response:ty) => {
        $($doc)*
        pub fn $method(
            &self,
            $($field: impl ::core::convert::Into<$fty>,)*
            $(body: &$body,)?
        ) -> ::core::result::Result<$response, $crate::Error> {
            let op = $crate::operations::$resource::$name::new(
                $($field,)*
                $($crate::operations::impl_operations!(@id body $body),)?
            );
            self.owner.execute(&op)
        }
    };
}

/// The async client's handles.
macro_rules! async_handles {
    ($($table:tt)*) => {
        $crate::operations::resource_handles!(
            [Client, Merchant, $crate::transport::Transport, async] $($table)*
        );
    };
}

/// The blocking client's handles.
macro_rules! blocking_handles {
    ($($table:tt)*) => {
        $crate::operations::resource_handles!(
            [Client, Merchant, $crate::blocking::Transport, blocking] $($table)*
        );
    };
}

pub(crate) use {
    async_handles, blocking_handles, impl_operations, operation_table, resource_handles,
};

operation_table!(impl_operations);

#[cfg(test)]
mod tests {
    use crate::operation::{Operation, Scope};

    /// Every command's name, gateway command and scope.
    macro_rules! names {
        ($(
            $(#[doc = $rdoc:literal])*
            $(#[cfg($cfg:meta)])?
            $resource:ident => $handle:ident in $resource_scope:ident {$(
                $(#[doc = $doc:literal])*
                $method:ident => $name:ident { $($rest:tt)* }
            )*}
        )*) => {{
            #[allow(unused_mut)]
            let mut commands = Vec::new();
            $(
                $(#[cfg($cfg)])?
                {
                    $(commands.push((
                        <crate::operations::$resource::$name<'static> as Operation>::NAME,
                        <crate::operations::$resource::$name<'static> as Operation>::COMMAND,
                        <crate::operations::$resource::$name<'static> as Operation>::SCOPE,
                        Scope::$resource_scope,
                    ));)*
                }
            )*
            commands
        }};
    }

    #[test]
    fn the_table_lists_every_command_once() {
        let commands = operation_table!(names);
        let expected = if cfg!(feature = "onboarding") { 13 } else { 11 };
        assert_eq!(commands.len(), expected);
        let mut names: Vec<&str> = commands.iter().map(|(name, ..)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), expected);
        let platform: Vec<&str> = commands
            .iter()
            .filter(|(_, _, scope, _)| *scope == Scope::Platform)
            .map(|(name, ..)| *name)
            .collect();
        let mut expected = vec!["settlements.list", "settlements.details"];
        if cfg!(feature = "onboarding") {
            expected.extend([
                "onboarding.create_merchant",
                "onboarding.create_fee_profile",
            ]);
        }
        expected.push("events.query");
        assert_eq!(platform, expected);
    }

    #[test]
    fn every_command_has_its_resource_scope() {
        // The clients put a resource on the `Merchant` or the `Client` by its
        // scope, so every command in it must need the same credentials.
        for (name, _, scope, resource_scope) in operation_table!(names) {
            assert_eq!(scope, resource_scope, "{name}");
        }
    }
}
