// A base class with a non-trivial (argument-taking) constructor, used to prove Wall 4
// (base_ctor_args on #[dotnet_class]) chains to it correctly.
namespace CdClassErgonomicsBase
{
    public class Widget
    {
        public readonly int Seed;
        public Widget(int seed)
        {
            Seed = seed;
        }
    }
}
