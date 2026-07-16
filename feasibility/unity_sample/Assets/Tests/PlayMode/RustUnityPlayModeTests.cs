using System.Collections;
using NUnit.Framework;
using UnityEngine.TestTools;

public class RustUnityPlayModeTests
{
    [UnityTest]
    public IEnumerator ManagedAndNativeRustRemainCallableInPlayMode()
    {
        yield return null;
        Assert.AreEqual(42, RustUnityBridge.SampleValue());
        Assert.AreEqual(98, RustUnityBridge.ManagedDomainValue());
        Assert.AreEqual(42, RustUnityBridge.NativeValue());
    }
}
