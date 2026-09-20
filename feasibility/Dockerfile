# Reproducible environment for building & testing rustc_codegen_clr.
#
# Reproducible Linux environment with .NET 8, 9, and 10 installed side-by-side.
#
#   Build (host-native arch, fast):   docker build -t rcc-dev feasibility
#   Build (on-path x86_64, for tests): docker build --platform=linux/amd64 -t rcc-dev feasibility
#
# The repo is NOT copied in — mount it at runtime so you can edit + rebuild
# without rebuilding the image (see feasibility/run.sh). The image is env-only.
FROM debian:bookworm-slim

# Pin the nightly so the image is reproducible. This was the *latest* nightly on
# 2026-06-17. To track a newer nightly: bump this and fix any fresh API drift.
ARG NIGHTLY=nightly-2026-09-18

ENV DEBIAN_FRONTEND=noninteractive \
    PATH=/root/.cargo/bin:/usr/share/dotnet:$PATH \
    DOTNET_ROOT=/usr/share/dotnet \
    DOTNET_CLI_TELEMETRY_OPTOUT=1 \
    NUGET_XMLDOC_MODE=skip \
    CARGO_TERM_COLOR=always

# System packages needed by the Rust and .NET build harness.
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl git pkg-config \
        build-essential \
    && rm -rf /var/lib/apt/lists/*

# .NET 10 SDK (the default target) plus the supported downlevel runtimes.
RUN curl -fsSL https://dot.net/v1/dotnet-install.sh -o /tmp/dotnet-install.sh \
    && bash /tmp/dotnet-install.sh --channel 10.0 --install-dir /usr/share/dotnet \
    && bash /tmp/dotnet-install.sh --channel 9.0 --runtime dotnet --install-dir /usr/share/dotnet \
    && bash /tmp/dotnet-install.sh --channel 8.0 --runtime dotnet --install-dir /usr/share/dotnet \
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

# Force the pinned nightly. The repo's rust-toolchain.toml pins bare "nightly", which would
# otherwise make rustup download a fresh (newer) nightly inside the container, defeating
# reproducibility. RUSTUP_TOOLCHAIN takes precedence over rust-toolchain.toml.
ENV RUSTUP_TOOLCHAIN=${NIGHTLY}
# Make `dotnet` resolvable in login shells too (it's already on ENV PATH for non-login).
RUN ln -sf /usr/share/dotnet/dotnet /usr/local/bin/dotnet

WORKDIR /work
CMD ["bash"]
