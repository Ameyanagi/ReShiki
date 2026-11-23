use crate::core::Size;
use crate::core::image;
use crate::graphics;
use crate::image::atlas::{self, Atlas};

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::{Arc, Weak};

pub type Image = graphics::image::Buffer;

/// Entry in cache corresponding to an image handle
#[derive(Debug)]
pub enum Memory {
    /// Image data on host
    Host(Image),
    /// Storage entry
    Device {
        entry: atlas::Entry,
        bind_group: Option<Arc<wgpu::BindGroup>>,
        allocation: Option<Weak<image::Memory>>,
    },
    Error(image::Error),
}

impl Memory {
    pub fn load(handle: &image::Handle) -> Self {
        match graphics::image::load(handle) {
            Ok(image) => Self::Host(image),
            Err(error) => Self::Error(error),
        }
    }

    pub fn dimensions(&self) -> Size<u32> {
        match self {
            Memory::Host(image) => {
                let (width, height) = image.dimensions();

                Size::new(width, height)
            }
            Memory::Device { entry, .. } => entry.size(),
            Memory::Error(_) => Size::new(1, 1),
        }
    }

    pub fn host(&self) -> Option<Image> {
        match self {
            Memory::Host(image) => Some(image.clone()),
            Memory::Device { .. } | Memory::Error(_) => None,
        }
    }
}

#[derive(Debug, Default)]
pub struct Cache {
    map: FxHashMap<image::Id, Memory>,
    hits: FxHashSet<image::Id>,
}

impl Cache {
    pub fn get_mut(&mut self, handle: &image::Handle) -> Option<&mut Memory> {
        let _ = self.hits.insert(handle.id());

        self.map.get_mut(&handle.id())
    }

    pub fn insert(&mut self, handle: &image::Handle, memory: Memory) {
        let _ = self.map.insert(handle.id(), memory);
        let _ = self.hits.insert(handle.id());
    }

    pub fn contains(&self, handle: &image::Handle) -> bool {
        self.map.contains_key(&handle.id())
    }

    pub fn trim(
        &mut self,
        atlas: &mut Atlas,
        on_drop: impl Fn(Arc<wgpu::BindGroup>),
    ) {
        // A retained image batch may still address the shared atlas. Do not
        // recycle its regions until it releases the bind group.
        let atlas_in_use = atlas.is_in_use();
        let hits = &self.hits;

        self.map.retain(|id, memory| {
            // Retain active allocations
            if let Memory::Device {
                allocation,
                bind_group,
                ..
            } = memory
                && (allocation
                    .as_ref()
                    .is_some_and(|allocation| allocation.strong_count() > 0)
                    || bind_group.as_ref().map_or(atlas_in_use, |bind_group| {
                        Arc::strong_count(bind_group) > 1
                    }))
            {
                return true;
            }

            let retain = hits.contains(id);

            if !retain {
                log::debug!("Dropping image allocation: {id:?}");

                if let Memory::Device {
                    entry, bind_group, ..
                } = memory
                {
                    if let Some(bind_group) = bind_group.take() {
                        on_drop(bind_group);
                    } else {
                        atlas.remove(entry);
                    }
                }
            }

            retain
        });

        self.hits.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::tests::Gpu;
    use std::cell::Cell;

    #[test]
    #[ignore = "requires a GPU adapter"]
    fn trims_idle_entries_without_insertions_but_keeps_each_frame_use() {
        let gpu = Gpu::new();
        let mut atlas = gpu.atlas();
        let mut cache = Cache::default();
        let handle = image::Handle::from_rgba(8, 8, vec![255; 8 * 8 * 4]);
        cache.insert(
            &handle,
            Memory::Device {
                entry: gpu.upload(&mut atlas),
                bind_group: None,
                allocation: None,
            },
        );
        cache.trim(&mut atlas, |_| {
            panic!("shared atlas has no dedicated group")
        });

        for _ in 0..3 {
            assert!(cache.get_mut(&handle).is_some());
            cache.trim(&mut atlas, |_| panic!("unexpected dedicated group"));
            assert!(cache.contains(&handle));
        }

        cache.trim(&mut atlas, |_| panic!("unexpected dedicated group"));
        assert!(!cache.contains(&handle));
    }

    #[test]
    #[ignore = "requires a GPU adapter"]
    fn retains_live_allocation_and_shared_atlas_until_owners_release_them() {
        let gpu = Gpu::new();
        let mut atlas = gpu.atlas();
        let mut cache = Cache::default();
        let handle = image::Handle::from_rgba(8, 8, vec![255; 8 * 8 * 4]);
        let entry = gpu.upload(&mut atlas);
        // SAFETY: this image has just been allocated in the atlas above.
        #[allow(unsafe_code)]
        let allocation = unsafe { image::allocate(&handle, entry.size()) };
        cache.insert(
            &handle,
            Memory::Device {
                entry,
                bind_group: None,
                allocation: Some(allocation.downgrade()),
            },
        );
        cache.trim(&mut atlas, |_| {});
        cache.trim(&mut atlas, |_| {});
        assert!(cache.contains(&handle));

        let retained_batch = atlas.bind_group().clone();
        drop(allocation);
        cache.trim(&mut atlas, |_| {});
        assert!(cache.contains(&handle));
        drop(retained_batch);
        cache.trim(&mut atlas, |_| {});
        assert!(!cache.contains(&handle));
    }

    #[test]
    #[ignore = "requires a GPU adapter"]
    fn retains_dedicated_bind_group_until_batch_releases_it() {
        let gpu = Gpu::new();
        let mut atlas = gpu.atlas();
        let mut dedicated = gpu.atlas();
        let entry = gpu.upload(&mut dedicated);
        let retained_batch = dedicated.bind_group().clone();
        drop(dedicated);
        let mut cache = Cache::default();
        let handle = image::Handle::from_rgba(8, 8, vec![255; 8 * 8 * 4]);
        cache.insert(
            &handle,
            Memory::Device {
                entry,
                bind_group: Some(retained_batch.clone()),
                allocation: None,
            },
        );
        let dropped = Cell::new(0);
        let on_drop = |_| dropped.set(dropped.get() + 1);
        cache.trim(&mut atlas, on_drop);
        cache.trim(&mut atlas, on_drop);
        assert!(cache.contains(&handle));
        assert_eq!(dropped.get(), 0);

        drop(retained_batch);
        cache.trim(&mut atlas, on_drop);
        assert!(!cache.contains(&handle));
        assert_eq!(dropped.get(), 1);
    }
}
