using System;

int value = MainModule.rust_add(20, 22);
Console.WriteLine(value);
return value == 42 ? 0 : 1;
