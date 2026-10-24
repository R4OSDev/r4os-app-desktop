//! Current primary CPU client coverage. No opacity is inferred from an old
//! retained layer, a native/per-output capture, or an external window BO.
use super::{geometry::Rect, scene::intersect};
use crate::windows::window::{Kind, Window};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    PrimaryCpu,
    CpuOutput,
    Native,
    Recorded,
}
#[derive(Default)]
pub struct Stats {
    pub layers_visited: u32,
    pub layers_culled: u32,
}
#[derive(Clone, Copy, Default)]
pub struct Cover {
    client: Option<Rect>,
}
impl Cover {
    pub fn new(
        path: Path,
        window: Option<&Window>,
        classic_frame_valid: bool,
        external: bool,
        paint_clip: Rect,
        damage: Rect,
    ) -> Self {
        if path != Path::PrimaryCpu || external || paint_clip != damage || !classic_frame_valid {
            return Self::default();
        }
        let Some(window) = window else {
            return Self::default();
        };
        if window.kind() != Kind::App
            || window.instance_id() == 0
            || !window.visible()
            || window.minimized()
            || window.close_requested().is_some()
        {
            return Self::default();
        }
        let client = window.client_surface().rect;
        Self {
            client: (!client.is_empty()).then_some(client),
        }
    }
    pub fn client(self) -> Option<Rect> {
        self.client
    }
    pub fn visible(self, stats: &mut Stats, damage: Rect, bounds: Rect) -> bool {
        stats.layers_visited = stats.layers_visited.wrapping_add(1);
        let Some(area) = intersect(damage, bounds) else {
            stats.layers_culled = stats.layers_culled.wrapping_add(1);
            return false;
        };
        if self.client.is_some_and(|cover| {
            cover.x <= area.x
                && cover.y <= area.y
                && cover.right() >= area.right()
                && cover.bottom() >= area.bottom()
        }) {
            stats.layers_culled = stats.layers_culled.wrapping_add(1);
            return false;
        }
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_cpu_cover_requires_current_classic_capture_and_complete_affected_bounds() {
        let mut window = Window::new(Kind::App, Rect::new(2, 2, 140, 100));
        window.bind_instance(1);
        let client = window.client_surface().rect;
        let screen = Rect::new(0, 0, 160, 120);
        let cover = Cover::new(Path::PrimaryCpu, Some(&window), true, false, client, client);
        assert_eq!(cover.client(), Some(client));
        let mut stats = Stats::default();
        assert!(!cover.visible(&mut stats, client, screen));
        assert!(cover.visible(
            &mut stats,
            Rect::new(client.x - 1, client.y, client.w + 1, client.h),
            screen
        ));
        for path in [Path::CpuOutput, Path::Native, Path::Recorded] {
            assert_eq!(
                Cover::new(path, Some(&window), true, false, client, client).client(),
                None
            );
        }
        assert_eq!(
            Cover::new(Path::PrimaryCpu, Some(&window), true, true, client, client).client(),
            None
        );
        assert_eq!(
            Cover::new(
                Path::PrimaryCpu,
                Some(&window),
                false,
                false,
                client,
                client
            )
            .client(),
            None
        );
        assert_eq!(
            Cover::new(
                Path::PrimaryCpu,
                Some(&window),
                true,
                false,
                client.inset(1, 1),
                client
            )
            .client(),
            None
        );
        window.minimize();
        assert_eq!(
            Cover::new(Path::PrimaryCpu, Some(&window), true, false, client, client).client(),
            None
        );
        window.restore();
        window.close();
        assert_eq!(
            Cover::new(Path::PrimaryCpu, Some(&window), true, false, client, client).client(),
            None
        );
        window.restore();
        window.request_close(1);
        assert_eq!(
            Cover::new(Path::PrimaryCpu, Some(&window), true, false, client, client).client(),
            None
        );
    }
}
