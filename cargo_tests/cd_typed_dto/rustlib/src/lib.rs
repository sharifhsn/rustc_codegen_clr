#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code)]

use dotnet_macros::{dotnet_class, dotnet_dto, dotnet_methods};
use mycorrhiza::bcl::dateonly::DateOnly;
use mycorrhiza::bcl::decimal::Decimal;
use mycorrhiza::nullable::{self, Nullable};
use mycorrhiza::system::MString;

/// A typed CLR DTO. The attribute is only class/property-generation sugar; no serializer is
/// involved in this fixture.
#[dotnet_dto]
pub struct InvoiceDto {
    amount: Decimal,
    date: Nullable<DateOnly>,
    memo: MString,
}

#[dotnet_class]
pub struct InvoiceFacade {}

#[dotnet_methods]
impl InvoiceFacade {
    #[dotnet(name = "CreateWithDate")]
    pub fn create_with_date(day_number: i32) -> InvoiceDtoHandle {
        let amount = Decimal::vt_static1::<"Parse", MString, Decimal>(MString::from("123.4500"));
        let date = DateOnly::vt_static1::<"FromDayNumber", i32, DateOnly>(day_number);
        InvoiceDto::new_managed(amount, nullable::some(date), MString::from("from-rust"))
    }

    #[dotnet(name = "CreateWithoutDate")]
    pub fn create_without_date() -> InvoiceDtoHandle {
        let amount = Decimal::vt_static1::<"Parse", MString, Decimal>(MString::from("7.00"));
        InvoiceDto::new_managed(
            amount,
            nullable::none::<DateOnly>(),
            MString::from("no-date"),
        )
    }
}
