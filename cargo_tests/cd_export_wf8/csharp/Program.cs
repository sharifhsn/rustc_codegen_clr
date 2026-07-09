int pass = 0, total = 0;
void Check<T>(string name, T got, T want)
{
    total++;
    bool ok = Equals(got, want);
    Console.WriteLine($"  {name} = {got} ({(ok ? "ok" : $"FAIL want {want}")})");
    if (ok) pass++;
}

// Nullable<T> return: a real int?, usable with .HasValue/.Value/nullable operators directly.
int? some = MainModule.maybe_positive(5);
Check("maybe_positive(5)", some, 10);

int? none = MainModule.maybe_positive(-3);
total++;
if (none is null) { Console.WriteLine("  maybe_positive(-3) = null (ok)"); pass++; }
else Console.WriteLine($"  maybe_positive(-3) = {none} (FAIL want null)");

// Managed array return: a real int[], usable with LINQ/foreach/indexers directly.
int[] squares = MainModule.first_n_squares(4);
Check("first_n_squares(4).Length", squares.Length, 4);
Check("first_n_squares(4)[0]", squares[0], 1);
Check("first_n_squares(4)[1]", squares[1], 4);
Check("first_n_squares(4)[2]", squares[2], 9);
Check("first_n_squares(4)[3]", squares[3], 16);
Check("first_n_squares(4).Sum() via LINQ", squares.Sum(), 30);

Console.WriteLine($"{pass}/{total}");
if (pass != total) Environment.Exit(1);
