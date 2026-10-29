Explicit ordinary pointer-wheel fixture (0.88.30)
================================================
Gui is an image_scope=none public C app, never requests native pointer capture.
It renders one real window, logs typed ordinary mouse Down/Up, reports native
client geometry on S and exits on Q. Exit0 requires exactly2Down/2Up. Input is
an explicit public R4DESK remote_input_push consumer taking x:y:buttons:wheel.
Each accepted event contains the complete button/wheel snapshot. Positive1 is
success in this existing API. These are no installed apps or periodic traces.

Build both ordinary module.R4MF files with the SDK manifest builder, current
Contract and normal toolset. Inject only into a private KVM4 Test image. Launch
Gui through DESKRUN /JSON /WAITFRAME and keep exact completion/output with the
existing SDK MouseLook/Observe K/R helper. Use software display screenshots and
native Info output to bind the actual client coordinates; no physical input or
physical GPU/wheel/latency claim follows.

Observed1280x720 sequence: client628,140,434x212. Input708:210:1:1 then
713:215:0:-1 yields native Down80,70/buttons1 and Up85,75/buttons0. Caption
packets708:122:1:1,778:157:1:1,808:177:0:-1 include final release coordinate
without a separate motion. Follow with888:252:0:0 and S: client remains728,195,
434x212 (+100,+55), proving the old drag does not continue. A new ordinary
client click768:235:1:-2 then771:238:0:2 yields Down40,40 and Up43,43.
Q ends normally, retained original output516bytes, observation release0.
Coordinates are fixture observations, not a fixed window-placement contract.

The native pointer_router.rs fixture calls the productive ordinary router and
SDK host session for both snapshot sources: volume Down/Motion/Up commit,
actual window final release, icon layout drag, wheel+both tray releases and
GUI Down/Up through the existing native SDK callback. The original volume Up+
Wheel case first fails against the unchanged router. Native/model and actual
public-input proofs are separate. The new captured/FPS route from0.88.28 is
independent and this ordinary C fixture never acquires it.
