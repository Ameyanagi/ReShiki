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

#[test]
fn empty_iced_candidates_are_automatic_but_other_values_are_exact_overrides() {
    for value in [None, Some(""), Some(","), Some(",,,")] {
        assert_eq!(override_backend(value), None);
        assert_eq!(requested_backends(value, None), wgpu::Backends::DX12);
    }
    for value in [
        "wgpu",
        "tiny-skia",
        "tiny_skia",
        "invalid",
        " ",
        "WGPU",
        ",wgpu,",
    ] {
        assert_eq!(override_backend(Some(value)), Some(value));
        assert_eq!(requested_backends(Some(value), None), wgpu::Backends::all());
    }
}

#[test]
fn present_wgpu_values_use_the_wgpu_parser_without_applying_the_dx12_default() {
    for (value, expected) in [
        ("", wgpu::Backends::empty()),
        (",,,", wgpu::Backends::empty()),
        ("invalid", wgpu::Backends::empty()),
        ("dx12", wgpu::Backends::DX12),
        (" D3D12 ", wgpu::Backends::DX12),
        ("vulkan", wgpu::Backends::VULKAN),
        ("gl", wgpu::Backends::GL),
        (
            " Vk, DX12,invalid ",
            wgpu::Backends::VULKAN | wgpu::Backends::DX12,
        ),
    ] {
        for iced_backend in [None, Some(""), Some(",,,"), Some("wgpu"), Some("tiny-skia")] {
            assert_eq!(
                requested_backends(iced_backend, Some(OsStr::new(value))),
                expected
            );
        }
    }
}

#[test]
fn non_unicode_wgpu_value_remains_present_without_defaulting_to_dx12() {
    use std::os::windows::ffi::OsStringExt;

    let value = std::ffi::OsString::from_wide(&[0xd800]);
    assert!(value.to_str().is_none());
    for iced_backend in [None, Some(""), Some(",,,"), Some("wgpu")] {
        assert_eq!(
            requested_backends(iced_backend, Some(&value)),
            wgpu::Backends::all()
        );
    }
    let report = diagnostics_with(None, Some(&value), |backends| {
        assert_eq!(backends, wgpu::Backends::all());
        Vec::new()
    });
    assert!(report["WGPU_BACKEND"].is_null());
    assert_eq!(report["adapter_probe_performed"], true);
}

#[test]
fn software_only_diagnostics_do_not_call_the_adapter_probe() {
    for value in [
        "tiny-skia",
        "tiny_skia",
        ",tiny-skia,,",
        "invalid",
        "WGPU",
        " wgpu",
        "tiny-skia, wgpu",
    ] {
        let report = diagnostics_with(Some(value), Some(OsStr::new("gl")), |_| {
            panic!("software-only diagnostics must not initialize graphics drivers")
        });
        assert_eq!(
            report,
            serde_json::json!({
                "automatic_preference": null,
                "startup_preference": value,
                "ICED_BACKEND": value,
                "WGPU_BACKEND": "gl",
                "adapter_probe_performed": false,
                "requested_wgpu_backends": [],
                "adapters": [],
            })
        );
    }
}

#[test]
fn automatic_and_any_exact_wgpu_candidate_probe_the_requested_backends() {
    for (iced_backend, wgpu_backend, expected, preference) in [
        (None, None, wgpu::Backends::DX12, "tiny-skia"),
        (Some(""), None, wgpu::Backends::DX12, "tiny-skia"),
        (Some(",,,"), None, wgpu::Backends::DX12, "tiny-skia"),
        (Some("wgpu"), None, wgpu::Backends::all(), "wgpu"),
        (
            Some("tiny-skia,wgpu"),
            None,
            wgpu::Backends::all(),
            "tiny-skia,wgpu",
        ),
        (
            Some("invalid,wgpu"),
            None,
            wgpu::Backends::all(),
            "invalid,wgpu",
        ),
        (None, Some("gl"), wgpu::Backends::GL, "tiny-skia"),
        (None, Some(""), wgpu::Backends::empty(), "tiny-skia"),
        (None, Some("invalid"), wgpu::Backends::empty(), "tiny-skia"),
    ] {
        let mut probed = false;
        let report = diagnostics_with(iced_backend, wgpu_backend.map(OsStr::new), |backends| {
            probed = true;
            assert_eq!(backends, expected);
            Vec::new()
        });
        assert!(probed);
        assert_eq!(report["adapter_probe_performed"], true);
        assert_eq!(report["automatic_preference"], "tiny-skia");
        assert_eq!(report["startup_preference"], preference);
        assert_eq!(
            report["ICED_BACKEND"],
            serde_json::json!(override_backend(iced_backend))
        );
        assert_eq!(report["WGPU_BACKEND"], serde_json::json!(wgpu_backend));
        assert_eq!(
            report["requested_wgpu_backends"],
            serde_json::json!(
                expected
                    .iter_names()
                    .map(|(name, _)| name)
                    .collect::<Vec<_>>()
            )
        );
        assert_eq!(report["adapters"], serde_json::json!([]));
    }
}

#[test]
fn diagnostics_report_the_adapter_based_automatic_preference() {
    for (device_type, preference) in [(Cpu, "tiny-skia"), (DiscreteGpu, "wgpu,tiny-skia")] {
        let report = diagnostics_with(None, None, |_| {
            vec![wgpu::AdapterInfo {
                name: "test adapter".into(),
                vendor: 0,
                device: 0,
                device_type,
                driver: "test driver".into(),
                driver_info: "test driver info".into(),
                backend: wgpu::Backend::Dx12,
            }]
        });
        assert_eq!(report["automatic_preference"], preference);
        assert_eq!(report["startup_preference"], preference);
        assert_eq!(
            report["adapters"],
            serde_json::json!([{
                "name": "test adapter",
                "device_type": format!("{device_type:?}"),
                "backend": "Dx12",
                "driver": "test driver",
                "driver_info": "test driver info",
            }])
        );
    }
}
