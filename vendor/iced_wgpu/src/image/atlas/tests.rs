use super::*;
use crate::image::tests::Gpu;

#[test]
#[ignore = "requires a GPU adapter"]
fn shrinks_only_idle_unreferenced_atlas_to_backend_baseline() {
    let gpu = Gpu::new();

    // GL's layer-count workaround is independent of the actual test device.
    for backend in [gpu.backend, wgpu::Backend::Gl] {
        let baseline = if backend == wgpu::Backend::Gl { 2 } else { 1 };
        let mut atlas =
            Atlas::with_size(&gpu.device, backend, gpu.layout.clone(), 8);
        let entries: Vec<_> = (0..3).map(|_| gpu.upload(&mut atlas)).collect();
        let grown_layers = atlas.texture.depth_or_array_layers();
        assert!(grown_layers > baseline);

        atlas.trim();
        assert_eq!(atlas.texture.depth_or_array_layers(), grown_layers);
        atlas.remove(&entries[0]);
        atlas.remove(&entries[1]);
        atlas.trim();
        assert_eq!(atlas.texture.depth_or_array_layers(), grown_layers);

        let retained_batch = atlas.bind_group().clone();
        atlas.remove(&entries[2]);
        atlas.trim();
        assert_eq!(atlas.texture.depth_or_array_layers(), grown_layers);
        drop(retained_batch);

        atlas.trim();
        assert_eq!(atlas.texture.depth_or_array_layers(), baseline);
        assert_eq!(atlas.layers.len(), baseline as usize);
        let group = Arc::downgrade(atlas.bind_group());
        atlas.trim();
        assert!(Arc::ptr_eq(&group.upgrade().unwrap(), atlas.bind_group()));

        let _ = gpu.upload(&mut atlas);
        atlas.trim();
        assert_eq!(atlas.texture.depth_or_array_layers(), baseline);
    }
}

#[test]
#[ignore = "requires a GPU adapter"]
fn recorded_upload_and_readback_survive_idle_reset_before_submission() {
    let gpu = Gpu::new();
    let mut atlas = gpu.atlas();
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    let mut belt = wgpu::util::StagingBelt::new(4096);
    let red = [255, 0, 0, 255].repeat(64);
    let blue = [0, 0, 255, 255].repeat(64);
    let entries: Vec<_> = [&red, &blue, &red]
        .into_iter()
        .map(|pixels| {
            atlas
                .upload(&gpu.device, &mut encoder, &mut belt, 8, 8, pixels)
                .unwrap()
        })
        .collect();
    let output = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("atlas pre-reset readback"),
        size: 256 * 8 * 2,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        atlas.texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &output,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(8),
            },
        },
        wgpu::Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 2,
        },
    );
    for entry in &entries {
        atlas.remove(entry);
    }
    let grown_layers = atlas.texture.depth_or_array_layers();
    atlas.trim();
    let baseline = if gpu.backend == wgpu::Backend::Gl {
        2
    } else {
        1
    };
    assert_eq!(atlas.texture.depth_or_array_layers(), baseline);
    assert!(grown_layers > baseline);

    belt.finish();
    let submission = gpu.queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    output
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
    let _ = gpu.device.poll(wgpu::PollType::Wait {
        submission_index: Some(submission),
        timeout: None,
    });
    receiver.recv().unwrap().unwrap();
    let pixels = output.slice(..).get_mapped_range();
    for layer in 0..2 {
        let expected = if layer == 0 { &red } else { &blue };
        for row in 0..8 {
            let offset = (layer * 8 + row) * 256;
            assert_eq!(
                &pixels[offset..offset + 32],
                &expected[row * 32..(row + 1) * 32]
            );
        }
    }
    drop(pixels);
    output.unmap();

    // Reusing the freshly shrunk atlas must not regrow or reset each frame.
    let entry = gpu.upload(&mut atlas);
    let group = Arc::downgrade(atlas.bind_group());
    atlas.trim();
    assert!(Arc::ptr_eq(&group.upgrade().unwrap(), atlas.bind_group()));
    atlas.remove(&entry);
}
