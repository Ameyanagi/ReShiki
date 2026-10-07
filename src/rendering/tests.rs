use super::*;
use wgpu::DeviceType::*;

#[test]
fn no_adapter_or_only_cpu_adapters_use_direct_software_rendering() {
    assert!(software_only([]));
    assert!(software_only([Cpu]));
    assert!(software_only([Cpu, Cpu]));
}

#[test]
fn hardware_adapters_keep_gpu_rendering_available() {
    for device in [IntegratedGpu, DiscreteGpu, VirtualGpu, Other] {
        assert!(!software_only([device]));
        assert!(!software_only([Cpu, device]));
        assert!(!software_only([device, Cpu]));
    }
}
