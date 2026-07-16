# Unity managed-Rust U1 sample

This isolated fixture exercises the Unity `netstandard2.1` managed-Rust boundary on the pinned
Unity `6000.3.19f1` macOS Apple-Silicon installation. The managed DLL and native P/Invoke plug-in
execute under Unity's EditMode and PlayMode runners, Mono player, and IL2CPP player. The acceptance
runner proves each gate independently; the pinned installation currently passes all four,
including the exact
`RUST_UNITY_OK=42,DOMAIN=98,NATIVE=42` player marker.

Build the product CLI, point the runner at the pinned Editor, and run it from the repository root:

```bash
cargo build --release -p cargo-dotnet
UNITY_BIN=/Applications/Unity/Hub/Editor/6000.3.19f1/Unity.app/Contents/MacOS/Unity \
  feasibility/unity_sample/run_acceptance.sh
```

The runner builds with the Unity compatibility profile (`NO_UNWIND=1`), stages managed and
optional native assets, checks the deterministic `RUST_UNITY_OK=42` marker, and retains separate
EditMode, PlayMode, Mono-player, and IL2CPP logs under `Builds/`. Missing Unity, a missing license,
or an unsupported module is a failed/skipped gate, never implicit support.
