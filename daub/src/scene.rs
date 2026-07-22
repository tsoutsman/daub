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

/// An ordered collection of drawing primitives.
///
/// Consecutive primitives of the same type are stored together in a typed
/// batch. Starting a batch for a different primitive type preserves submission
/// order by creating a new batch.
pub struct Scene {
    pub clip_bounds: Rectangle,
    batches: Vec<Box<dyn ErasedBatch>>,
}

impl Scene {
    #[must_use]
    pub const fn new(clip_bounds: Rectangle) -> Self {
        Self {
            clip_bounds,
            batches: Vec::new(),
        }
    }

    /// Adds a primitive to the scene.
    ///
    /// The primitive is appended to the last batch when its type matches.
    /// Otherwise, a new batch is started.
    pub fn push<P>(&mut self, primitive: P)
    where
        P: Primitive,
    {
        self.batch::<P>().push(primitive);
    }

    /// Adds several primitives to one consecutive batch.
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

    /// Returns a writer for the current batch of `P` primitives.
    ///
    /// A new batch is started if the last batch contains another primitive
    /// type. Holding the writer avoids a type-erased lookup for each push.
    #[expect(clippy::missing_panics_doc, reason = "see below")]
    pub fn batch<P>(&mut self) -> BatchWriter<'_, P>
    where
        P: Primitive,
    {
        let can_reuse_last = self
            .batches
            .last()
            .is_some_and(|batch| batch.as_any().is::<TypedBatch<P>>());

        if !can_reuse_last {
            self.batches.push(Box::new(TypedBatch::<P>::new()));
        }

        #[expect(clippy::expect_used, reason = "see below")]
        let batch = self
            .batches
            .last_mut()
            .and_then(|batch| batch.as_any_mut().downcast_mut::<TypedBatch<P>>())
            .expect("the last batch was just created or type-checked");

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

    pub(crate) fn batches_mut(&mut self) -> impl ExactSizeIterator<Item = &mut dyn ErasedBatch> {
        self.batches.iter_mut().map(Box::as_mut)
    }
}

impl fmt::Debug for Scene {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Scene")
            .field("clip_bounds", &self.clip_bounds)
            .field("len", &self.len())
            .field("batch_count", &self.batch_count())
            .finish_non_exhaustive()
    }
}

/// A typed writer for one consecutive primitive batch.
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
    fn primitive_type_id(&self) -> TypeId;

    fn renderer_type_id(&self) -> TypeId;

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn create_renderer(
        &self,
        device: &wgpu::Device,
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
    fn primitive_type_id(&self) -> TypeId {
        TypeId::of::<P>()
    }

    fn renderer_type_id(&self) -> TypeId {
        TypeId::of::<P::Renderer>()
    }

    fn len(&self) -> usize {
        self.primitives.len()
    }

    fn create_renderer(
        &self,
        device: &wgpu::Device,
        config: &RendererConfig,
        pipeline_cache: &mut RenderPipelineCache,
    ) -> Box<dyn ErasedPrimitiveRenderer> {
        Box::new(P::Renderer::new(device, config, pipeline_cache))
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

    pub(crate) fn downcast_mut<P>(&mut self) -> Option<&mut TypedBatch<P>>
    where
        P: Primitive,
    {
        self.as_any_mut().downcast_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::Scene;
    use crate::{
        geometry::Rectangle,
        render::{Primitive, PrimitiveRenderer, RenderPipelineCache, RendererConfig},
    };

    struct PrimitiveA(u32);
    struct PrimitiveB;

    struct RendererA(wgpu::RenderPipeline);
    struct RendererB(wgpu::RenderPipeline);

    impl Primitive for PrimitiveA {
        type Renderer = RendererA;
    }

    impl Primitive for PrimitiveB {
        type Renderer = RendererB;
    }

    impl PrimitiveRenderer for RendererA {
        type Primitive = PrimitiveA;

        fn new(
            device: &wgpu::Device,
            config: &RendererConfig,
            cache: &mut RenderPipelineCache,
        ) -> Self {
            Self(cache.get_or_create::<Self>(device, config))
        }

        fn build_pipeline(_: &wgpu::Device, _: &RendererConfig) -> wgpu::RenderPipeline {
            std::process::abort()
        }

        fn render_pipeline(&self) -> &wgpu::RenderPipeline {
            &self.0
        }

        fn render_batch(
            &mut self,
            _: &[Self::Primitive],
            _: &mut wgpu::RenderPass<'_>,
            _: Option<wgpu::BufferSlice<'_>>,
        ) {
        }
    }

    impl PrimitiveRenderer for RendererB {
        type Primitive = PrimitiveB;

        fn new(
            device: &wgpu::Device,
            config: &RendererConfig,
            cache: &mut RenderPipelineCache,
        ) -> Self {
            Self(cache.get_or_create::<Self>(device, config))
        }

        fn build_pipeline(_: &wgpu::Device, _: &RendererConfig) -> wgpu::RenderPipeline {
            std::process::abort()
        }

        fn render_pipeline(&self) -> &wgpu::RenderPipeline {
            &self.0
        }

        fn render_batch(
            &mut self,
            _: &[Self::Primitive],
            _: &mut wgpu::RenderPass<'_>,
            _: Option<wgpu::BufferSlice<'_>>,
        ) {
        }
    }

    #[test]
    fn groups_consecutive_primitives() {
        let bounds = Rectangle::default();
        let mut scene = Scene::new(bounds);

        scene.push(PrimitiveA(1));
        scene.push(PrimitiveA(2));
        scene.push(PrimitiveB);
        scene.push(PrimitiveB);

        assert_eq!(scene.len(), 4);
        assert_eq!(scene.batch_count(), 2);

        let batches = scene.batches().collect::<Vec<_>>();
        let Some(first) = batches[0].downcast_ref::<PrimitiveA>() else {
            std::process::abort();
        };

        assert_eq!(first.primitives().len(), 2);
        assert_eq!(first.primitives()[0].0, 1);
        assert_eq!(first.primitives()[1].0, 2);
    }

    #[test]
    fn preserves_interleaved_submission_order() {
        let bounds = Rectangle::default();
        let mut scene = Scene::new(bounds);

        scene.push(PrimitiveA(1));
        scene.push(PrimitiveB);
        scene.push(PrimitiveA(2));

        assert_eq!(scene.len(), 3);
        assert_eq!(scene.batch_count(), 3);

        let batches = scene.batches().collect::<Vec<_>>();
        assert!(batches[0].downcast_ref::<PrimitiveA>().is_some());
        assert!(batches[1].downcast_ref::<PrimitiveB>().is_some());
        assert!(batches[2].downcast_ref::<PrimitiveA>().is_some());
    }

    #[test]
    fn empty_extend_does_not_create_a_batch() {
        let mut scene = Scene::new(Rectangle::default());

        scene.extend::<PrimitiveA, _>(std::iter::empty());

        assert!(scene.is_empty());
        assert_eq!(scene.batch_count(), 0);
    }
}
