using System;
using RustExports = Rcl.Interop.MainModule;

int value = RustExports.rust_add(20, 22);
Console.WriteLine(value);
return value == 42 ? 0 : 1;
