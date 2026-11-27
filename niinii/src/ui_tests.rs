//! UI behavior, driven headlessly by Dear ImGui Test Engine. The engine is
//! process-global, so every scenario runs inside the one test below.

use std::{convert::Infallible, num::NonZeroU64};

use dear_imgui_reflect::{ImGuiReflect, ImGuiReflectExt, ReflectSession};
use dear_imgui_rs::{Context, FramePrepareOptions, Key, KeyChord, Ui};
use dear_imgui_test_engine::{
    RunOutcome, RunSpeed, RunnerControl, ScriptCount, ScriptTest, TestEngine, TestEngineResult,
    TestRunner,
};

use crate::{
    settings::Settings,
    view::{panel::Panel, settings as settings_view, shortcuts::Action},
};

/// Exercises the vendored reflect patches: a column layout, inline optional
/// values with their own attributes, and a field gated on another.
#[derive(Default, ImGuiReflect)]
#[imgui(columns = 2, open)]
struct Sample {
    #[imgui(name = "Limit", enable_with = 7, help = "A limit")]
    limit: Option<u32>,
    #[imgui(name = "Flag")]
    flag: bool,
    #[imgui(name = "Ratio", slider, min = 0.0, max = 1.0, enable_with = 0.5)]
    ratio: Option<f32>,
    #[imgui(name = "Gated", enabled_if = self.flag)]
    gated: bool,
}

/// The parts of the app under test, drawn every frame.
struct Harness {
    sample: Sample,
    settings: Settings,
    settings_panel: Panel,
    reflect: ReflectSession,
    field: String,
    pressed: Vec<Action>,
}

impl Harness {
    fn new() -> Self {
        Self {
            sample: Sample::default(),
            settings: Settings::default(),
            settings_panel: Panel::new("Settings"),
            reflect: settings_view::reflect_session(),
            field: String::new(),
            pressed: Vec::new(),
        }
    }

    fn ui(&mut self, ui: &Ui) {
        ui.window("Harness").build(|| {
            self.settings_panel.menu_item(ui);
            ui.input_text("Field", &mut self.field).build();
            self.pressed.extend(Action::pressed(ui));
        });
        ui.window("Sample").build(|| {
            ui.inspector(&self.reflect)
                .input("Sample", &mut self.sample);
        });
        settings_view::show(
            &mut self.settings_panel,
            ui,
            &self.reflect,
            &mut self.settings,
        );
    }
}

fn key(key: Key) -> KeyChord {
    KeyChord::new(key)
}

fn once() -> TestEngineResult<ScriptCount> {
    ScriptCount::new(1)
}

/// Scenarios run in registration order against one harness; the checks on
/// the harness state come after the run.
fn register(engine: &mut TestEngine) -> TestEngineResult<()> {
    engine.add_script_test(
        "shortcuts",
        "plain key runs its action",
        |t: &mut ScriptTest| {
            t.set_ref("Harness")?;
            t.key_press(key(Key::T), once()?)
        },
    )?;
    engine.add_script_test("shortcuts", "typing does not run actions", |t| {
        t.set_ref("Harness")?;
        t.item_click("Field")?;
        t.key_press(key(Key::T), once()?)?;
        t.key_press(key(Key::Escape), once()?)
    })?;
    engine.add_script_test("settings", "reflected editors write settings", |t| {
        t.set_ref("Harness")?;
        t.item_click("Settings")?;
        t.set_ref("Settings")?;
        // Sections start expanded.
        t.item_set_checked("**/Stroke text", false)?;
        t.item_set_checked("**/Auto-translate", true)
    })?;
    engine.add_script_test("reflect", "inline option enables in columns", |t| {
        t.set_ref("Sample")?;
        t.item_set_checked("**/##Limit##enabled", true)?;
        t.item_set_checked("**/##Ratio##enabled", true)?;
        t.item_set_checked("**/Flag", true)?;
        t.item_set_checked("**/Gated", true)
    })?;
    Ok(())
}

#[test]
fn ui() {
    let mut context = Context::create();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .expect("headless tests use the legacy font atlas")
        .build();

    let mut engine = TestEngine::create().expect("create test engine");
    engine.start(&mut context).expect("start test engine");
    engine.set_run_speed(RunSpeed::Fast).expect("set run speed");
    register(&mut engine).expect("register tests");
    context.prepare_frame(FramePrepareOptions::new([1280.0, 720.0], 1.0 / 60.0));

    let mut harness = Harness::new();
    let report = TestRunner::new(&mut engine)
        .frame_budget(NonZeroU64::new(2000).unwrap())
        .run_headless(&mut context, |ui, _frame| {
            harness.ui(ui);
            Ok::<_, Infallible>(RunnerControl::Continue)
        })
        .expect("run tests");
    let results: Vec<String> = report
        .tests()
        .iter()
        .map(|t| format!("{}/{}: {:?}", t.category(), t.name(), t.status()))
        .collect();
    assert_eq!(report.outcome(), RunOutcome::Passed, "{results:#?}");
    engine.shutdown().expect("shut down test engine");

    // Only the first scenario's key press: the second went to the text field.
    assert_eq!(harness.pressed, vec![Action::Translate]);
    assert!(!harness.settings.interface.stroke_text);
    assert!(harness.settings.translation.auto_translate);
    assert_eq!(harness.sample.limit, Some(7));
    assert_eq!(harness.sample.ratio, Some(0.5));
    assert!(harness.sample.flag);
    assert!(harness.sample.gated);
}
