//! Same original output-manager Source; real shared R4GFX snapshot algorithm.
use super::*;
use r4os_sdk::{App, ImportSpec, StartupInfo};
unsafe extern "C" {
    fn r4desk_catalog_context() -> *const a::R4XStartContext;
    fn r4desk_catalog_control(op: u32, value: u64) -> i32;
}
fn control(op: u32, value: u64) {
    assert_eq!(unsafe { r4desk_catalog_control(op, value) }, 0);
}
#[test]
fn original_complete_catalog_generation_encoding_and_failed_discovery() {
    let raw = unsafe { r4desk_catalog_context() };
    let imports =
        unsafe { core::slice::from_raw_parts((*raw).imports as *const a::R4XStartImport, 2) };
    let sys = unsafe { &*(imports[0].table as *const a::R4XStartR4Sys) };
    let draw = unsafe { &*(imports[1].table as *const a::R4XStartR4Draw) };
    let specs = [
        ImportSpec {
            module_name: b"R4SYS",
            symbol_name: b"Query",
            minimum: 1,
            optional: false,
            group_id: 1,
            magic: sys.magic,
            version: sys.abi_version,
            size: sys.size,
        },
        ImportSpec {
            module_name: b"R4DRAW",
            symbol_name: b"Query",
            minimum: 1,
            optional: false,
            group_id: 3,
            magic: draw.magic,
            version: draw.abi_version,
            size: draw.size,
        },
    ];
    let expected = StartupInfo {
        app_class: a::R4XStartAppClass::console,
        required_groups: 10,
        imports: &specs,
    };
    let app = unsafe { App::from_raw(raw, &expected, c"CATFIX") }.unwrap();
    let context = Context::new(&app).unwrap();
    let initial = Snapshot::read(&context).unwrap();
    assert_eq!((initial.revision(), initial.entries().len()), (1, 1));
    assert!(!initial.entries()[0].active());
    assert_eq!(
        initial.entries()[0].key,
        Key {
            adapter: 1,
            connector: 2,
            receiver: [0; 16]
        }
    );
    control(2, 1);
    assert!(matches!(
        Snapshot::read(&context),
        Err(Error::Native("Stale"))
    ));
    assert_eq!(initial.entries()[0].info.identity.connection_generation, 4);
    control(2, 0);
    control(0, 2);
    control(1, 0);
    let empty = Snapshot::read(&context).unwrap();
    assert!(empty.entries().is_empty());
    assert_eq!(empty.revision(), 2);
    assert!(!initial.same_outputs(&empty).unwrap());
    control(1, 1);
    control(3, 1);
    control(0, 10);
    let before = Snapshot::read(&context).unwrap();
    let entry = before.entries()[0];
    assert!(entry.active());
    assert_eq!(entry.presentation.width, 1920);
    assert_eq!(
        (
            entry.target.device_generation,
            entry.target.connection_generation,
            entry.target.display_generation
        ),
        (3, 4, 5)
    );
    assert_eq!(entry.color.unwrap().bpc, 8);
    assert_eq!(entry.color.unwrap().revision, 10);
    for scenario in 0..5 {
        control(0, 11);
        control(4, if scenario == 1 { 10 } else { 8 });
        control(5, if scenario == 2 { 3 } else { 2 });
        control(6, if scenario == 3 { 5 } else { 4 });
        let next = Snapshot::read(&context).unwrap();
        assert!(!before.same_outputs(&next).unwrap());
        assert_eq!(next.entries()[0].color.unwrap().revision, 11);
        assert_eq!(
            next.entries()[0].color.unwrap().bpc,
            if scenario == 1 { 10 } else { 8 }
        );
        assert_eq!(
            next.entries()[0].presentation.buffer_count,
            if scenario == 2 { 3 } else { 2 }
        );
        assert_eq!(
            next.entries()[0].target.connection_generation,
            if scenario == 3 { 5 } else { 4 }
        );
        assert!(
            next.same_outputs(&Snapshot::read(&context).unwrap())
                .unwrap()
        );
    }
    // The original output-manager normalization oracle: an unrepresentable
    // span must leave every coordinate and primary flag unchanged. A single
    // INT_MIN origin is still admissible after translation to the primary.
    use crate::{
        outputs::topology_wire,
        render::output_geometry::topology::{Output, Point, Viewport},
    };
    let mut values = [
        Output {
            key: Key {
                adapter: 1,
                connector: 1,
                receiver: [0; 16],
            },
            primary: true,
            view: Viewport {
                origin: Point { x: i32::MIN, y: 0 },
                pixel_w: 640,
                pixel_h: 480,
                ..Default::default()
            },
            ..Default::default()
        },
        Output {
            key: Key {
                adapter: 1,
                connector: 2,
                receiver: [0; 16],
            },
            view: Viewport {
                origin: Point {
                    x: i32::MAX - 640,
                    y: 0,
                },
                pixel_w: 640,
                pixel_h: 480,
                ..Default::default()
            },
            ..Default::default()
        },
    ];
    let before = values;
    assert!(topology_wire::normalize(&mut values, 0, 1).is_err());
    assert_eq!(values, before);
    let one = topology_wire::normalize(&mut values[..1], 0, 2).unwrap();
    assert_eq!(one.translation, Point { x: i32::MIN, y: 0 });
    assert_eq!(values[0].view.origin, Point::default());
    assert_eq!(one.layout.outputs().len(), 1);
    // Exercise the actual return protocol of new native clients: successful
    // void setters/requests intentionally carry no optional PRESENT payload.
    use crate::outputs::{color_policy, intents::Intents, modes, preferences::Preferences};
    let preferences = Preferences::new().unwrap();
    let refresh = preferences.refresh_choice(entry.key).unwrap();
    let mut intents = Intents::new().unwrap();
    intents.frame(1).unwrap();
    intents.frame(2).unwrap();
    intents.frame(3).unwrap();
    assert!(intents.animated(3).unwrap());
    intents
        .step(
            &context,
            entry.target,
            entry.info.identity,
            &refresh,
            None,
            3,
            0,
            false,
            false,
        )
        .unwrap();
    intents.release(&context, entry.target).unwrap();
    let mut modes = modes::Modes::new().unwrap();
    assert!(!modes.busy().unwrap());
    assert!(modes.reset(&context).unwrap());
    assert!(modes.close(&context).unwrap());
    let key = Key {
        receiver: [1; 16],
        ..entry.key
    };
    let mut choice = color_policy::choice(key).unwrap();
    let signal = color_policy::sdr().unwrap();
    color_policy::set_signal(&mut choice, signal).unwrap();
    let mut colors = color_policy::Restoration::new().unwrap();
    colors.record(&choice, entry.info.identity).unwrap();
    assert!(colors.seen(&choice, entry.info.identity).unwrap());
    let manager = crate::outputs::manager::Manager::new().unwrap();
    assert_eq!(manager.slots.len(), 16);
    assert!(!manager.active());
}
