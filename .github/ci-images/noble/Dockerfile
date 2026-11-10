FROM ubuntu:24.04@sha256:fe81560990fae86dc4743202b2cb4d3df98d0bcf949173cecef15ce1127ef852 AS base

ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH

# apt tries the mirrors in priority order and moves on when one fails or lacks a file.
RUN printf '%s\n' 'path-exclude=/usr/share/doc/*' 'path-exclude=/usr/share/man/*' \
        'path-exclude=/usr/share/locale/*' > /etc/dpkg/dpkg.cfg.d/01_nodoc \
    && echo 'man-db man-db/auto-update boolean false' | debconf-set-selections \
    && rm -f /var/lib/man-db/auto-update \
    && printf 'http://azure.archive.ubuntu.com/ubuntu/\tpriority:1\nhttp://mirrors.edge.kernel.org/ubuntu/\tpriority:2\nhttp://archive.ubuntu.com/ubuntu/\tpriority:3\nhttp://security.ubuntu.com/ubuntu/\tpriority:4\n' > /etc/apt/mirrors.txt \
    && sed -i -E 's#^URIs: http://(archive|security)\.ubuntu\.com/ubuntu/?$#URIs: mirror+file:/etc/apt/mirrors.txt#' /etc/apt/sources.list.d/ubuntu.sources \
    && for attempt in 1 2 3; do \
        apt-get update -o Acquire::Retries=5 \
        && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends -o Acquire::Retries=5 \
            ca-certificates curl git tar gzip xz-utils zstd unzip build-essential pkg-config python3 file \
            libicu74 libssl3t64 zlib1g libgssapi-krb5-2 \
            libxkbcommon-dev libwayland-dev libx11-dev libxrandr-dev libxi-dev libxcursor-dev \
            libxinerama-dev libgl1-mesa-dev libfontconfig1-dev libudev-dev xvfb xauth \
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

FROM base AS desktop

# The pinned base already lists noble-backports; LibreOffice comes from there (>= 25.8).
RUN for attempt in 1 2 3; do \
        apt-get update -o Acquire::Retries=5 \
        && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends -o Acquire::Retries=5 \
            gnome-shell sway wtype xdotool dbus-x11 python3-gi gir1.2-gtk-4.0 \
            mesa-vulkan-drivers libgl1-mesa-dri fonts-dejavu-core libglib2.0-bin \
            libegl1-mesa-dev libxcb-shape0-dev libxcb-xfixes0-dev libx11-xcb-dev \
        && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends -o Acquire::Retries=5 \
            -t noble-backports libreoffice-core-nogui python3-uno \
        && break; \
        [ "$attempt" = 3 ] && exit 1; sleep 30; \
    done \
    && rm -rf /var/lib/apt/lists/*
