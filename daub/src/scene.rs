use std::{
    any::{Any, TypeId},
    fmt,
};

use crate::{
    geometry::Rectangle,
    render::{
        ErasedPrimitiveRenderer, Primitive, PrimitiveRenderer, RenderPipelineCache, RendererConfig,
    },
};

/// A strictly ordered collection of drawing [`Layer`]s.
///
/// Every primitive in an earlier layer is rendered before every primitive in a
/// later layer. Primitive submission order within a layer follows [`Layer`]'s
/// type-batching rules.
#[derive(Default)]
pub struct Scene {
    layers: Vec<Layer>,
}

impl Scene {
    #[must_use]
    pub const fn new() -> Self {
        Self { layers: Vec::new() }
    }

    /// Appends a strict painter's-order layer.
    pub fn push(&mut self, layer: Layer) {
        self.layers.push(layer);
    }

    /// Appends strict painter's-order layers.
    pub fn extend<I>(&mut self, layers: I)
    where
        I: IntoIterator<Item = Layer>,
    {
        self.layers.extend(layers);
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// Returns the number of layers, including empty layers.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.layers.len()
    }

    #[must_use]
    pub fn primitive_count(&self) -> usize {
        self.layers.iter().map(Layer::len).sum()
    }

    pub fn clear(&mut self) {
        self.layers.clear();
    }

    pub(crate) fn layers(&self) -> impl ExactSizeIterator<Item = &Layer> {
        self.layers.iter()
    }
}

impl<I> From<I> for Scene
where
    I: IntoIterator<Item = Layer>,
{
    fn from(layers: I) -> Self {
        Self {
            layers: layers.into_iter().collect(),
        }
    }
}

impl From<Layer> for Scene {
    fn from(layer: Layer) -> Self {
        Self::from([layer])
    }
}

impl fmt::Debug for Scene {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Scene")
            .field("layer_count", &self.len())
            .field("primitive_count", &self.primitive_count())
            .finish_non_exhaustive()
    }
}

/// A clipped collection of primitives grouped by type.
///
/// A layer contains at most one batch for each primitive type. The first
/// occurrence of a type establishes that batch's position relative to other
/// types; later primitives of the same type join the existing batch regardless
/// of intervening submissions. Same-type insertion order is preserved, while
/// cross-type submission order after first occurrence is intentionally ignored.
pub struct Layer {
    pub clip_bounds: Rectangle,
    batches: Vec<Box<dyn ErasedBatch>>,
}

impl Layer {
    #[must_use]
    pub const fn new(clip_bounds: Rectangle) -> Self {
        Self {
            clip_bounds,
            batches: Vec::new(),
        }
    }

    /// Adds a primitive to the layer.
    ///
    /// The primitive is appended to this layer's existing batch for its type.
    /// A new batch is created at the end when the type has not appeared in the
    /// layer before.
    pub fn push<P>(&mut self, primitive: P)
    where
        P: Primitive,
    {
        self.batch::<P>().push(primitive);
    }

    /// Adds several primitives to one type batch.
    ///
    /// An empty iterator does not create a batch.
    pub fn extend<P, I>(&mut self, primitives: I)
    where
        P: Primitive,
        I: IntoIterator<Item = P>,
    {
        let mut primitives = primitives.into_iter();
        let Some(primitive) = primitives.next() else {
            return;
        };

        let mut batch = self.batch::<P>();
        batch.push(primitive);
        batch.extend(primitives);
    }

    /// Returns a writer for this layer's batch of `P` primitives.
    ///
    /// A new batch is created at the end if `P` has not appeared in the layer.
    /// Holding the writer avoids a type-erased lookup for each push.
    #[expect(clippy::missing_panics_doc, reason = "see below")]
    pub fn batch<P>(&mut self) -> BatchWriter<'_, P>
    where
        P: Primitive,
    {
        let batch_index = self
            .batches
            .iter()
            .position(|batch| batch.as_any().is::<TypedBatch<P>>())
            .unwrap_or_else(|| {
                self.batches.push(Box::new(TypedBatch::<P>::new()));
                self.batches.len() - 1
            });

        #[expect(clippy::expect_used, reason = "see below")]
        let batch = self
            .batches
            .get_mut(batch_index)
            .and_then(|batch| batch.as_any_mut().downcast_mut::<TypedBatch<P>>())
            .expect("the batch was just created or type-checked");

        BatchWriter { batch }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.batches.iter().all(|batch| batch.is_empty())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.batches.iter().map(|batch| batch.len()).sum()
    }

