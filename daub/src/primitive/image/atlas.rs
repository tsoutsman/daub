use std::{cell::RefCell, error, fmt, rc::Rc};

const BYTES_PER_PIXEL: usize = 4;
const BYTES_PER_PIXEL_U32: u32 = 4;
const GUTTER: u32 = 1;
const INITIAL_TEXTURE_SIZE: u32 = 256;

/// A reference-counted image region within an [`ImageAtlas`].
///
/// Handles remain valid when the atlas grows and after the [`ImageAtlas`]
/// value itself is dropped. Atlas images are intended to remain on the thread
/// where their atlas was created.
#[derive(Clone)]
pub struct AtlasImage {
    pub(crate) storage: Rc<RefCell<wgpu::Texture>>,
    pub(crate) region: AtlasRegion,
}

impl AtlasImage {
    /// Returns the source image width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.region.width
    }

    /// Returns the source image height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.region.height
    }
}

impl fmt::Debug for AtlasImage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AtlasImage")
            .field("atlas", &Rc::as_ptr(&self.storage))
            .field("region", &self.region)
            .finish()
    }
}

/// A growing GPU texture containing packed RGBA8 images.
///
/// Images are inserted incrementally. When the current texture has no room,
/// the atlas allocates a geometrically larger texture and copies its existing
/// pixels on the GPU. Previously returned [`AtlasImage`] handles keep stable
/// pixel regions throughout growth.
///
/// The atlas and its image handles are intended to remain on the thread where
/// the atlas was created.
pub struct ImageAtlas {
    device: wgpu::Device,
    queue: wgpu::Queue,
    storage: Rc<RefCell<wgpu::Texture>>,
    shelves: Vec<Shelf>,
    width: u32,
    height: u32,
    limit: u32,
}

impl ImageAtlas {
    #[must_use]
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let limit = device.limits().max_texture_dimension_2d.max(1);
        Self::with_initial_size(device, queue, INITIAL_TEXTURE_SIZE.min(limit), limit)
    }

    /// Adds one image containing tightly packed, straight-alpha RGBA8 pixels.
    ///
    /// The returned handle remains valid if later insertions grow the atlas.
    ///
    /// # Errors
    ///
    /// Returns an error for zero dimensions, invalid pixel data length, or an
    /// image or atlas exceeding the device's maximum two-dimensional texture
    /// size.
    pub fn add_rgba8(
        &mut self,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<AtlasImage, ImageAtlasError> {
        if width == 0 || height == 0 {
            return Err(ImageAtlasError::EmptyImage { width, height });
        }

        let maximum_image_dimension = self.limit.saturating_sub(GUTTER * 2);
        if width > maximum_image_dimension || height > maximum_image_dimension {
            return Err(ImageAtlasError::ImageTooLarge {
                width,
                height,
                limit: self.limit,
            });
        }

        let expected_len = image_byte_len(width, height);
        if pixels.len() != expected_len {
            return Err(ImageAtlasError::InvalidDataLength {
                width,
                height,
                expected: expected_len,
                actual: pixels.len(),
            });
        }

        let padded_width = width + GUTTER * 2;
        let padded_height = height + GUTTER * 2;

        let (padded_x, padded_y) = loop {
            if let Some(position) = self.allocate(padded_width, padded_height) {
                break position;
            }
            self.grow(padded_width, padded_height)?;
        };
        let region = AtlasRegion {
            x: padded_x + GUTTER,
            y: padded_y + GUTTER,
            width,
            height,
        };
        let padded_pixels = extrude_gutter(width, height, pixels);
        let texture = self.storage.borrow();
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: padded_x,
                    y: padded_y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &padded_pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_width * BYTES_PER_PIXEL_U32),
                rows_per_image: Some(padded_height),
            },
            wgpu::Extent3d {
                width: padded_width,
                height: padded_height,
                depth_or_array_layers: 1,
            },
        );
        drop(texture);

        Ok(AtlasImage {
            storage: Rc::clone(&self.storage),
            region,
        })
    }

    fn with_initial_size(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        initial_size: u32,
        limit: u32,
    ) -> Self {
        let texture = create_texture(device, initial_size, initial_size);

        Self {
            device: device.clone(),
            queue: queue.clone(),
            storage: Rc::new(RefCell::new(texture)),
            shelves: Vec::new(),
            width: initial_size,
            height: initial_size,
            limit,
        }
    }

    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        for shelf in &mut self.shelves {
            let right = shelf.next_x + width;
            if height <= shelf.height && right <= self.width {
                let position = (shelf.next_x, shelf.y);
                shelf.next_x = right;
                return Some(position);
            }
        }

        let y = self
            .shelves
            .last()
            .map_or(0, |shelf| shelf.y + shelf.height);
        if y + height > self.height || width > self.width {
            return None;
        }

        self.shelves.push(Shelf {
            y,
            height,
            next_x: width,
        });
        Some((0, y))
    }

    fn grow(&mut self, required_width: u32, required_height: u32) -> Result<(), ImageAtlasError> {
        let mut new_width = self.width.max(required_width);
        let mut new_height = self.height.max(required_height);

        if new_width == self.width && new_height == self.height {
            if self.width <= self.height && self.width < self.limit {
                new_width = grow_dimension(self.width, self.limit);
            } else if self.height < self.limit {
                new_height = grow_dimension(self.height, self.limit);
            } else if self.width < self.limit {
                new_width = grow_dimension(self.width, self.limit);
            } else {
                return Err(ImageAtlasError::AtlasFull { limit: self.limit });
            }
        }

        let texture = create_texture(&self.device, new_width, new_height);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("daub image atlas growth encoder"),
            });
        let mut stored_texture = self.storage.borrow_mut();
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &stored_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);

        *stored_texture = texture;
        drop(stored_texture);
        self.width = new_width;
        self.height = new_height;
        Ok(())
    }
}

