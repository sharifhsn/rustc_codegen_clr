namespace Contracts
{
    // The contract a consumer programs against. A Rust-defined managed type will implement this, and
    // the consumer will use that Rust object *only* through this interface — never touching the
    // concrete type — proving genuine polymorphic interop (the shape of slotting a Rust implementation
    // into an existing C# codebase's interface-driven design / DI container).
    public interface IGreeter
    {
        string Greet(string name);
        int Priority();
    }
}
