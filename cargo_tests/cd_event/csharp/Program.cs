using System;

class Program
{
    static int checks = 0, passed = 0;
    static void Check(string name, bool ok)
    {
        checks++;
        if (ok) passed++;
        Console.WriteLine($"  [{(ok ? "OK" : "FAIL")}] {name}");
    }

    static void Main()
    {
        var n = new Notifier();

        // Decisive check: `+=`/`-=` only compile against a GENUINE event. A plain method pair
        // named add_Changed/remove_Changed would require `n.add_Changed(handler)` — this operator
        // syntax is C#'s own compiler confirming it saw a real `event` member (i.e. the PE writer
        // emitted the Event + MethodSemantics metadata rows correctly).
        Action handler = () => { };
        n.Changed += handler;
        n.Changed -= handler;
        Check("n.Changed += / -= compiles and runs (real event syntax)", true);

        var ev = typeof(Notifier).GetEvent("Changed");
        Check("GetEvent(\"Changed\") finds a real EventInfo", ev != null);
        Check("EventInfo.EventHandlerType is System.Action", ev?.EventHandlerType == typeof(Action));

        var addMethod = typeof(Notifier).GetMethod("add_Changed");
        Check("add_Changed is still a real callable method", addMethod != null);

        Console.WriteLine($"cd_event: {passed}/{checks} checks passed");
        Environment.Exit(passed == checks ? 0 : 1);
    }
}
