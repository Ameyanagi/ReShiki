//! GPU regression support. Run the ignored `image::` tests with `image,svg`.

use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread;

pub(super) struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub backend: wgpu::Backend,
    pub layout: wgpu::BindGroupLayout,
}

impl Gpu {
    pub fn new() -> Self {
        let instance = wgpu::Instance::default();
        let adapter = block_on(
            instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
        )
        .expect("image cache regressions require a GPU adapter");
        let (device, queue) = block_on(
            adapter.request_device(&wgpu::DeviceDescriptor::default()),
        )
        .expect("request image cache test device");
        let layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("image cache regression layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float {
                            filterable: true,
                        },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                }],
            });

        Self {
            device,
            queue,
            backend: adapter.get_info().backend,
            layout,
        }
    }

    pub fn atlas(&self) -> super::atlas::Atlas {
        super::atlas::Atlas::with_size(
            &self.device,
            self.backend,
            self.layout.clone(),
            8,
        )
    }

    pub fn upload(
        &self,
        atlas: &mut super::atlas::Atlas,
    ) -> super::atlas::Entry {
        let mut encoder =
            self.device.create_command_encoder(&Default::default());
        let mut belt = wgpu::util::StagingBelt::new(256);
        let entry = atlas
            .upload(
                &self.device,
                &mut encoder,
                &mut belt,
                8,
                8,
                &[255; 8 * 8 * 4],
            )
            .expect("upload test image");
        belt.finish();
        let submission = self.queue.submit([encoder.finish()]);
        let _ = self.device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        });
        entry
    }
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    struct Unpark(thread::Thread);

    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }

    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);

    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::park(),
        }
    }
}