impl fmt::Debug for ImageAtlas {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageAtlas")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

/// An error encountered while adding an image to an atlas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageAtlasError {
    EmptyImage {
        width: u32,
        height: u32,
    },
    InvalidDataLength {
        width: u32,
        height: u32,
        expected: usize,
        actual: usize,
    },
    ImageTooLarge {
        width: u32,
        height: u32,
        limit: u32,
    },
    AtlasFull {
        limit: u32,
    },
}

impl fmt::Display for ImageAtlasError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyImage { width, height } => {
                write!(
                    formatter,
                    "image dimensions must be non-zero, got {width}x{height}"
                )
            }
            Self::InvalidDataLength {
                width,
                height,
                expected,
                actual,
            } => write!(
                formatter,
                "RGBA8 image {width}x{height} requires {expected} bytes, got {actual}"
            ),
            Self::ImageTooLarge {
                width,
                height,
                limit,
            } => write!(
                formatter,
                "image {width}x{height} plus its filtering gutter exceeds the device texture \
                 limit of {limit}x{limit}"
            ),
            Self::AtlasFull { limit } => {
                write!(
                    formatter,
                    "image atlas reached the device texture limit of {limit}x{limit}"
                )
            }
        }
    }
}

impl error::Error for ImageAtlasError {}

