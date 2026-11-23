use crate::core::svg;
use crate::core::{Color, Size};
use crate::image::atlas::{self, Atlas};

use resvg::tiny_skia;
use resvg::usvg;
use rustc_hash::{FxHashMap, FxHashSet};
use std::fs;
use std::panic;
use std::sync::Arc;

/// Entry in cache corresponding to an svg handle
pub enum Svg {
    /// Parsed svg
    Loaded(usvg::Tree),
    /// Svg not found or failed to parse
    NotFound,
}

impl Svg {
    /// Viewport width and height
    pub fn viewport_dimensions(&self) -> Size<u32> {
        match self {
            Svg::Loaded(tree) => {
                let size = tree.size();

                Size::new(size.width() as u32, size.height() as u32)
            }
            Svg::NotFound => Size::new(1, 1),
        }
    }
}

/// Caches svg vector and raster data
#[derive(Debug, Default)]
pub struct Cache {
    svgs: FxHashMap<u64, Svg>,
    rasterized: FxHashMap<(u64, u32, u32, ColorFilter), atlas::Entry>,
    svg_hits: FxHashSet<u64>,
    rasterized_hits: FxHashSet<(u64, u32, u32, ColorFilter)>,
    fontdb: Option<Arc<usvg::fontdb::Database>>,
}

type ColorFilter = Option<[u8; 4]>;

impl Cache {
    /// Load svg
    pub fn load(&mut self, handle: &svg::Handle) -> &Svg {
        if self.svgs.contains_key(&handle.id()) {
            let _ = self.svg_hits.insert(handle.id());
            return self.svgs.get(&handle.id()).unwrap();
        }

        // TODO: Reuse `cosmic-text` font database
        if self.fontdb.is_none() {
            let mut fontdb = usvg::fontdb::Database::new();
            fontdb.load_system_fonts();

            self.fontdb = Some(Arc::new(fontdb));
        }

        let options = usvg::Options {
            fontdb: self
                .fontdb
                .as_ref()
                .expect("fontdb must be initialized")
                .clone(),
            ..usvg::Options::default()
        };

        let svg = match handle.data() {
            svg::Data::Path(path) => fs::read_to_string(path)
                .ok()
                .and_then(|contents| {
                    usvg::Tree::from_str(&contents, &options).ok()
                })
                .map(Svg::Loaded)
                .unwrap_or(Svg::NotFound),
            svg::Data::Bytes(bytes) => {
                match usvg::Tree::from_data(bytes, &options) {
                    Ok(tree) => Svg::Loaded(tree),
                    Err(_) => Svg::NotFound,
                }
            }
        };

        let _ = self.svgs.insert(handle.id(), svg);
        let _ = self.svg_hits.insert(handle.id());
        self.svgs.get(&handle.id()).unwrap()
    }

