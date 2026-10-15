//! Prefer Windows' native DX12 backend before considering software rendering.
//! Avoid initializing unrelated graphics drivers just to discover CPU-only WARP.
use iced::wgpu;
use std::ffi::OsStr;

fn requested_backends(iced_backend: Option<&str>, wgpu_backend: Option<&OsStr>) -> wgpu::Backends {
    // Match WGPU's environment parser while retaining presence for non-Unicode
    // values, which its parser ignores without making the variable absent.
    wgpu_backend
        .and_then(OsStr::to_str)
        .map(wgpu::Backends::from_comma_list)
        .unwrap_or_else(|| {
            if override_backend(iced_backend).is_none() && wgpu_backend.is_none() {
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

fn override_backend(value: Option<&str>) -> Option<&str> {
    // Iced treats an empty candidate list as automatic selection too.
    value.filter(|value| value.split(',').any(|candidate| !candidate.is_empty()))
}

pub fn configure() {
    let iced_backend = std::env::var("ICED_BACKEND").ok();
    if override_backend(iced_backend.as_deref()).is_some() {
        return;
    }
    // Iced also reads WGPU_BACKEND. Apply the same default to its later
    // initialization; changing only this probe would still initialize GL there.
    reshiki_windows::prefer_direct3d_backend();
    let wgpu_backend = std::env::var_os("WGPU_BACKEND");
    if software_only(
        adapters(requested_backends(
            iced_backend.as_deref(),
            wgpu_backend.as_deref(),
        ))
        .iter()
        .map(|adapter| adapter.device_type),
    ) {
        reshiki_windows::use_software_renderer();
    }
}

/// Report the startup preference, not a claim that a window/device was created.
pub fn diagnostics() -> serde_json::Value {
    let iced_backend = std::env::var("ICED_BACKEND").ok();
    let wgpu_backend = std::env::var_os("WGPU_BACKEND");
    diagnostics_with(iced_backend.as_deref(), wgpu_backend.as_deref(), adapters)
}

fn diagnostics_with(
    iced_backend: Option<&str>,
    wgpu_backend: Option<&OsStr>,
    probe_adapters: impl FnOnce(wgpu::Backends) -> Vec<wgpu::AdapterInfo>,
) -> serde_json::Value {
    let explicit = override_backend(iced_backend);
    // Do not initialize graphics drivers when the explicit renderer list cannot
    // select WGPU. Earlier candidates can fail, so consider the complete list.
    let probe = explicit.is_none_or(|value| value.split(',').any(|candidate| candidate == "wgpu"));
    let backends = if probe {
        requested_backends(iced_backend, wgpu_backend)
    } else {
        wgpu::Backends::empty()
    };
    let adapters = if probe {
        probe_adapters(backends)
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
        "startup_preference": explicit.or(automatic),
        "ICED_BACKEND": explicit,
        "WGPU_BACKEND": wgpu_backend.and_then(OsStr::to_str),
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
