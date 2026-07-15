#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features, dead_code)]

use dotnet_macros::{dotnet_class, dotnet_dto, dotnet_methods, dotnet_record, dotnet_value};
use mycorrhiza::bcl::dateonly::DateOnly;
use mycorrhiza::bcl::decimal::Decimal;
use mycorrhiza::bcl::datetime::DateTime;
use mycorrhiza::bcl::datetimeoffset::DateTimeOffset;
use mycorrhiza::bcl::guid::Guid;
use mycorrhiza::cancellation::CancellationToken;
use mycorrhiza::collections::{List, MutableDictionary, MutableList, ReadOnlyList};
use mycorrhiza::enumerate::ManagedEnumerable;
use mycorrhiza::intrinsics::ManagedArray;
use mycorrhiza::memory::{Memory, ReadOnlyMemory};
use mycorrhiza::managed_option::ManagedOption;
use mycorrhiza::nullable::{self, Nullable};
use mycorrhiza::progress::Progress;
use mycorrhiza::system::MString;
use mycorrhiza::task::{Task, ValueTask, await_unit, future_to_value_task_unit};
use std::sync::atomic::{AtomicI32, Ordering};

static CANCELLATION_CALLBACKS: AtomicI32 = AtomicI32::new(0);
static DISPOSED_RESOURCES: AtomicI32 = AtomicI32::new(0);

struct NativeResourceState;

impl Drop for NativeResourceState {
    fn drop(&mut self) {
        DISPOSED_RESOURCES.fetch_add(1, Ordering::SeqCst);
    }
}

/// A typed CLR DTO. The attribute is only class/property-generation sugar; no serializer is
/// involved in this fixture.
#[dotnet_dto]
pub struct InvoiceDto {
    amount: Decimal,
    date: Nullable<DateOnly>,
    memo: MString,
}

/// Immutable record-shaped application data: primary constructor plus getter-only properties.
#[dotnet_record]
pub struct RiskScenario {
    scenario_id: Guid,
    name: MString,
    as_of: DateTimeOffset,
    calculated_at: DateTime,
    shock_percent: f64,
    horizon_days: i32,
}

/// Value semantics are explicit rather than guessed from the Rust layout.
#[dotnet_value]
pub struct RatePoint {
    tenor_days: i32,
    rate: f64,
}

/// A managed `IDisposable` whose opaque token owns real Rust heap state. The generated lifecycle
/// contract lets C# use `using`; the Rust implementation retains explicit ownership policy.
#[dotnet_class(field_setters = true)]
pub struct NativeResource {
    token: usize,
}

#[dotnet_methods(disposable, async_disposable)]
impl NativeResource {
    pub fn create() -> NativeResourceHandle {
        let token = Box::into_raw(Box::new(NativeResourceState)) as usize;
        NativeResourceHandle::ctor1(token)
    }

    pub fn is_disposed(this: NativeResourceHandle) -> bool {
        this.instance0::<"read_token", usize>() == 0
    }

    pub fn disposed_count() -> i32 {
        DISPOSED_RESOURCES.load(Ordering::SeqCst)
    }

    pub fn dispose(this: NativeResourceHandle) {
        let token = this.instance0::<"read_token", usize>();
        if token == 0 {
            return;
        }
        // Invalidate the managed object before running Rust Drop, so repeated sequential Dispose
        // calls are harmless even if cleanup policy later grows more complex.
        this.instance1::<"set_token", usize, ()>(0);
        unsafe { drop(Box::from_raw(token as *mut NativeResourceState)) };
    }

    pub fn dispose_async(this: NativeResourceHandle) -> ValueTask {
        // Detach managed identity from native ownership before suspension. The future captures only
        // the native token, never a raw GC reference; repeated Dispose/DisposeAsync sees zero.
        let token = this.instance0::<"read_token", usize>();
        if token != 0 {
            this.instance1::<"set_token", usize, ()>(0);
        }
        future_to_value_task_unit(async move {
            // Exercise a real Pending -> Ready managed Task before releasing the Rust state.
            await_unit(Task::delay(1)).await;
            if token != 0 {
                unsafe { drop(Box::from_raw(token as *mut NativeResourceState)) };
            }
        })
    }
}

#[dotnet_class]
pub struct InvoiceFacade {}

#[dotnet_methods]
impl InvoiceFacade {
    /// Sum a genuine managed array of generated CLR value types without copying it into Rust heap
    /// storage or routing through JSON.
    pub fn sum_rate_points(points: ManagedArray<RatePointHandle>) -> f64 {
        let mut total = 0.0;
        for index in 0..points.len() {
            total += points.get(index).vt_field::<"rate", f64>();
        }
        total
    }

    /// Return the same managed value-type array, preserving its CLR identity and allocation.
    pub fn echo_rate_points(points: ManagedArray<RatePointHandle>) -> ManagedArray<RatePointHandle> {
        points
    }

