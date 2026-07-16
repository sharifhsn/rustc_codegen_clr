using System.Runtime.InteropServices;
using System;
using UnityEngine;

public static class RustUnityBridge
{
    [DllImport("unity_native_sample", EntryPoint = "rust_native_multiply")]
    static extern int RustNativeMultiply(int left, int right);

    public static int SampleValue()
    {
        return Rust.Unity.Sample.Exports.sample_value();
    }

    public static int NativeValue() => RustNativeMultiply(6, 7);

    public static int ManagedDomainValue()
    {
        var damage = Rust.Unity.Sample.Exports.CalculateDamage(30, 9, true);
        var sum = Rust.Unity.Sample.Exports.SumScores(new[] { 3, 5, 7 });
        var scaled = Rust.Unity.Sample.Exports.ScaleScores(new[] { 2, 4 }, 3);
        var description = Rust.Unity.Sample.Exports.DescribePlayer("Ada", 21);
        int? doubled = Rust.Unity.Sample.Exports.DoubleIfPresent(8);
        int? missing = Rust.Unity.Sample.Exports.DoubleIfPresent(null);
        var state = Rust.Unity.Sample.Exports.RoundtripState(EncounterState.Active);
        var transformed = Rust.Unity.Sample.Exports.InvokeTransform(new Func<int, int>(value => value + 5), 8);
        var turn = Rust.Unity.Sample.Exports.ResolveTacticsTurn(new[] { 12, 15, 20 }, 1, 4);
        var replayHash = Rust.Unity.Sample.Exports.ReplayHash(turn);
        var snapshot = new CombatSnapshot { Score = sum, Active = true };

        if (damage != 42 || scaled.Length != 2 || scaled[0] != 6 || scaled[1] != 12 ||
            description != "ADA" || doubled != 16 || missing.HasValue ||
            state != EncounterState.Active || transformed != 13 ||
            turn.Length != 3 || turn[0] != 12 || turn[1] != 8 || turn[2] != 20 ||
            replayHash != 518247 ||
            snapshot.Score != 15 || !snapshot.Active)
            throw new InvalidOperationException("managed Rust domain acceptance failed");

        return damage + snapshot.Score + scaled[1] + doubled.Value + transformed;
    }
}

public sealed class RustUnityProbe : MonoBehaviour
{
    void Start()
    {
        Debug.Log($"RUST_UNITY_OK={RustUnityBridge.SampleValue()},DOMAIN={RustUnityBridge.ManagedDomainValue()},NATIVE={RustUnityBridge.NativeValue()}");
        Application.Quit(0);
    }
}
