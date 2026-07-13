$ErrorActionPreference = "Stop"

$Version = if ($env:RUST_DOTNET_VERSION) { $env:RUST_DOTNET_VERSION } else { "0.0.1" }
$Repository = if ($env:RUST_DOTNET_REPOSITORY) { $env:RUST_DOTNET_REPOSITORY } else { "sharifhsn/rustc_codegen_clr" }

if (-not [Environment]::Is64BitOperatingSystem) {
    throw "rust-dotnet $Version supports Windows x64 only."
}

$HostId = "windows-x64"
$Base = if ($env:RUST_DOTNET_BASE_URL) { $env:RUST_DOTNET_BASE_URL } else { "https://github.com/$Repository/releases/download/rust-dotnet-v$Version" }
$Work = Join-Path ([IO.Path]::GetTempPath()) ("rust-dotnet-install-" + [Guid]::NewGuid())
$Driver = Join-Path $Work "cargo-dotnet-$HostId.exe"
$Bundle = Join-Path $Work "cargo-dotnet-sdk-$HostId-$Version.zip"

New-Item -ItemType Directory -Path $Work | Out-Null
try {
    Write-Host "Downloading rust-dotnet $Version for $HostId..."
    Invoke-WebRequest "$Base/cargo-dotnet-$HostId.exe" -OutFile $Driver
    Invoke-WebRequest "$Base/cargo-dotnet-sdk-$HostId-$Version.zip" -OutFile $Bundle
    Invoke-WebRequest "$Base/cargo-dotnet-sdk-$HostId-$Version.zip.sha256" -OutFile "$Bundle.sha256"

    & $Driver bundle install $Bundle
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
finally {
    Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "rust-dotnet $Version is installed. Open a new terminal, then run:"
Write-Host "  cargo dotnet doctor"
Write-Host "  cargo dotnet new hello-dotnet --app"
Write-Host "  cargo dotnet run hello-dotnet"
