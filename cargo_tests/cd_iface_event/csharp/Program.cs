using System;
using System.Reflection;

// A C# class implementing the Rust-defined `IButton` interface with a FIELD-LIKE event. This only
// COMPILES if `Clicked` is a genuine .NET event on the interface (Event row + MethodSemantics
// binding the abstract add_/remove_ accessors) — with plain abstract methods csc would demand
// explicit `add_Clicked`/`remove_Clicked` implementations (CS0535) and reject `event Action
// Clicked;` as an implementation. So the compile itself is the zeroth check.
class Button : IButton
{
    public int Id() => 7;
    public event Action Clicked;
    public void Raise() => Clicked?.Invoke();
}

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
        var btn = new Button();
        IButton b = btn;

        // Reflection: the interface declares a real EventInfo with the right delegate type, and
        // its add accessor is a genuine abstract virtual interface member.
        EventInfo ev = typeof(IButton).GetEvent("Clicked");
        Check("typeof(IButton).GetEvent(\"Clicked\") != null", ev != null);
        Check("EventHandlerType == typeof(Action)", ev != null && ev.EventHandlerType == typeof(Action));
        MethodInfo add = typeof(IButton).GetMethod("add_Clicked");
        Check("add_Clicked accessor is abstract && virtual",
              add != null && add.IsAbstract && add.IsVirtual);

        // Subscribe THROUGH THE INTERFACE reference (virtual dispatch to the field-like
        // implementation), raise from the implementer, observe.
        int hits = 0;
        Action h = () => hits++;
        b.Clicked += h;
        btn.Raise();
        Check("subscribe via interface, Raise() -> handler hit once", hits == 1);

        // Unsubscribe through the interface (remove_ accessor bound), raise again -> unchanged.
        b.Clicked -= h;
        btn.Raise();
        Check("unsubscribe via interface, Raise() -> count unchanged", hits == 1);

        // The plain abstract member coexists with the event on the same interface.
        Check("Id() == 7 through the interface", b.Id() == 7);

        Console.WriteLine($"cd_iface_event: {passed}/{checks} checks passed");
        Environment.Exit(passed == checks ? 0 : 1);
    }
}
