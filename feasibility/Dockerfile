# Reproducible environment for building & testing rustc_codegen_clr.
#
# The project is only tested on Linux x86_64 / .NET 8 CoreCLR. This image gives
# you that environment (the host's macOS/arm64 box is doubly off the tested path).
#
#   Build (host-native arch, fast):   docker build -t rcc-dev feasibility
#   Build (on-path x86_64, for tests): docker build --platform=linux/amd64 -t rcc-dev feasibility
#
# The repo is NOT copied in — mount it at runtime so you can edit + rebuild
# without rebuilding the image (see feasibility/run.sh). The image is env-only.
FROM debian:bookworm-slim

# Pin the nightly so the image is reproducible. This was the *latest* nightly on
# 2026-06-17. To track a newer nightly: bump this and fix any fresh API drift.
ARG NIGHTLY=nightly-2026-06-17

ENV DEBIAN_FRONTEND=noninteractive \
    PATH=/root/.cargo/bin:/usr/share/dotnet:$PATH \
    DOTNET_ROOT=/usr/share/dotnet \
    DOTNET_CLI_TELEMETRY_OPTOUT=1 \
    NUGET_XMLDOC_MODE=skip \
    CARGO_TERM_COLOR=always

# System toolchain: C compilers (for C_MODE), git/curl, and `ilasm` via mono
# (the project supports both the Mono and CoreCLR ILASM flavours; mono is easiest).
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl git pkg-config \
        build-essential clang gcc \
        mono-devel \
    && rm -rf /var/lib/apt/lists/*

# .NET 8 SDK (provides the `dotnet` CLI + runtime to run produced assemblies).
RUN curl -fsSL https://dot.net/v1/dotnet-install.sh -o /tmp/dotnet-install.sh \
    && bash /tmp/dotnet-install.sh --channel 8.0 --install-dir /usr/share/dotnet \
    && rm /tmp/dotnet-install.sh \
    && dotnet --info

# Rust nightly + the components a rustc codegen backend needs.
# NOTE: components must each get their own -c flag (space-separated values are
# parsed as toolchain names). Do NOT append `|| true` here — it would mask a
# failed toolchain install and produce a broken image.
RUN curl --proto '=https' --tlsv1.2 -fsSf https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain none --profile minimal \
    && rustup toolchain install "${NIGHTLY}" \
        -c rustc-dev -c rust-src -c llvm-tools-preview -c rustfmt -c clippy \
    && rustup default "${NIGHTLY}" \
    && rustc -V && cargo -V

# .NET 9 RUNTIME (side-by-side with 8) + its matching CoreCLR ilasm, for `cargo dotnet --dotnet 9`
# in the docker backend. Each runtime needs ITS CoreCLR ilasm: the default net8 path keeps Mono's
# `ilasm`; net9 uses this one (Mono/older PEs are rejected by the net9 loader). The bash front-end
# sets ILASM_PATH=/root/.dotnet/ilasm9-tool/ilasm + DOTNET_VERSION=9 when --dotnet 9 is requested.
# Placed AFTER the rust layer so it doesn't bust that (expensive) cache.
RUN apt-get update && apt-get install -y --no-install-recommends unzip \
    && rm -rf /var/lib/apt/lists/* \
    && curl -fsSL https://dot.net/v1/dotnet-install.sh -o /tmp/dotnet-install.sh \
    && bash /tmp/dotnet-install.sh --channel 9.0 --runtime dotnet --install-dir /usr/share/dotnet \
    && rm /tmp/dotnet-install.sh \
    && rid=linux-x64; case "$(dpkg --print-architecture)" in arm64) rid=linux-arm64;; esac \
    && curl -fsSL -o /tmp/ilasm9.nupkg "https://www.nuget.org/api/v2/package/runtime.$rid.microsoft.netcore.ilasm/9.0.0" \
    && mkdir -p /tmp/ilasm9 /root/.dotnet/ilasm9-tool \
    && (cd /tmp/ilasm9 && unzip -qo /tmp/ilasm9.nupkg) \
    && cp "/tmp/ilasm9/runtimes/$rid/native/ilasm" /root/.dotnet/ilasm9-tool/ilasm \
    && chmod +x /root/.dotnet/ilasm9-tool/ilasm \
    && rm -rf /tmp/ilasm9 /tmp/ilasm9.nupkg \
    && test -x /root/.dotnet/ilasm9-tool/ilasm

# Sanity-check ilasm is on PATH (mono provides it). Non-fatal: version output is noisy.
RUN (ilasm --version 2>/dev/null || mono --version 2>/dev/null | head -1) | head -1 \
    || echo "WARNING: ilasm not found on PATH"

# Force the pinned nightly. The repo's rust-toolchain.toml pins bare "nightly", which would
# otherwise make rustup download a fresh (newer) nightly inside the container, defeating
# reproducibility. RUSTUP_TOOLCHAIN takes precedence over rust-toolchain.toml.
ENV RUSTUP_TOOLCHAIN=${NIGHTLY}
# Make `dotnet` resolvable in login shells too (it's already on ENV PATH for non-login).
RUN ln -sf /usr/share/dotnet/dotnet /usr/local/bin/dotnet

WORKDIR /work
CMD ["bash"]