    #[must_use]
    pub fn batch_count(&self) -> usize {
        self.batches.len()
    }

    pub fn clear(&mut self) {
        self.batches.clear();
    }

    pub(crate) fn batches(&self) -> impl ExactSizeIterator<Item = &dyn ErasedBatch> {
        self.batches.iter().map(Box::as_ref)
    }
}

impl fmt::Debug for Layer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Layer")
            .field("clip_bounds", &self.clip_bounds)
            .field("len", &self.len())
            .field("batch_count", &self.batch_count())
            .finish_non_exhaustive()
    }
}

/// A typed writer for one layer-local primitive batch.
pub struct BatchWriter<'a, P>
where
    P: Primitive,
{
    batch: &'a mut TypedBatch<P>,
}

impl<P> BatchWriter<'_, P>
where
    P: Primitive,
{
    pub fn push(&mut self, primitive: P) {
        self.batch.primitives.push(primitive);
    }

    pub fn extend<I>(&mut self, primitives: I)
    where
        I: IntoIterator<Item = P>,
    {
        self.batch.primitives.extend(primitives);
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.batch.primitives.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.batch.primitives.len()
    }
}

impl<P> fmt::Debug for BatchWriter<'_, P>
where
    P: Primitive,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BatchWriter")
            .field("primitive", &std::any::type_name::<P>())
            .field("len", &self.len())
            .finish()
    }
}

pub(crate) struct TypedBatch<P> {
    primitives: Vec<P>,
}

impl<P> TypedBatch<P> {
    const fn new() -> Self {
        Self {
            primitives: Vec::new(),
        }
    }

    pub(crate) fn primitives(&self) -> &[P] {
        &self.primitives
    }

    pub(crate) fn primitives_mut(&mut self) -> &mut [P] {
        &mut self.primitives
    }
}

pub(crate) trait ErasedBatch: Any {
    fn renderer_type_id(&self) -> TypeId;

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn create_renderer(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        config: &RendererConfig,
        pipeline_cache: &mut RenderPipelineCache,
    ) -> Box<dyn ErasedPrimitiveRenderer>;

