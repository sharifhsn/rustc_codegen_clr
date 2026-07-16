using UnityEditor;
using UnityEditor.Build.Reporting;
using UnityEditor.SceneManagement;
using UnityEngine;

public static class BuildPlayers
{
    static void Build(string path, ScriptingImplementation backend)
    {
        var scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
        new GameObject("RustUnityProbe").AddComponent<RustUnityProbe>();
        const string scenePath = "Assets/GeneratedRustUnityAcceptance.unity";
        EditorSceneManager.SaveScene(scene, scenePath);
        PlayerSettings.SetScriptingBackend(BuildTargetGroup.Standalone, backend);
        // The staged Rust native plug-in is built for the host architecture.
        // Unity's value 1 selects Apple silicon; a universal player would also
        // require an x86_64 slice in the dylib.
        PlayerSettings.SetArchitecture(BuildTargetGroup.Standalone, 1);
        var report = BuildPipeline.BuildPlayer(new[] { scenePath }, path,
            BuildTarget.StandaloneOSX, BuildOptions.None);
        if (report.summary.result != BuildResult.Succeeded) throw new System.Exception(report.summary.ToString());
    }
    public static void Mono() => Build("Builds/Mono.app", ScriptingImplementation.Mono2x);
    public static void IL2CPP() => Build("Builds/IL2CPP.app", ScriptingImplementation.IL2CPP);
}