    /// Load svg and upload raster data
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        belt: &mut wgpu::util::StagingBelt,
        handle: &svg::Handle,
        color: Option<Color>,
        size: Size,
        scale: f32,
        atlas: &mut Atlas,
    ) -> Option<&atlas::Entry> {
        let id = handle.id();

        let (width, height) = (
            (scale * size.width).ceil() as u32,
            (scale * size.height).ceil() as u32,
        );

        let color = color.map(Color::into_rgba8);
        let key = (id, width, height, color);

        // TODO: Optimize!
        // We currently rerasterize the SVG when its size changes. This is slow
        // as heck. A GPU rasterizer like `pathfinder` may perform better.
        // It would be cool to be able to smooth resize the `svg` example.
        if self.rasterized.contains_key(&key) {
            let _ = self.svg_hits.insert(id);
            let _ = self.rasterized_hits.insert(key);

            return self.rasterized.get(&key);
        }

        match self.load(handle) {
            Svg::Loaded(tree) => {
                if width == 0 || height == 0 {
                    return None;
                }

                // TODO: Optimize!
                // We currently rerasterize the SVG when its size changes. This is slow
                // as heck. A GPU rasterizer like `pathfinder` may perform better.
                // It would be cool to be able to smooth resize the `svg` example.
                let mut img = tiny_skia::Pixmap::new(width, height)?;

                let tree_size = tree.size().to_int_size();

                let target_size = if width > height {
                    tree_size.scale_to_width(width)
                } else {
                    tree_size.scale_to_height(height)
                };

                let transform = if let Some(target_size) = target_size {
                    let tree_size = tree_size.to_size();
                    let target_size = target_size.to_size();

                    tiny_skia::Transform::from_scale(
                        target_size.width() / tree_size.width(),
                        target_size.height() / tree_size.height(),
                    )
                } else {
                    tiny_skia::Transform::default()
                };

                // SVG rendering can panic on malformed or complex vectors.
                // We catch panics to prevent crashes and continue gracefully.
                let render =
                    panic::catch_unwind(panic::AssertUnwindSafe(|| {
                        resvg::render(tree, transform, &mut img.as_mut());
                    }));

                if let Err(error) = render {
                    log::warn!(
                        "SVG rendering for {handle:?} panicked: {error:?}"
                    );
                }

                let mut rgba = img.take();

                if let Some(color) = color {
                    rgba.chunks_exact_mut(4).for_each(|rgba| {
                        if rgba[3] > 0 {
                            rgba[0] = color[0];
                            rgba[1] = color[1];
                            rgba[2] = color[2];
                        }
                    });
                }

                let allocation = atlas
                    .upload(device, encoder, belt, width, height, &rgba)?;

                log::debug!("allocating {id} {width}x{height}");

                let _ = self.svg_hits.insert(id);
                let _ = self.rasterized_hits.insert(key);
                let _ = self.rasterized.insert(key, allocation);

                self.rasterized.get(&key)
            }
            Svg::NotFound => None,
        }
    }

    /// Load svg and upload raster data
    pub fn trim(&mut self, atlas: &mut Atlas) {
        let atlas_in_use = atlas.is_in_use();
        let svg_hits = &self.svg_hits;
        let rasterized_hits = &self.rasterized_hits;

        // Each hit set is a subset of its cache, so matching counts mean that
        // every entry was used and a full map scan cannot evict anything.
        if self.svgs.len() != svg_hits.len() {
            self.svgs
                .retain(|k, _| atlas_in_use || svg_hits.contains(k));
        }
        if self.rasterized.len() != rasterized_hits.len() {
            self.rasterized.retain(|k, entry| {
                let retain = atlas_in_use || rasterized_hits.contains(k);

                if !retain {
                    atlas.remove(entry);
                }

                retain
            });
        }
        self.svg_hits.clear();
        self.rasterized_hits.clear();
    }
}

impl std::fmt::Debug for Svg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Svg::Loaded(_) => write!(f, "Svg::Loaded"),
            Svg::NotFound => write!(f, "Svg::NotFound"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::tests::Gpu;

    #[test]
    #[ignore = "requires a GPU adapter"]
    fn trims_idle_sizes_but_retains_measurements_and_batches() {
        let gpu = Gpu::new();
        let mut atlas = gpu.atlas();
        let mut cache = Cache {
            fontdb: Some(Arc::new(usvg::fontdb::Database::new())),
            ..Cache::default()
        };
        let handle = svg::Handle::from_memory(
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><path d="M0 0h4v4H0z" fill="#fff"/></svg>"##.as_slice(),
        );
        let mut encoder =
            gpu.device.create_command_encoder(&Default::default());
        let mut belt = wgpu::util::StagingBelt::new(256);
        for size in [4.0, 8.0] {
            assert!(
                cache
                    .upload(
                        &gpu.device,
                        &mut encoder,
                        &mut belt,
                        &handle,
                        None,
                        Size::new(size, size),
                        1.0,
                        &mut atlas,
                    )
                    .is_some()
            );
        }
        belt.finish();
        let _ = gpu.queue.submit([encoder.finish()]);
        cache.trim(&mut atlas);
        assert_eq!(cache.rasterized.len(), 2);

        let retained_batch = atlas.bind_group().clone();
        cache.trim(&mut atlas);
        assert_eq!(cache.rasterized.len(), 2);
        drop(retained_batch);

        for _ in 0..3 {
            let mut encoder =
                gpu.device.create_command_encoder(&Default::default());
            assert!(
                cache
                    .upload(
                        &gpu.device,
                        &mut encoder,
                        &mut belt,
                        &handle,
                        None,
                        Size::new(4.0, 4.0),
                        1.0,
                        &mut atlas,
                    )
                    .is_some()
            );
            cache.trim(&mut atlas);
            assert_eq!(cache.rasterized.len(), 1);
            assert!(cache.svgs.contains_key(&handle.id()));
        }

        // Measurement alone also counts as use, independently of uploads.
        let _ = cache.load(&handle);
        cache.trim(&mut atlas);
        assert!(cache.rasterized.is_empty());
        assert!(cache.svgs.contains_key(&handle.id()));
        cache.trim(&mut atlas);
        assert!(cache.svgs.is_empty());
    }
}
