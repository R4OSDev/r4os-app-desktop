//! Native capture uses the same shell/window layer walk as CPU composition.
//! Commands, assets and exact external read identities belong to the output.
use super::{
    compositor::{self, Scene, Stats},
    geometry::Rect,
    gpu::engine::Engine,
    gpu::receipt::Consumer,
    layers::{Cache, Mode},
    painter::Painter,
    primitive_assets::{Error, Result},
    primitive_frame::Frame,
    text::cache::{Cache as FontCache, Fonts},
};
use crate::{
    hosted::{native_images::Reads, replay::Shapes, window_image::Memory, window_set::Set},
    shell::{desktop_view, taskbar_view},
};
struct Capture<'a, M: Memory> {
    cache: &'a mut Cache,
    frame: &'a mut Frame,
    engine: &'a mut Engine,
    reads: &'a mut Reads,
    windows: &'a mut Set<M, Consumer>,
    finished: bool,
}
impl<M: Memory> compositor::Capture for Capture<'_, M> {
    fn paint(
        &mut self,
        key: u32,
        bounds: Rect,
        damage: Rect,
        paint: impl FnOnce(&mut Painter<'_, '_>) -> Result<()>,
    ) -> Result<bool> {
        self.cache
            .paint(key, bounds, damage, Some(self.frame), None, paint)
    }
    fn external(&mut self, window: usize, key: u32, bounds: Rect, damage: &[Rect]) -> Result<()> {
        self.reads.capture(
            self.windows,
            self.engine,
            self.cache,
            window,
            key,
            bounds,
            damage,
        )
    }
}
impl<M: Memory> Drop for Capture<'_, M> {
    fn drop(&mut self) {
        if !self.finished {
            self.cache.cancel();
        }
    }
}
/// # Safety
/// Hosted frame maps are immutable through capture. The caller retains all
/// returned BO reads until Engine completes/discards after physical retirement;
/// preparation errors and cancellation do not release those reads implicitly.
pub unsafe fn capture<M: Memory>(
    cache: &mut Cache,
    frame: &mut Frame,
    engine: &mut Engine,
    reads: &mut Reads,
    windows: &mut Set<M, Consumer>,
    mode: Mode,
    scene: &Scene<'_>,
    damage: &[Rect],
    fonts_cache: &mut FontCache,
    fonts: &mut impl Fonts,
    icons: &mut (impl desktop_view::Icons + taskbar_view::Icons),
    shapes: &mut impl Shapes,
) -> Result<Stats> {
    if engine.active()
        || !reads.empty()
        || !matches!(
            scene.path,
            super::coverage::Path::Native | super::coverage::Path::Recorded
        )
    {
        return Err(Error::State);
    }
    engine.clear_sources().map_err(|_| Error::State)?;
    frame.set_mirror(false)?;
    cache.start(mode, Some(frame))?;
    let changed = damage.iter().copied().reduce(Rect::merged);
    if matches!(mode, Mode::NativeOutput(_)) {
        engine.reuse_capture(cache, frame, changed).map_err(|_| Error::Graphics)?;
    }
    let desktop = match mode {
        Mode::Desktop(bounds) | Mode::CpuOutput(bounds) => bounds,
        Mode::NativeOutput(view) => super::output_geometry::logical(view)?,
    };
    let external = reads.available(windows);
    let mut capture = Capture {
        cache,
        frame,
        engine,
        reads,
        windows,
        finished: false,
    };
    let stats = unsafe {
        compositor::draw(
            &mut capture,
            desktop,
            external,
            scene,
            if matches!(mode, Mode::NativeOutput(_)) { core::slice::from_ref(&desktop) } else { damage },
            fonts_cache,
            fonts,
            icons,
            shapes,
        )
    }?;
    capture.cache.finish(Some(capture.frame))?;
    capture.cache.complete_scene = matches!(mode, Mode::NativeOutput(_))
        || (damage.len() == 1 && damage[0] == desktop);
    if let Mode::NativeOutput(view) = mode {
        capture.engine.composition_damage(view, changed).map_err(|_| Error::Graphics)?;
        capture.engine.present_intent = 0;
    }
    capture.finished = true;
    Ok(stats)
}
