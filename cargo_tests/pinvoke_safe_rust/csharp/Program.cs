static void Check(bool condition, string message)
{
    if (!condition) throw new InvalidOperationException(message);
}

var values = new[] { 2, -3, 6 };
Check(MainModule.SumSquares(values) == 49, "C# -> managed Rust -> native Rust sum failed");

var incremented = MainModule.Increment(new[] { 4, 0, -2 });
Check(incremented.SequenceEqual(new[] { 5, 1, -1 }),
    "C# -> managed Rust -> native Rust mutation failed");

Check(MainModule.Describe("C# 🦀", values) == "C# 🦀: count=3, sum=5",
    "C# -> managed Rust -> native Rust UTF-8/owned String failed");

Check(MainModule.RunningTotals(values).SequenceEqual(new long[] { 2, -1, 5 }),
    "C# -> managed Rust -> native Rust owned vector failed");

Console.WriteLine("C# safe Rust P/Invoke facade assertions passed");
