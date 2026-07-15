using System;

namespace Mycorrhiza.Interop.Helpers;

/// <summary>
/// A neutral metadata marker used by generated-API acceptance tests and available to consumers
/// that want to tag Rust-authored managed APIs without introducing another dependency.
/// </summary>
[AttributeUsage(AttributeTargets.All, AllowMultiple = true, Inherited = false)]
public sealed class RustApiAttribute(string name) : Attribute
{
    public string Name { get; } = name;

    public bool Stable { get; set; }

    public int Order { get; set; }
}
