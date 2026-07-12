// The compiler's internal MainModule sentinel is projected to each package's explicit managed
// identity at final link time, so identical Rust function names remain independently callable.
Console.WriteLine(Collision.Alpha.Exports.LibraryName());
Console.WriteLine(Collision.Beta.Exports.LibraryName());
Console.WriteLine(Collision.InvalidCustomAssembly.Exports.LibraryName());
