using WebapiDemo;

Console.WriteLine(Backend.Describe(21));
Console.WriteLine($"native Rust probe={Backend.NativeAssetProbe()}");
return Backend.Double(21) == 42 && Backend.NativeAssetProbe() == 0 ? 0 : 1;