    /// Reference DTO arrays stay in GC-owned storage; no `Vec<InvoiceDtoHandle>` is constructed.
    pub fn count_invoices(invoices: ManagedArray<InvoiceDtoHandle>) -> i32 {
        invoices.len()
    }

    pub fn echo_invoices(
        invoices: ManagedArray<InvoiceDtoHandle>,
    ) -> ManagedArray<InvoiceDtoHandle> {
        invoices
    }

    /// Accept any CLR implementation of `IReadOnlyList<RatePoint>`, including an array.
    pub fn sum_readonly_rates(points: ReadOnlyList<RatePointHandle>) -> f64 {
        let mut total = 0.0;
        for index in 0..points.len() {
            total += points.at(index).vt_field::<"rate", f64>();
        }
        total
    }

    pub fn echo_readonly_invoices(
        invoices: ReadOnlyList<InvoiceDtoHandle>,
    ) -> ReadOnlyList<InvoiceDtoHandle> {
        invoices
    }

    /// Consume the framework-native retained-buffer abstraction directly; no pinning or pointer
    /// pair appears in the public API.
    pub fn sum_readonly_memory(values: ReadOnlyMemory<f64>) -> f64 {
        let mut copy = Memory::from_slice(&vec![0.0; values.len() as usize]);
        values.copy_to(&mut copy);
        copy.to_vec().into_iter().sum()
    }

    /// Round-trip a `ReadOnlyMemory<T>` value while preserving its managed backing storage.
    pub fn echo_readonly_memory(values: ReadOnlyMemory<f64>) -> ReadOnlyMemory<f64> {
        values
    }

    /// Mutate a caller-owned `Memory<T>` view and return the same view.
    pub fn fill_memory(mut values: Memory<i32>, value: i32) -> Memory<i32> {
        values.fill(value);
        values
    }

    /// Work against the familiar mutable interface rather than requiring a concrete BCL type.
    pub fn update_list(mut values: MutableList<i32>) -> MutableList<i32> {
        let _ = values.set(0, 41);
        values.push(42);
        values
    }

    pub fn update_dictionary(
        mut values: MutableDictionary<i32, f64>,
    ) -> MutableDictionary<i32, f64> {
        let base = values.get(7).unwrap_or(0.0);
        values.insert(8, base * 2.0);
        values
    }

    /// Produce the standard lazy-consumer interface from an ordinary managed list.
    pub fn produce_sequence() -> ManagedEnumerable<i32> {
        List::from_slice(&[2, 3, 5, 7]).into_enumerable()
    }

    pub fn sum_sequence(values: ManagedEnumerable<i32>) -> i32 {
        values.iter().sum()
    }

    pub fn has_invoice(value: ManagedOption<InvoiceDtoHandle>) -> bool {
        value.is_some()
    }

    pub fn echo_optional_invoice(
        value: ManagedOption<InvoiceDtoHandle>,
    ) -> ManagedOption<InvoiceDtoHandle> {
        value
    }

    pub fn absent_invoice() -> ManagedOption<InvoiceDtoHandle> {
        ManagedOption::from_raw(InvoiceDtoHandle::null())
    }

    /// Scoped numeric slices become stack-only managed spans with no allocation or retention.
    pub fn sum_span(values: &[i32]) -> i32 {
        values.iter().sum()
    }

    pub fn scale_span(values: &mut [f64], factor: f64) {
        for value in values {
            *value *= factor;
        }
    }

    pub fn annualized_rate(point: RatePointHandle) -> f64 {
        let days = point.vt_field::<"tenorDays", i32>();
        let rate = point.vt_field::<"rate", f64>();
        rate * 365.0 / days as f64
    }

    #[dotnet(name = "ObserveCancellation")]
    pub fn observe_cancellation(token: CancellationToken) -> bool {
        token.is_cancellation_requested()
    }

    #[dotnet(name = "RegisterCanceledCallback")]
    pub fn register_canceled_callback(token: CancellationToken) -> i32 {
        CANCELLATION_CALLBACKS.store(0, Ordering::SeqCst);
        let registration = token.register(|| {
            CANCELLATION_CALLBACKS.fetch_add(1, Ordering::SeqCst);
        });
        registration.dispose();
        CANCELLATION_CALLBACKS.load(Ordering::SeqCst)
    }

    #[dotnet(name = "ThrowIfCanceled")]
    pub fn throw_if_canceled(token: CancellationToken) {
        token.throw_if_cancellation_requested();
    }

    #[dotnet(name = "ReportProgress")]
    pub fn report_progress(progress: Progress<i32>, value: i32) {
        progress.report(value);
    }

    #[dotnet(name = "CreateScenario")]
    pub fn create_scenario() -> RiskScenarioHandle {
        RiskScenario::new_managed(
            Guid::parse(MString::from("7eb9b72f-4f65-4ce6-9fbb-ea40680f75a8")),
            MString::from("rate-up"),
            DateTimeOffset::parse_str("2026-07-15T12:30:00-04:00"),
            DateTime::parse_str("2026-07-15T16:31:00Z"),
            1.25,
            30,
        )
    }

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
