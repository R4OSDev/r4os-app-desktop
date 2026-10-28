//! Native capture uses the same shell/window layer walk as CPU composition.
//! Commands, assets and exact external read identities belong to the output.
use super::{
    compositor::{self, Scene, Stats},
    geometry::{Rect, Regions},
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
use alloc::vec::Vec;
/// Reusing an immutable CPU recording does not assert that any GPU layer is
/// ready. The transaction owner revalidates its own actual residency later.
pub struct RecordingReplay {
    commands: Vec<super::primitive_frame::Command>,
    reusable: [bool; super::layers::CAPACITY],
    frame: u64,
    view: Option<super::output_geometry::topology::Viewport>,
    complete: bool,
}
impl RecordingReplay {
    pub fn new() -> Result<Self> {
        let mut commands = Vec::new();
        commands
            .try_reserve_exact(super::primitive_frame::CAPACITY)
            .map_err(|_| Error::OutOfMemory)?;
        Ok(Self {
            commands,
            reusable: [false; super::layers::CAPACITY],
            frame: 0,
            view: None,
            complete: false,
        })
    }
    fn save(&mut self, cache: &Cache, frame: &mut Frame) {
        self.frame = cache.frame();
        self.view = cache.view();
        self.complete = cache.captured() && cache.complete_scene;
        frame.exchange_commands(&mut self.commands);
    }
    fn prepare(
        &mut self,
        cache: &Cache,
        frame: &mut Frame,
        changed: Option<Regions>,
    ) -> Result<()> {
        self.reusable.fill(false);
        let Some(changed) = changed else {
            return Ok(());
        };
        if !self.complete
            || self.commands.is_empty()
            || self.view.is_none()
            || self.view != cache.view()
            || self.frame.checked_add(1) != Some(cache.frame())
        {
            return Ok(());
        }
        for (i, entry) in cache.entries().iter().enumerate() {
            self.reusable[i] = entry.initialized
                && !entry.external
                && entry.frame == self.frame
                && !changed.intersects(entry.capture_bounds);
        }
        for command in &self.commands {
            if let Some(texture) = command.texture {
                if !frame
                    .assets
                    .contains(command.content, texture, command.source)
                {
                    self.reusable[command.layer as usize] = false;
                }
            }
        }
        for command in &self.commands {
            if self.reusable[command.layer as usize] {
                if let Some(texture) = command.texture {
                    frame.assets.reuse_texture(texture as usize)?;
                }
            }
        }
        Ok(())
    }
    fn replay(&self) -> super::layers::Replay<'_> {
        super::layers::Replay {
            commands: &self.commands,
            reusable: &self.reusable,
        }
    }
}
struct Capture<'a, M: Memory> {
    cache: &'a mut Cache,
    frame: &'a mut Frame,
    engine: &'a mut Engine,
    reads: &'a mut Reads,
    windows: &'a mut Set<M, Consumer>,
    recording_replay: Option<&'a RecordingReplay>,
    finished: bool,
}
impl<M: Memory> Capture<'_, M> {
    fn solid_background(&self, bounds: Rect, rgb: u32) -> Option<u32> {
        // Unit scale has exact integer rectangle edges under all rotations.
        // Other scales retain the original transparent capture until their
        // sampling fringe has a separately established backdrop proof.
        if self.cache.view()?.scale != 120 {
            return None;
        }
        let first = self.cache.commands().first()?;
        let base = &self.cache.entries()[first.entry as usize];
        if base.key != 1
            || base.external
            || base.bounds != self.cache.screen()
            || first.scissor != self.cache.screen()
        {
            return None;
        }
        let clipped = super::scene::intersect(base.capture_bounds, bounds)?;
        if self.cache.commands()[1..].iter().any(|c| {
            self.cache.entries()[c.entry as usize]
                .capture_bounds
                .intersects(clipped)
        }) {
            return None;
        }
        // The current captured background must really be one full solid
        // fill, including recordings reused by CPU. The recorder normally
        // folds an initial clear of the same rectangle into that fill.
        // A wallpaper, text or unrecognized background command rejects it.
        let commands = self.frame.commands();
        let full = Rect::new(0, 0, base.bounds.w, base.bounds.h);
        let valid = |c: &super::primitive_frame::Command| {
            c.layer == first.entry
                && c.texture.is_none()
                && !c.over
                && c.grid.enabled == 0
                && c.target == full
                && c.scissor == full
        };
        let initial = commands.first()?;
        let fill_index = usize::from(valid(initial) && initial.color == 0);
        let fill = commands.get(fill_index)?;
        if !valid(fill)
            || fill.color != (0xff000000 | (rgb & 0xffffff))
            || !fill.opaque
            || commands
                .get(fill_index + 1)
                .is_some_and(|c| c.layer == first.entry)
        {
            return None;
        }
        Some(rgb & 0xffffff)
    }
}
impl<M: Memory> compositor::Capture for Capture<'_, M> {
    fn paint(
        &mut self,
        key: u32,
        bounds: Rect,
        damage: Rect,
        paint: impl FnOnce(&mut Painter<'_, '_>) -> Result<()>,
    ) -> Result<bool> {
        // Only NativeOutput computed this capture's reusable mask. Keep the
        // immutable engine snapshot separate from the mutable cache/frame,
        // and end this borrow before any external read can change the engine.
        let replay = self.cache.view().map(|_| {
            self.recording_replay
                .map(RecordingReplay::replay)
                .unwrap_or_else(|| self.engine.replay())
        });
        self.cache
            .paint(key, bounds, damage, Some(self.frame), replay, paint)
    }
    fn paint_over_solid(
        &mut self,
        key: u32,
        bounds: Rect,
        damage: Rect,
        background: u32,
        paint: impl FnOnce(&mut Painter<'_, '_>) -> Result<()>,
    ) -> Result<bool> {
        let background = self.solid_background(bounds, background);
        let replay = self.cache.view().map(|_| {
            self.recording_replay
                .map(RecordingReplay::replay)
                .unwrap_or_else(|| self.engine.replay())
        });
        self.cache.paint_over_solid(
            key,
            bounds,
            damage,
            Some(self.frame),
            replay,
            background,
            paint,
        )
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
    unsafe {
        capture_impl(
            cache,
            frame,
            engine,
            reads,
            windows,
            mode,
            scene,
            damage,
            fonts_cache,
            fonts,
            icons,
            shapes,
            None,
        )
    }
}
/// # Safety
/// Same read/immutable-map contract as capture. This owner has never submitted
/// GPU work; its CPU replay cannot publish a GPU residency or completion.
pub unsafe fn capture_recording<M: Memory>(
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
    replay: &mut RecordingReplay,
) -> Result<Stats> {
    unsafe {
        capture_impl(
            cache,
            frame,
            engine,
            reads,
            windows,
            mode,
            scene,
            damage,
            fonts_cache,
            fonts,
            icons,
            shapes,
            Some(replay),
        )
    }
}
unsafe fn capture_impl<M: Memory>(
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
    mut recording_replay: Option<&mut RecordingReplay>,
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
    if let Some(replay) = recording_replay.as_deref_mut() {
        replay.save(cache, frame);
    }
    frame.set_mirror(false)?;
    cache.start(mode, Some(frame))?;
    let changed = Some(Regions::from_slice(damage));
    if matches!(mode, Mode::NativeOutput(_)) {
        if let Some(replay) = recording_replay.as_deref_mut() {
            replay.prepare(cache, frame, changed)?;
        } else {
            engine
                .reuse_capture(cache, frame, changed)
                .map_err(|_| Error::Graphics)?;
        }
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
        recording_replay: recording_replay.as_deref(),
        finished: false,
    };
    let stats = unsafe {
        compositor::draw(
            &mut capture,
            desktop,
            external,
            scene,
            if matches!(mode, Mode::NativeOutput(_)) {
                core::slice::from_ref(&desktop)
            } else {
                damage
            },
            fonts_cache,
            fonts,
            icons,
            shapes,
        )
    }?;
    capture.cache.finish(Some(capture.frame))?;
    capture.cache.complete_scene =
        matches!(mode, Mode::NativeOutput(_)) || (damage.len() == 1 && damage[0] == desktop);
    if let Mode::NativeOutput(view) = mode {
        capture
            .engine
            .composition_damage(view, changed)
            .map_err(|_| Error::Graphics)?;
        capture.engine.present_intent = 0;
    }
    capture.finished = true;
    Ok(stats)
}

#[cfg(test)]
#[path = "../../../Tests/RustNative/native_capture.rs"]
mod tests;
