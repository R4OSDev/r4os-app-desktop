//! Main-task frame scheduling. CPU pixels, painter caches and copied counters
//! have one owner; output receipts and GUI buffer lifetimes stay external.
use super::{
    compositor::{self, Completed, Scene},
    coverage::Path,
    display::{Display, Presentation},
    geometry::{Dirty, MAX_DAMAGE_REGIONS, Rect},
    icons,
    layers::Mode,
    primitive_assets::{Error, Result},
    scene::CpuScene,
    shapes::Renderer,
    software::{self, Owner},
    text::{cache::Cache, native::SessionFonts},
};
use crate::{
    hosted::{window_image::Memory, window_set::Set},
    outputs::manager::Manager,
    render::gpu::receipt::Consumer,
};
use alloc::vec::Vec;
use r4os_sdk::{App, StartupInfo, graphics::Graphics, gui::Session, resources::Resources};
#[derive(Default)]
pub struct Stats {
    pub redraws: u64,
    pub full: u64,
    pub mixed: u64,
    pub cursor: u64,
    pub last_damage: u8,
    pub pixels: u64,
    pub presents: u64,
    pub present_errors: u64,
    pub present_lost_frames: u64,
    present_generation_last: u64,
    pub copied_bytes: u64,
    pub last_composition_ns: u64,
    pub last_present_ns: u64,
}
pub struct Frame {
    pub completion: Completed,
    pub presentation: Presentation,
    pub generation: u64,
    pub started_ns: u64,
    pub completed_ns: u64,
}
pub struct Frames {
    pixels: Vec<u32>,
    width: i32,
    height: i32,
    pub current: bool,
    pub generation: u64,
    pub software: Owner,
    pub fonts: Cache,
    pub icons: icons::Cache,
    pub shapes: Renderer,
    pub stats: Stats,
}
impl Frames {
    pub fn new() -> Result<Self> {
        Ok(Self {
            pixels: Vec::new(),
            width: 0,
            height: 0,
            current: false,
            generation: 0,
            software: Owner::new(128 * 1024 * 1024)?,
            fonts: Cache::new()?,
            icons: icons::Cache::new()?,
            shapes: Renderer::default(),
            stats: Stats::default(),
        })
    }
    pub fn resize(&mut self, width: i32, height: i32) -> Result<bool> {
        if (width, height) == (self.width, self.height) {
            return Ok(false);
        }
        let count = CpuScene::required_bytes(width, height).ok_or(Error::Invalid)? / 4;
        if count > self.pixels.len() {
            self.pixels
                .try_reserve_exact(count - self.pixels.len())
                .map_err(|_| Error::OutOfMemory)?;
        }
        self.pixels.resize(count, 0);
        self.width = width;
        self.height = height;
        self.current = false;
        Ok(true)
    }
    pub fn pixels(&self) -> Option<(&[u32], i32, i32, u64)> {
        self.current
            .then_some((&self.pixels, self.width, self.height, self.generation))
    }
    pub fn canonical(&mut self) -> Option<CpuScene<'_>> {
        if !self.current {
            return None;
        }
        CpuScene::new(&mut self.pixels, self.width, self.height)
    }
    pub fn capture_shadow<T>(
        &mut self,
        pixels: &[u32],
        width: i32,
        height: i32,
        call: impl FnOnce(&mut CpuScene<'_>) -> T,
    ) -> Result<T> {
        if pixels.len() != CpuScene::required_bytes(width, height).ok_or(Error::Bounds)? / 4 {
            return Err(Error::Bounds);
        }
        self.resize(width, height)?;
        self.current = false;
        self.pixels.copy_from_slice(pixels);
        let mut canvas = CpuScene::new(&mut self.pixels, width, height).ok_or(Error::State)?;
        Ok(call(&mut canvas))
    }
    /// # Safety
    /// All hosted committed maps and exact external BO fronts remain resident
    /// and immutable through painting. No lifecycle pump runs in this scope.
    pub unsafe fn primary<M: Memory>(
        &mut self,
        app: &App<'_>,
        session: &mut Session<'_, '_>,
        graphics: &Graphics<'_, '_>,
        colors: &impl software::Colors,
        windows: &mut Set<M, Consumer>,
        scene: &mut Scene<'_>,
        regions: &[Rect],
        input_tick: u64,
    ) -> Result<Frame> {
        self.resize(scene.width, scene.height)?;
        if regions.is_empty() {
            return Err(Error::Invalid);
        }
        let start = crate::runtime::environment::precise(app).ok_or(Error::State)?;
        let mut canvas =
            CpuScene::new(&mut self.pixels, self.width, self.height).ok_or(Error::State)?;
        let mut fonts = SessionFonts::new(session, app);
        let mut loader = icons::Native { app };
        let mut icons = icons::Paint {
            cache: &mut self.icons,
            loader: &mut loader,
        };
        let old = scene.path;
        scene.path = Path::PrimaryCpu;
        let composed = unsafe {
            compositor::compose(
                &mut self.software,
                colors,
                &mut canvas,
                scene,
                windows.cpu_fronts(),
                regions,
                &mut self.fonts,
                &mut fonts,
                &mut icons,
                &mut self.shapes,
            )
        };
        scene.path = old;
        let completion = match composed {
            Ok(v) => v,
            Err(e) => {
                self.current = false;
                return Err(e);
            }
        };
        let composed = crate::runtime::environment::precise(app).ok_or(Error::State)?;
        self.stats.last_composition_ns = composed.saturating_sub(start);
        self.generation = self.generation.wrapping_add(1).max(1);
        let display = Display(graphics);
        let bounds = regions
            .iter()
            .copied()
            .reduce(Rect::merged)
            .ok_or(Error::Invalid)?;
        let _ = display.begin(bounds);
        let presentation = display.present(&canvas, regions, self.generation, input_tick);
        let completed_ns = crate::runtime::environment::precise(app).ok_or(Error::State)?;
        self.stats.last_present_ns = completed_ns.saturating_sub(composed);
        self.current = true;
        self.stats.redraws = self.stats.redraws.saturating_add(1);
        if regions.len() == 1 && regions[0] == Rect::new(0, 0, self.width, self.height) {
            self.stats.full = self.stats.full.saturating_add(1);
        } else {
            self.stats.mixed = self.stats.mixed.saturating_add(1);
        }
        let area = regions
            .iter()
            .map(|r| r.w.max(0) as u64 * r.h.max(0) as u64)
            .sum::<u64>();
        self.stats.pixels = self.stats.pixels.saturating_add(area);
        if presentation.presented {
            self.stats.presents = self.stats.presents.saturating_add(1);
        } else {
            self.stats.present_errors = self.stats.present_errors.saturating_add(1);
        }
        // Original Desktop diagnostic: count discontinuous nonzero CPU
        // presentation generations. This is neither elapsed time nor proof
        // that an output was physically visible.
        let generation = presentation.result.present_generation;
        if generation != 0 {
            if self.stats.present_generation_last != 0
                && generation != self.stats.present_generation_last.wrapping_add(1)
            {
                self.stats.present_lost_frames = self.stats.present_lost_frames.wrapping_add(1);
            }
            self.stats.present_generation_last = generation;
        }
        if presentation.copied {
            self.stats.copied_bytes = self.stats.copied_bytes.saturating_add(area * 4);
        }
        Ok(Frame {
            completion,
            presentation,
            generation: self.generation,
            started_ns: start,
            completed_ns,
        })
    }
    /// # Safety
    /// Same immutable frame/BO loans as `primary`; native tasks own all copied
    /// command/cache state until their actual join and consumer retirement.
    pub unsafe fn outputs<'a, M: Memory>(
        &mut self,
        app: &App<'_>,
        session: &mut Session<'_, '_>,
        graphics: &Graphics<'_, '_>,
        resources: &'a Resources<'a>,
        expected: &'static StartupInfo<'static>,
        manager: &mut Manager<'a>,
        windows: &mut Set<M, Consumer>,
        colors: &impl software::Colors,
        scene: &mut Scene<'_>,
        regions: &[Rect],
        time: u64,
        input_ns: u64,
    ) -> bool {
        manager.invalidate(regions);
        let mut changed = false;
        let mut failures = 0u32;
        for (index, slot) in manager.slots.iter_mut().enumerate() {
            if !slot.dirty() {
                continue;
            }
            let Some(bounds) = slot.bounds() else {
                continue;
            };
            let Some(damage) = slot.damage.bounds() else {
                continue;
            };
            let ready = if let Some(output) = slot.resource.gpu_mut() {
                output.available(app, manager.revision, time, input_ns)
            } else {
                slot.resource
                    .software()
                    .is_some_and(|output| output.ready())
            };
            if !ready {
                continue;
            }
            let mut fonts = SessionFonts::new(session, app);
            let mut loader = icons::Native { app };
            let mut icons = icons::Paint {
                cache: &mut self.icons,
                loader: &mut loader,
            };
            let old = scene.path;
            let disabled = scene.disabled;
            scene.disabled = slot.disabled;
            let captured = if let Some(output) = slot.resource.gpu_mut() {
                scene.path = Path::Native;
                let mut damage_regions = [Rect::ZERO; MAX_DAMAGE_REGIONS];
                let count = slot.damage.take_regions(&mut damage_regions);
                let result = unsafe {
                    output.capture(
                        windows,
                        Mode::NativeOutput(slot.view),
                        scene,
                        &damage_regions[..count],
                        &mut self.fonts,
                        &mut fonts,
                        &mut icons,
                        &mut self.shapes,
                    )
                };
                if result.is_ok() {
                    unsafe { output.start(app, resources, expected, time) }
                } else {
                    output.reject(app);
                    false
                }
            } else if let Some(output) = slot.resource.software_mut() {
                scene.path = Path::CpuOutput;
                unsafe {
                    output.render(
                        app,
                        graphics,
                        colors,
                        scene,
                        windows.cpu_fronts(),
                        damage,
                        input_ns,
                        time,
                        time.saturating_add(2_000_000_000),
                        &mut self.fonts,
                        &mut fonts,
                        &mut icons,
                        &mut self.shapes,
                    )
                }
                .is_ok()
            } else {
                false
            };
            scene.path = old;
            scene.disabled = disabled;
            if captured {
                slot.damage = Dirty::default();
                changed = true;
                self.current = false;
            } else {
                slot.damage.invalidate(damage);
                failures |= 1 << index;
            }
            let _ = bounds;
        }
        for index in 0..manager.slots.len() {
            if failures & (1 << index) != 0 {
                manager.fail(index, time);
            }
        }
        changed
    }
    /// # Safety
    /// Same native frame/map and join contract as `outputs`, for the existing
    /// single-display route without a connector viewport transformation.
    pub unsafe fn native_primary<'a, M: Memory>(
        &mut self,
        app: &App<'_>,
        session: &mut Session<'_, '_>,
        resources: &'a Resources<'a>,
        expected: &'static StartupInfo<'static>,
        output: &mut crate::outputs::native::Output<'a>,
        windows: &mut Set<M, Consumer>,
        scene: &mut Scene<'_>,
        regions: &[Rect],
        time: u64,
    ) -> bool {
        if regions.is_empty() {
            return false;
        }
        let mut fonts = SessionFonts::new(session, app);
        let mut loader = icons::Native { app };
        let mut icons = icons::Paint {
            cache: &mut self.icons,
            loader: &mut loader,
        };
        let previous = scene.path;
        scene.path = Path::Native;
        let result = unsafe {
            output.capture(
                windows,
                Mode::Desktop(Rect::new(0, 0, scene.width, scene.height)),
                scene,
                regions,
                &mut self.fonts,
                &mut fonts,
                &mut icons,
                &mut self.shapes,
            )
        };
        scene.path = previous;
        let captured = if result.is_ok() {
            unsafe { output.start(app, resources, expected, time) }
        } else {
            output.reject(app);
            false
        };
        if captured {
            self.current = false
        }
        captured
    }
}
