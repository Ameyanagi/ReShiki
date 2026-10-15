//! Prefer Windows' native DX12 backend before considering software rendering.
//! Avoid initializing unrelated graphics drivers just to discover CPU-only WARP.
use iced::wgpu;

fn requested_backends() -> wgpu::Backends {
    wgpu::Backends::from_env().unwrap_or_else(|| {
        if override_backend().is_none() && std::env::var_os("WGPU_BACKEND").is_none() {
            wgpu::Backends::DX12
        } else {
            // Match Iced when an explicit renderer bypasses our configuration,
            // or a non-Unicode backend value is ignored by WGPU's parser.
            wgpu::Backends::all()
        }
    })
}

fn adapters(backends: wgpu::Backends) -> Vec<wgpu::AdapterInfo> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends,
        flags: wgpu::InstanceFlags::empty(),
        ..Default::default()
    });
    instance
        .enumerate_adapters(backends)
        .into_iter()
        .map(|adapter| adapter.get_info())
        .collect()
}

fn software_only(devices: impl IntoIterator<Item = wgpu::DeviceType>) -> bool {
    devices
        .into_iter()
        .all(|device| device == wgpu::DeviceType::Cpu)
}

fn override_backend() -> Option<String> {
    // Iced treats an empty candidate list as automatic selection too.
    std::env::var("ICED_BACKEND")
        .ok()
        .filter(|value| value.split(',').any(|candidate| !candidate.is_empty()))
}

pub fn configure() {
    if override_backend().is_some() {
        return;
    }
    // Iced also reads WGPU_BACKEND. Apply the same default to its later
    // initialization; changing only this probe would still initialize GL there.
    reshiki_windows::prefer_direct3d_backend();
    if software_only(
        adapters(requested_backends())
            .iter()
            .map(|adapter| adapter.device_type),
    ) {
        reshiki_windows::use_software_renderer();
    }
}

/// Report the startup preference, not a claim that a window/device was created.
pub fn diagnostics() -> serde_json::Value {
    let explicit = override_backend();
    // Do not initialize graphics drivers when the explicit renderer list cannot
    // select WGPU. Earlier candidates can fail, so consider the complete list.
    let probe = explicit
        .as_deref()
        .is_none_or(|value| value.split(',').any(|candidate| candidate == "wgpu"));
    let backends = if probe {
        requested_backends()
    } else {
        wgpu::Backends::empty()
    };
    let adapters = if probe {
        adapters(backends)
    } else {
        Vec::new()
    };
    let automatic = probe.then(|| {
        if software_only(adapters.iter().map(|adapter| adapter.device_type)) {
            "tiny-skia"
        } else {
            "wgpu,tiny-skia"
        }
    });
    serde_json::json!({
        "automatic_preference": automatic,
        "startup_preference": explicit.as_deref().or(automatic),
        "ICED_BACKEND": explicit,
        "WGPU_BACKEND": std::env::var("WGPU_BACKEND").ok(),
        "adapter_probe_performed": probe,
        "requested_wgpu_backends": backends.iter_names().map(|(name, _)| name).collect::<Vec<_>>(),
        "adapters": adapters.iter().map(|adapter| serde_json::json!({
            "name": adapter.name,
            "device_type": format!("{:?}", adapter.device_type),
            "backend": format!("{:?}", adapter.backend),
            "driver": adapter.driver,
            "driver_info": adapter.driver_info,
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests;