    fn as_any(&self) -> &dyn Any;

    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<P> ErasedBatch for TypedBatch<P>
where
    P: Primitive,
{
    fn renderer_type_id(&self) -> TypeId {
        TypeId::of::<P::Renderer>()
    }

    fn len(&self) -> usize {
        self.primitives.len()
    }

    fn create_renderer(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        config: &RendererConfig,
        pipeline_cache: &mut RenderPipelineCache,
    ) -> Box<dyn ErasedPrimitiveRenderer> {
        Box::new(P::Renderer::new(device, queue, config, pipeline_cache))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl dyn ErasedBatch + '_ {
    pub(crate) fn downcast_ref<P>(&self) -> Option<&TypedBatch<P>>
    where
        P: Primitive,
    {
        self.as_any().downcast_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::{Layer, Scene};
    use crate::{
        geometry::Rectangle,
        render::{Primitive, PrimitiveRenderer, RenderPipelineCache, RendererConfig},
    };

    struct PrimitiveA(u32);
    struct PrimitiveB;

    struct RendererA;
    struct RendererB;

    impl Primitive for PrimitiveA {
        type Renderer = RendererA;
    }

    impl Primitive for PrimitiveB {
        type Renderer = RendererB;
    }

    impl PrimitiveRenderer for RendererA {
        type Primitive = PrimitiveA;

        fn new(
            _: &wgpu::Device,
            _: &wgpu::Queue,
            _: &RendererConfig,
            _: &mut RenderPipelineCache,
        ) -> Self {
            Self
        }

        fn render_batch(
            &mut self,
            _: &[Self::Primitive],
            _: &mut wgpu::RenderPass<'_>,
            _: Option<wgpu::BufferSlice<'_>>,
        ) -> Result<(), crate::render::PrimitiveRendererError> {
            Ok(())
        }
    }

    impl PrimitiveRenderer for RendererB {
        type Primitive = PrimitiveB;

        fn new(
            _: &wgpu::Device,
            _: &wgpu::Queue,
            _: &RendererConfig,
            _: &mut RenderPipelineCache,
        ) -> Self {
            Self
        }

        fn render_batch(
            &mut self,
            _: &[Self::Primitive],
            _: &mut wgpu::RenderPass<'_>,
            _: Option<wgpu::BufferSlice<'_>>,
        ) -> Result<(), crate::render::PrimitiveRendererError> {
            Ok(())
        }
    }

    #[test]
    fn groups_interleaved_primitives_by_type() {
        let bounds = Rectangle::default();
        let mut layer = Layer::new(bounds);

        layer.push(PrimitiveA(1));
        layer.push(PrimitiveB);
        layer.push(PrimitiveA(2));
        layer.push(PrimitiveB);

        assert_eq!(layer.len(), 4);
        assert_eq!(layer.batch_count(), 2);

        let batches = layer.batches().collect::<Vec<_>>();
        let Some(first) = batches[0].downcast_ref::<PrimitiveA>() else {
            std::process::abort();
        };

        assert_eq!(first.primitives().len(), 2);
        assert_eq!(first.primitives()[0].0, 1);
        assert_eq!(first.primitives()[1].0, 2);
    }

    #[test]
    fn type_order_follows_first_appearance() {
        let bounds = Rectangle::default();
        let mut layer = Layer::new(bounds);

        layer.push(PrimitiveB);
        layer.push(PrimitiveA(1));
        layer.push(PrimitiveB);

        let batches = layer.batches().collect::<Vec<_>>();
        assert!(batches[0].downcast_ref::<PrimitiveB>().is_some());
        assert!(batches[1].downcast_ref::<PrimitiveA>().is_some());
    }

    #[test]
    fn scene_preserves_layer_order() {
        let mut first = Layer::new(Rectangle::default());
        first.push(PrimitiveA(1));
        let mut second = Layer::new(Rectangle::default());
        second.push(PrimitiveB);
        let mut scene = Scene::new();

        scene.push(first);
        scene.push(second);

        assert_eq!(scene.len(), 2);
        assert_eq!(scene.primitive_count(), 2);
        let layers = scene.layers().collect::<Vec<_>>();
        let Some(first_batch) = layers[0].batches().next() else {
            std::process::abort();
        };
        let Some(second_batch) = layers[1].batches().next() else {
            std::process::abort();
        };
        assert!(first_batch.downcast_ref::<PrimitiveA>().is_some());
        assert!(second_batch.downcast_ref::<PrimitiveB>().is_some());
    }

    #[test]
    fn scene_can_be_created_from_owned_layers() {
        let mut first = Layer::new(Rectangle::default());
        first.push(PrimitiveA(1));
        let mut second = Layer::new(Rectangle::default());
        second.push(PrimitiveB);

        let scene = Scene::from([first, second]);

        assert_eq!(scene.len(), 2);
        assert_eq!(scene.primitive_count(), 2);
    }

    #[test]
    fn scene_can_be_created_from_one_layer() {
        let mut layer = Layer::new(Rectangle::default());
        layer.push(PrimitiveA(1));

        let scene = Scene::from(layer);

        assert_eq!(scene.len(), 1);
        assert_eq!(scene.primitive_count(), 1);
    }

    #[test]
    fn empty_extend_does_not_create_a_batch() {
        let mut layer = Layer::new(Rectangle::default());

        layer.extend::<PrimitiveA, _>(std::iter::empty());

        assert!(layer.is_empty());
        assert_eq!(layer.batch_count(), 0);
    }
}
