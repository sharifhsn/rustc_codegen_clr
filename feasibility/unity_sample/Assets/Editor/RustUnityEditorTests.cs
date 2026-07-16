using NUnit.Framework;

public class RustUnityEditorTests
{
    [Test] public void ManagedRustExportIsCallable() => Assert.AreEqual(42, RustUnityBridge.SampleValue());
    [Test] public void ManagedRustDomainSurfaceIsCallable() => Assert.AreEqual(98, RustUnityBridge.ManagedDomainValue());
    [Test] public void NativeRustExportIsCallable() => Assert.AreEqual(42, RustUnityBridge.NativeValue());
}
