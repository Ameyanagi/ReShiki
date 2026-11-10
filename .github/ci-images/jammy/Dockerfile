FROM ubuntu:22.04@sha256:076e7bf82319619f28dd0aa9c02ab34b055ad37100395b4f49a2ffba04fb5e11 AS base

ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH

# apt tries the mirrors in priority order and moves on when one fails or lacks a file.
RUN printf '%s\n' 'path-exclude=/usr/share/doc/*' 'path-exclude=/usr/share/man/*' \
        'path-exclude=/usr/share/locale/*' > /etc/dpkg/dpkg.cfg.d/01_nodoc \
    && echo 'man-db man-db/auto-update boolean false' | debconf-set-selections \
    && rm -f /var/lib/man-db/auto-update \
    && printf 'http://azure.archive.ubuntu.com/ubuntu/\tpriority:1\nhttp://mirrors.edge.kernel.org/ubuntu/\tpriority:2\nhttp://archive.ubuntu.com/ubuntu/\tpriority:3\nhttp://security.ubuntu.com/ubuntu/\tpriority:4\n' > /etc/apt/mirrors.txt \
    && sed -i -E 's#http://(archive|security)\.ubuntu\.com/ubuntu/?#mirror+file:/etc/apt/mirrors.txt#' /etc/apt/sources.list \
    && for attempt in 1 2 3; do \
        apt-get update -o Acquire::Retries=5 \
        && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends -o Acquire::Retries=5 \
            ca-certificates curl git tar gzip xz-utils zstd unzip build-essential pkg-config python3 file \
            libicu70 libssl3 zlib1g libgssapi-krb5-2 \
            binutils iproute2 libxkbcommon-dev libwayland-dev libx11-dev libxrandr-dev libxi-dev \
            libxcursor-dev libxinerama-dev libgl1-mesa-dev libfontconfig1-dev libudev-dev \
        && break; \
        [ "$attempt" = 3 ] && exit 1; sleep 30; \
    done \
    && rm -rf /var/lib/apt/lists/*

# Pin the bootstrap tools; each job installs its own Rust toolchain. Container jobs run as
# root over a workspace the runner user owns, so git must trust it.
RUN curl -fsSL --retry 5 -o /tmp/powershell.tar.gz \
        https://github.com/PowerShell/PowerShell/releases/download/v7.6.9/powershell-7.6.9-linux-x64.tar.gz \
    && echo '1da6d6d6035d651f20e70ddee7c4c15ac6ab8b9fb40ee92240d91263f28a884b  /tmp/powershell.tar.gz' | sha256sum -c - \
    && mkdir -p /opt/microsoft/powershell/7 \
    && tar -xzf /tmp/powershell.tar.gz -C /opt/microsoft/powershell/7 \
    && chmod +x /opt/microsoft/powershell/7/pwsh \
    && ln -s /opt/microsoft/powershell/7/pwsh /usr/local/bin/pwsh \
    && curl -fsSL --retry 5 -o /tmp/rustup-init \
        https://static.rust-lang.org/rustup/archive/1.29.3/x86_64-unknown-linux-gnu/rustup-init \
    && echo 'b1c8cb14e30889e7e2999e6b69e62d9e9b9ffc0f4df8180564d306d9680e6c55  /tmp/rustup-init' | sha256sum -c - \
    && chmod +x /tmp/rustup-init \
    && /tmp/rustup-init -y --no-modify-path --profile minimal --default-toolchain none \
    && rm /tmp/powershell.tar.gz /tmp/rustup-init \
    && git config --system --add safe.directory '*'