#[derive(Debug, Clone, Copy)]
struct Shelf {
    y: u32,
    height: u32,
    next_x: u32,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AtlasRegion {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

fn create_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("daub image atlas"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn grow_dimension(current: u32, limit: u32) -> u32 {
    (current * 2).min(limit)
}

fn image_byte_len(width: u32, height: u32) -> usize {
    width as usize * height as usize * BYTES_PER_PIXEL
}

fn extrude_gutter(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    let padded_width = width + GUTTER * 2;
    let padded_height = height + GUTTER * 2;
    let mut padded = vec![0; image_byte_len(padded_width, padded_height)];

    for destination_y in 0..padded_height {
        let source_y = destination_y.saturating_sub(GUTTER).min(height - 1);
        for destination_x in 0..padded_width {
            let source_x = destination_x.saturating_sub(GUTTER).min(width - 1);
            let source_offset = pixel_offset(source_x, source_y, width);
            let destination_offset = pixel_offset(destination_x, destination_y, padded_width);
            padded[destination_offset..destination_offset + BYTES_PER_PIXEL]
                .copy_from_slice(&pixels[source_offset..source_offset + BYTES_PER_PIXEL]);
        }
    }

    padded
}

fn pixel_offset(x: u32, y: u32, width: u32) -> usize {
    (y as usize * width as usize + x as usize) * BYTES_PER_PIXEL
}

#[cfg(test)]
mod tests {
    use super::{BYTES_PER_PIXEL, ImageAtlas, ImageAtlasError, extrude_gutter, pixel_offset};

    #[test]
    fn rejects_invalid_source_images() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut atlas = ImageAtlas::new(&device, &queue);

        assert_eq!(
            atlas.add_rgba8(0, 1, &[]).err(),
            Some(ImageAtlasError::EmptyImage {
                width: 0,
                height: 1,
            })
        );
        assert_eq!(
            atlas.add_rgba8(2, 2, &[0; 15]).err(),
            Some(ImageAtlasError::InvalidDataLength {
                width: 2,
                height: 2,
                expected: 16,
                actual: 15,
            })
        );
    }

    #[test]
    fn add_returns_the_final_image_handle() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut atlas = ImageAtlas::new(&device, &queue);
        let image = atlas
            .add_rgba8(4, 2, &[255; 32])
            .unwrap_or_else(|_| std::process::abort());

        assert_eq!((image.width(), image.height()), (4, 2));
    }

    #[test]
    fn existing_handles_follow_texture_growth() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut atlas = ImageAtlas::with_initial_size(&device, &queue, 4, 64);
        let first = atlas
            .add_rgba8(1, 1, &[255; BYTES_PER_PIXEL])
            .unwrap_or_else(|_| std::process::abort());
        let original_region = first.region;
        let second = atlas
            .add_rgba8(1, 1, &[0; BYTES_PER_PIXEL])
            .unwrap_or_else(|_| std::process::abort());
        let storage = first.storage.borrow();

        assert_eq!(first.region, original_region);
        assert_eq!(second.region.x, 4);
        assert_eq!((atlas.width, atlas.height), (8, 4));
        assert_eq!((storage.width(), storage.height()), (8, 4));
    }

    #[test]
    fn grows_exactly_to_fit_an_oversized_image() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut atlas = ImageAtlas::with_initial_size(&device, &queue, 4, 64);

        atlas
            .add_rgba8(7, 1, &[255; 28])
            .unwrap_or_else(|_| std::process::abort());

        assert_eq!((atlas.width, atlas.height), (9, 4));
    }

    #[test]
    fn handles_keep_the_texture_alive_after_the_atlas_is_dropped() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let image = {
            let mut atlas = ImageAtlas::new(&device, &queue);
            atlas
                .add_rgba8(1, 1, &[255; BYTES_PER_PIXEL])
                .unwrap_or_else(|_| std::process::abort())
        };

        assert_eq!((image.width(), image.height()), (1, 1));
        assert!(image.storage.borrow().width() > 0);
    }

    #[test]
    fn extrudes_edge_pixels_into_the_gutter() {
        let red = [255, 0, 0, 255];
        let blue = [0, 0, 255, 128];
        let padded = extrude_gutter(2, 1, &[red, blue].concat());

        for y in 0..3 {
            assert_eq!(pixel(&padded, 4, 0, y), red);
            assert_eq!(pixel(&padded, 4, 1, y), red);
            assert_eq!(pixel(&padded, 4, 2, y), blue);
            assert_eq!(pixel(&padded, 4, 3, y), blue);
        }
    }

    fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; BYTES_PER_PIXEL] {
        let offset = pixel_offset(x, y, width);
        pixels[offset..offset + BYTES_PER_PIXEL]
            .try_into()
            .unwrap_or_else(|_| std::process::abort())
    }
}
