#![feature(adt_const_params, unsized_const_params)]
#![allow(internal_features, incomplete_features)]

use dotnet_macros::{dotnet_dto, dotnet_enum, dotnet_export};
use mycorrhiza::delegate::Func1;
use mycorrhiza::system::DotNetString;

#[dotnet_enum(name = "EncounterState")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum EncounterState {
    Preparing = 0,
    Active = 1,
    Complete = 2,
}

/// Ordinary managed DTO that Unity-side C# can construct and mutate without serialization glue.
#[dotnet_dto]
pub struct CombatSnapshot {
    pub score: i32,
    pub active: bool,
}

/// Small deterministic domain call used by the Unity edge fixture.
#[dotnet_export(name = "sample_value")]
pub fn sample_value() -> i32 {
    42
}

/// Small gameplay calculation with deterministic saturation policy.
#[dotnet_export(name = "CalculateDamage")]
pub fn calculate_damage(base: i32, armor: i32, critical: bool) -> i32 {
    let reduced = base.saturating_sub(armor).max(0);
    if critical {
        reduced.saturating_mul(2)
    } else {
        reduced
    }
}

/// A normal C# `int[]` enters Rust as a `Vec<i32>`.
#[dotnet_export(name = "SumScores")]
pub fn sum_scores(scores: Vec<i32>) -> i32 {
    scores.into_iter().sum()
}

/// A Rust `Vec<i32>` returns to C# as a normal managed `int[]`.
#[dotnet_export(name = "ScaleScores")]
pub fn scale_scores(scores: Vec<i32>, factor: i32) -> Vec<i32> {
    scores
        .into_iter()
        .map(|score| score.saturating_mul(factor))
        .collect()
}

/// Managed strings cross the facade without pointer/length glue. The Rust implementation can
/// operate on the CLR string directly, avoiding a redundant UTF-16 -> UTF-8 -> UTF-16 round trip.
#[dotnet_export(name = "DescribePlayer")]
pub fn describe_player(name: DotNetString, score: i32) -> DotNetString {
    if score >= 20 { name.to_upper() } else { name }
}

/// Rust `Option<i32>` is projected as C# `int?`.
#[dotnet_export(name = "DoubleIfPresent")]
pub fn double_if_present(value: Option<i32>) -> Option<i32> {
    value.map(|number| number.saturating_mul(2))
}

/// A genuine CLR enum remains strongly typed on both sides.
#[dotnet_export(name = "RoundtripState", enums(EncounterState))]
pub fn roundtrip_state(state: EncounterState) -> EncounterState {
    state
}

/// Unity-side delegates can be invoked directly by managed Rust.
#[dotnet_export(name = "InvokeTransform")]
pub fn invoke_transform(transform: Func1<i32, i32>, value: i32) -> i32 {
    transform.invoke(value)
}

/// Resolve one deterministic tactics turn in managed Rust. Unity supplies ordinary managed
/// arrays; Rust owns the rule that focus fire gains one point per living allied unit.
#[dotnet_export(name = "ResolveTacticsTurn")]
pub fn resolve_tactics_turn(mut health: Vec<i32>, focus_target: i32, power: i32) -> Vec<i32> {
    let living = health.iter().filter(|value| **value > 0).count() as i32;
    if let Some(target) = usize::try_from(focus_target)
        .ok()
        .and_then(|index| health.get_mut(index))
    {
        *target = target.saturating_sub(power.max(0).saturating_add(living)).max(0);
    }
    health
}

/// Stable replay checksum used by EditMode, PlayMode, Mono, and IL2CPP acceptance gates.
#[dotnet_export(name = "ReplayHash")]
pub fn replay_hash(values: Vec<i32>) -> i32 {
    values
        .into_iter()
        .fold(17_i32, |hash, value| hash.wrapping_mul(31).wrapping_add(value))
}
