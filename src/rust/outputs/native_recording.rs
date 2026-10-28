//! The one newest CPU scene. This engine records BO identities only; it never
//! owns a device session, GPU allocation, submitted job or swapchain image.
use super::*;
use crate::render::native_scene::RecordingReplay;

pub(super) struct Recording {
    pub(super) cache: Cache,
    pub(super) frame: Frame,
    pub(super) recorder: Engine,
    pub(super) reads: Reads,
    replay: RecordingReplay,
    pub(super) damage: Option<Regions>,
    pub(super) input_ns: u64,
    pub(super) cursor: CaptureCursor,
    pub(super) captured: bool,
    pub(super) generation: (u64, u64),
}
impl Recording {
    pub(super) fn seed(
        cache: &Cache,
        frame: &Frame,
        generation: (u64, u64),
    ) -> Result<Self, CaptureError> {
        Ok(Self {
            cache: cache.fork_recording()?,
            frame: frame.fork()?,
            recorder: Engine::new().map_err(|_| CaptureError::OutOfMemory)?,
            reads: Reads::default(),
            replay: RecordingReplay::new()?,
            damage: None,
            input_ns: 0,
            cursor: CaptureCursor::default(),
            captured: false,
            generation,
        })
    }
    pub(super) fn release<M: Memory>(&mut self, windows: &mut Set<M, Consumer>) -> bool {
        // No GPU API is used by this owner. None means the exact borrowed
        // source was never submitted, not an invented execution receipt.
        self.recorder
            .return_recording(&mut self.reads.completion(windows))
            && self.reads.empty()
    }
    pub(super) unsafe fn capture<M: Memory>(
        &mut self,
        windows: &mut Set<M, Consumer>,
        mode: Mode,
        scene: &Scene<'_>,
        damage: &[Rect],
        fonts_cache: &mut FontCache,
        fonts: &mut impl Fonts,
        icons: &mut (impl desktop_view::Icons + taskbar_view::Icons),
        shapes: &mut impl Shapes,
        input_ns: u64,
        cursor: CaptureCursor,
    ) -> Result<Stats, CaptureError> {
        if !self.release(windows) {
            return Err(CaptureError::State);
        }
        self.captured = false;
        // A legacy primary delta cannot serve as a standalone newest scene.
        // Record it completely; viewport-native capture already does this.
        let full = match mode {
            Mode::Desktop(bounds) => Some(bounds),
            _ => None,
        };
        let regions = full.as_ref().map_or(damage, core::slice::from_ref);
        let result = unsafe {
            native_scene::capture_recording(
                &mut self.cache,
                &mut self.frame,
                &mut self.recorder,
                &mut self.reads,
                windows,
                mode,
                scene,
                regions,
                fonts_cache,
                fonts,
                icons,
                shapes,
                &mut self.replay,
            )
        };
        if result.is_ok() {
            let changed = Regions::from_slice(regions);
            self.damage = Some(self.damage.map_or(changed, |old| old.merged(changed)));
            self.input_ns = input_ns;
            self.cursor = cursor;
            self.captured = true;
        }
        result
    }
}
