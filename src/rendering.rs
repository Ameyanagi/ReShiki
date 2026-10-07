//! Windows includes both renderers. Avoid treating WARP's CPU emulation as GPU
//! acceleration, while retaining Iced's initialization fallback on real GPUs.
use iced::wgpu;

fn adapters() -> Vec<wgpu::AdapterInfo> {
    let backends = wgpu::Backends::from_env().unwrap_or(wgpu::Backends::all());
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
    if override_backend().is_none()
        && software_only(adapters().iter().map(|adapter| adapter.device_type))
    {
        reshiki_windows::use_software_renderer();
    }
}

/// Report the startup preference, not a claim that a window/device was created.
pub fn diagnostics() -> serde_json::Value {
    let adapters = adapters();
    let automatic = if software_only(adapters.iter().map(|adapter| adapter.device_type)) {
        "tiny-skia"
    } else {
        "wgpu,tiny-skia"
    };
    serde_json::json!({
        "automatic_preference": automatic,
        "ICED_BACKEND": override_backend(),
        "WGPU_BACKEND": std::env::var("WGPU_BACKEND").ok(),
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
