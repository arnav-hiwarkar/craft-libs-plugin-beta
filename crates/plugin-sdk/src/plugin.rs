use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use serde::Serialize;

use crate::abi::Pixels;
use crate::effect::{Cx, Effect, Frame, Pass, Rows};
use crate::ui::{Panel, Ui};
use crate::viewer::{Overlay, Space, Tool, ToolSpec};
use crate::wire::{
    Call, CommandSpec, Context, Contributions, ControlsReply, EffectSpec, HostCall, Outcome, PanelPlace, PanelReply, PanelSize, PanelSpec, Pixel,
    TaskSpec, Window,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Error(pub String);

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Error(message)
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Error(message.into())
    }
}

impl From<crate::wire::FileError> for Error {
    fn from(error: crate::wire::FileError) -> Self {
        Error(error.message)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Error(error.to_string())
    }
}

type CommandFn = Box<dyn Fn() -> Result<()> + Send + Sync>;
type TaskFn = Box<dyn Fn(&Task) -> Result<serde_json::Value> + Send + Sync>;

pub struct Task {
    args: serde_json::Value,
}

impl Task {
    pub fn args(&self) -> &serde_json::Value {
        &self.args
    }

    pub fn progress(&self, fraction: f64, message: &str) -> Result<()> {
        let call = HostCall::Progress { fraction, message: message.into() };
        crate::host::call::<serde_json::Value>(&call, &[]).map(|_| ())
    }
}

type MakePanel = Box<dyn Fn() -> Box<dyn Panel> + Send + Sync>;

enum Panels {
    One(Mutex<Box<dyn Panel>>),
    Each(MakePanel, Mutex<HashMap<String, Box<dyn Panel>>>),
}

#[derive(Clone, Copy, Default, PartialEq, Debug)]
enum Last {
    #[default]
    Nothing,
    Effect,
    Panel,
    Command,
    Task,
    Tool,
}

fn misplaced(last: Last, modifier: &str, follows: &str) -> ! {
    panic!("`.{modifier}(..)` sets a {follows}, but it follows {last:?}; call it right after the `.{follows}(..)` it belongs to")
}

#[derive(Default)]
pub struct Plugin {
    effects: Vec<(EffectSpec, Box<dyn Effect>)>,
    panels: Vec<(PanelSpec, Panels)>,
    commands: Vec<(CommandSpec, CommandFn)>,
    tasks: Vec<(TaskSpec, TaskFn)>,
    tools: Vec<(ToolSpec, Mutex<Box<dyn Tool>>)>,
    last: Last,
}

impl Plugin {
    pub fn new() -> Self {
        Plugin::default()
    }

    pub fn effect(mut self, effect: impl Effect) -> Self {
        self.effects.push((effect.spec(), Box::new(effect)));
        self.last = Last::Effect;
        self
    }

    pub fn panel(mut self, id: &str, title: &str, panel: impl Panel) -> Self {
        let spec = PanelSpec { id: id.into(), title: title.into(), ..Default::default() };
        self.panels.push((spec, Panels::One(Mutex::new(Box::new(panel)))));
        self.last = Last::Panel;
        self
    }

    pub fn panel_each<P: Panel>(mut self, id: &str, title: &str, make: impl Fn() -> P + Send + Sync + 'static) -> Self {
        let spec = PanelSpec { id: id.into(), title: title.into(), multiple: true, ..Default::default() };
        let make: MakePanel = Box::new(move || Box::new(make()));
        self.panels.push((spec, Panels::Each(make, Mutex::new(HashMap::new()))));
        self.last = Last::Panel;
        self
    }

    pub fn command(mut self, id: &str, title: &str, run: impl Fn() -> Result<()> + Send + Sync + 'static) -> Self {
        self.commands.push((CommandSpec { id: id.into(), title: title.into(), ..Default::default() }, Box::new(run)));
        self.last = Last::Command;
        self
    }

    pub fn shortcut(mut self, keys: &str) -> Self {
        if let Some((spec, _)) = self.commands.last_mut() {
            spec.shortcut = Some(keys.into());
        }
        self
    }

    pub fn task(mut self, id: &str, title: &str, run: impl Fn(&Task) -> Result<serde_json::Value> + Send + Sync + 'static) -> Self {
        self.tasks.push((TaskSpec { id: id.into(), title: title.into() }, Box::new(run)));
        self.last = Last::Task;
        self
    }

    pub fn tool(mut self, id: &str, name: &str, tool: impl Tool) -> Self {
        let spec = ToolSpec { id: id.into(), name: name.into(), icon: tool.icon(), cursor: tool.cursor(), named_icon: None, options: None };
        self.tools.push((spec, Mutex::new(Box::new(tool))));
        self.last = Last::Tool;
        self
    }

    fn last_panel(&mut self, modifier: &str) -> &mut PanelSpec {
        let last = self.last;
        match (last, self.panels.last_mut()) {
            (Last::Panel, Some((spec, _))) => spec,
            _ => misplaced(last, modifier, "panel"),
        }
    }

    fn last_command(&mut self, modifier: &str) -> &mut CommandSpec {
        let last = self.last;
        match (last, self.commands.last_mut()) {
            (Last::Command, Some((spec, _))) => spec,
            _ => misplaced(last, modifier, "command"),
        }
    }

    pub fn icon(mut self, icon: &str) -> Self {
        let last = self.last;
        match (last, self.panels.last_mut(), self.commands.last_mut(), self.tools.last_mut()) {
            (Last::Panel, Some((spec, _)), _, _) => spec.icon = Some(icon.into()),
            (Last::Command, _, Some((spec, _)), _) => spec.icon = Some(icon.into()),
            (Last::Tool, _, _, Some((spec, _))) => spec.named_icon = Some(icon.into()),
            _ => misplaced(last, "icon", "panel, command or tool"),
        }
        self
    }

    pub fn place(mut self, place: PanelPlace) -> Self {
        self.last_panel("place").place = place;
        self
    }

    pub fn size(mut self, size: PanelSize) -> Self {
        self.last_panel("size").size = size;
        self
    }

    pub fn keys(mut self, chords: &[&str]) -> Self {
        self.last_panel("keys").keys = chords.iter().map(|chord| chord.to_string()).collect();
        self
    }

    pub fn help(mut self, url: &str) -> Self {
        self.last_panel("help").help = Some(url.into());
        self
    }

    pub fn menu(mut self, path: &str) -> Self {
        self.last_command("menu").menu = Some(path.into());
        self
    }

    pub fn context(mut self, context: Context) -> Self {
        let spec = self.last_command("context");
        if !spec.context.contains(&context) {
            spec.context.push(context);
        }
        self
    }

    pub fn toolbar(mut self) -> Self {
        self.last_command("toolbar").toolbar = true;
        self
    }

    pub fn options(mut self, panel: &str) -> Self {
        let last = self.last;
        match (last, self.tools.last_mut()) {
            (Last::Tool, Some((spec, _))) => spec.options = Some(panel.into()),
            _ => misplaced(last, "options", "tool"),
        }
        self
    }

    pub fn contributions(&self) -> Contributions {
        Contributions {
            effects: self.effects.iter().map(|(spec, _)| spec.clone()).collect(),
            panels: self.panels.iter().map(|(spec, _)| spec.clone()).collect(),
            commands: self.commands.iter().map(|(spec, _)| spec.clone()).collect(),
            tasks: self.tasks.iter().map(|(spec, _)| spec.clone()).collect(),
            tools: self.tools.iter().map(|(spec, _)| spec.clone()).collect(),
            separate_input: true,
            bounded: true,
            ui_level: crate::wire::UI_LEVEL,
        }
    }

    pub fn dispatch(&self, request: &[u8], pixels: &mut [Pixel]) -> Vec<u8> {
        self.serve(request, Pixels::packed(pixels))
    }

    pub(crate) fn serve(&self, request: &[u8], pixels: Pixels) -> Vec<u8> {
        let outcome: Outcome<serde_json::Value> =
            serde_json::from_slice::<Call>(request).map_err(Error::from).and_then(|call| self.run(call, pixels)).map_err(|e| e.0).into();
        serde_json::to_vec(&outcome).unwrap_or_default()
    }

    fn run(&self, call: Call, pixels: Pixels) -> Result<serde_json::Value> {
        match call {
            Call::Describe => json(&self.contributions()),
            Call::Layout { effect, cx } => json(&self.find_effect(&effect)?.layout(&Cx::new(cx))),
            Call::Render { effect, cx, frame } => {
                let (width, height) = (frame.width as usize, frame.height as usize);
                let origin = [frame.origin[0] as usize, frame.origin[1] as usize];
                let (read, window, out) = match frame.input {
                    None => {
                        let pixels = pixels.into_packed();
                        if pixels.len() != width.saturating_mul(height) {
                            return Err(format!("frame is {width}x{height} at {origin:?} but {} pixels were passed", pixels.len()).into());
                        }
                        let (effect, input) =
                            (self.find_effect(&effect)?, Rows { width, height: cx.size[1] as usize, first_row: origin[1], pixels: &[] });
                        let cx = Cx::new(cx).band(origin[1], height);
                        return crate::abi::in_place(pixels, |pixels| {
                            let mut frame = Frame::new(width, height, origin, pixels, input);
                            effect.render(&cx, &mut frame)?;
                            json(&frame.reply())
                        });
                    }
                    Some(window) if frame.spread => {
                        let (read, _, out) = pixels.spread()?;
                        (read, window, out)
                    }
                    Some(window) => {
                        let (read, out) =
                            pixels.into_packed().split_at_mut_checked(window.pixels()).ok_or("the input rows do not fit in the pixels passed")?;
                        (&*read, window, out)
                    }
                };
                let inside = origin[0].checked_add(width).is_some_and(|right| right <= window.size[0] as usize) && read.len() == window.pixels();
                if out.len() != width.saturating_mul(height) || !inside {
                    return Err(format!("frame is {width}x{height} at {origin:?} but {} pixels were passed", out.len()).into());
                }
                let input = rows(window, read);
                let mut frame = Frame::new(width, height, origin, out, input);
                self.find_effect(&effect)?.render(&Cx::new(cx).band(origin[1], height), &mut frame)?;
                json(&frame.reply())
            }
            Call::Input { effect, cx, out } => json(&self.find_effect(&effect)?.input(&Cx::new(cx), out)),
            Call::Pass { effect, cx, kernel, f, u, src, aux, out, spread } => {
                let (src_len, aux_len) = (src.pixels(), aux.map_or(0, |aux| aux.pixels()));
                let parts = if spread {
                    Some(pixels.spread()?)
                } else {
                    pixels.into_packed().split_at_mut_checked(src_len.saturating_add(aux_len)).and_then(|(read, band)| {
                        let (src, aux) = read.split_at_checked(src_len)?;
                        Some((src, aux, band))
                    })
                };
                let fits = |(src, aux, band): &(&[Pixel], &[Pixel], &mut [Pixel])| {
                    src.len() == src_len && aux.len() == aux_len && band.len() == out.pixels()
                };
                let (src_pixels, aux_pixels, band) = parts.filter(fits).ok_or("the pass images do not match the pixels passed")?;
                let pass = Pass {
                    kernel: &kernel,
                    f: &f,
                    u: &u,
                    src: rows(src, src_pixels),
                    aux: aux.map(|aux| rows(aux, aux_pixels)),
                    out_size: out.size.map(|side| side as usize),
                };
                let input = rows(Window { size: out.size, first_row: 0, rows: 0 }, &[]);
                let mut frame = Frame::new(out.size[0] as usize, out.rows as usize, [0, out.first_row as usize], band, input);
                let cx = Cx::new(cx).band(out.first_row as usize, out.rows as usize).on_grid(out.size);
                self.find_effect(&effect)?.pass(&cx, &pass, &mut frame)?;
                Ok(serde_json::Value::Null)
            }
            Call::Panel { panel, input } => {
                let (_, state) = self.panels.iter().find(|(spec, _)| spec.id == panel).ok_or_else(|| Error(format!("no panel `{panel}`")))?;
                let panicked = || Error("the panel panicked earlier; reload the plugin to start it again".into());
                let mut ui = Ui::new(&input);
                match state {
                    Panels::One(one) => one.lock().map_err(|_| panicked())?.ui(&mut ui),
                    Panels::Each(make, each) => {
                        let mut each = each.lock().map_err(|_| panicked())?;
                        each.entry(input.instance.clone().unwrap_or_default()).or_insert_with(make).ui(&mut ui);
                    }
                }
                json(&ui.reply())
            }
            Call::Controls { effect, param, instance, value, input } => {
                let mut edited = value.clone();
                let mut ui = Ui::new(&input);
                self.find_effect(&effect)?.controls(instance, &param, &mut edited, &mut ui);
                let PanelReply { widgets, topics, timers, .. } = ui.reply();
                json(&ControlsReply { widgets, value: (edited != value).then_some(edited), topics, timers })
            }
            Call::Command { command } => {
                let (_, run) = self.commands.iter().find(|(spec, _)| spec.id == command).ok_or_else(|| Error(format!("no command `{command}`")))?;
                run()?;
                Ok(serde_json::Value::Null)
            }
            Call::Task { task, args } => {
                let (_, run) = self.tasks.iter().find(|(spec, _)| spec.id == task).ok_or_else(|| Error(format!("no task `{task}`")))?;
                run(&Task { args })
            }
            Call::Overlay { effect, cx } => {
                let mut overlay = Overlay::new();
                self.find_effect(&effect)?.overlay(&Cx::new(cx), &mut overlay);
                json(&overlay)
            }
            Call::Drag { effect, cx, key, to, mut value } => {
                self.find_effect(&effect)?.drag(&Cx::new(cx), &key, to, &mut value);
                json(&Some(value))
            }
            Call::Button { effect, param, instance, layer } => {
                self.find_effect(&effect)?.button(&crate::effect::Press { param, instance, layer })?;
                Ok(serde_json::Value::Null)
            }
            Call::Migrate { effect, from, mut values } => {
                self.find_effect(&effect)?.migrate(from, &mut values);
                json(&values)
            }
            Call::Tool { tool, event } => {
                let (_, state) = self.tools.iter().find(|(spec, _)| spec.id == tool).ok_or_else(|| Error(format!("no tool `{tool}`")))?;
                let mut overlay = Overlay::in_space(Space::Comp);
                state
                    .lock()
                    .map_err(|_| Error("the tool panicked earlier; reload the plugin to start it again".into()))?
                    .event(&event, &mut overlay)?;
                json(&overlay)
            }
        }
    }

    fn find_effect(&self, id: &str) -> Result<&dyn Effect> {
        self.effects.iter().find(|(spec, _)| spec.id == id).map(|(_, effect)| effect.as_ref()).ok_or_else(|| {
            let known: Vec<&str> = self.effects.iter().map(|(spec, _)| spec.id.as_str()).collect();
            Error(format!("no effect `{id}`; this plugin has {known:?}"))
        })
    }
}

fn rows(window: Window, pixels: &[Pixel]) -> Rows<'_> {
    Rows { width: window.size[0] as usize, height: window.size[1] as usize, first_row: window.first_row as usize, pixels }
}

fn json(value: &impl Serialize) -> Result<serde_json::Value> {
    Ok(serde_json::to_value(value)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewer::ToolSpec;
    use crate::wire::{Param, ParamKind, Timer, Topic, UiEvent, UiEventKind, UiInput, Widget};

    struct Toggle;

    impl Effect for Toggle {
        fn spec(&self) -> EffectSpec {
            EffectSpec::new("toggle", "Toggle", "Test").param(Param::custom("state", "State", "off"))
        }

        fn controls(&self, _instance: u64, param: &str, value: &mut String, ui: &mut Ui) {
            ui.on(Topic::Selection);
            ui.every("blink", 250);
            if ui.button(param) {
                *value = if value == "off" { "on".into() } else { "off".into() };
            }
        }
    }

    fn controls(plugin: &Plugin, value: &str, events: Vec<UiEvent>) -> ControlsReply {
        let input = UiInput { events, ..Default::default() };
        let call = Call::Controls { effect: "toggle".into(), param: "state".into(), instance: 1, value: value.into(), input };
        let reply = plugin.dispatch(&serde_json::to_vec(&call).unwrap_or_default(), &mut []);
        match serde_json::from_slice::<Outcome<ControlsReply>>(&reply) {
            Ok(Outcome::Ok(reply)) => reply,
            other => panic!("{other:?}"),
        }
    }

    struct Copy;

    impl Effect for Copy {
        fn spec(&self) -> EffectSpec {
            EffectSpec::new("copy", "Copy", "Test")
        }

        fn pass(&self, _cx: &Cx, pass: &Pass, out: &mut Frame) -> Result<()> {
            out.pixels.copy_from_slice(pass.src.pixels);
            Ok(())
        }
    }

    #[test]
    fn safe_dispatch_refuses_frames_and_passes_given_by_address() {
        let plugin = Plugin::new().effect(Copy);
        let (source, mut written) = (vec![[0.5f32; 4]; 4], vec![[0.0f32; 4]; 4]);
        let address = |at: usize, len: usize| -> Pixel {
            let bytes: Vec<u8> = [at as u64, len as u64].iter().flat_map(|word| word.to_ne_bytes()).collect();
            std::array::from_fn(|i| f32::from_ne_bytes([bytes[4 * i], bytes[4 * i + 1], bytes[4 * i + 2], bytes[4 * i + 3]]))
        };
        let table = |out: &mut Vec<Pixel>| {
            vec![address(source.as_ptr() as usize, 4), address(source.as_ptr() as usize, 0), address(out.as_mut_ptr() as usize, 4)]
        };
        let window = Window { size: [2, 2], first_row: 0, rows: 2 };
        let render = Call::Render {
            effect: "copy".into(),
            cx: Default::default(),
            frame: crate::wire::FrameDesc { width: 2, height: 2, origin: [0, 0], input: Some(window), spread: true },
        };
        let pass = Call::Pass {
            effect: "copy".into(),
            cx: Default::default(),
            kernel: "k".into(),
            f: vec![],
            u: vec![],
            src: window,
            aux: None,
            out: window,
            spread: true,
        };
        for call in [render, pass] {
            let reply = plugin.dispatch(&serde_json::to_vec(&call).unwrap(), &mut table(&mut written));
            let refused: Outcome<serde_json::Value> = serde_json::from_slice(&reply).unwrap();
            assert!(matches!(&refused, Outcome::Err(why) if why.contains("by address")), "{refused:?}");
        }
        assert_eq!(written, vec![[0.0; 4]; 4]);
    }

    struct TurnLeft;

    impl Effect for TurnLeft {
        fn spec(&self) -> EffectSpec {
            EffectSpec::new("turn-left", "Turn Left", "Test")
        }

        fn render(&self, _cx: &Cx, frame: &mut Frame) -> Result<()> {
            let first = frame.at(0, frame.origin[1] as isize)[0];
            for (_, read, out) in frame.rows() {
                out.iter_mut().enumerate().for_each(|(x, pixel)| *pixel = [read[(x + 1) % read.len()][0], first, 0.0, 1.0]);
            }
            Ok(())
        }
    }

    #[test]
    fn a_frame_without_input_rows_changes_in_place_and_its_rows_read_the_input_as_it_was() {
        let plugin = Plugin::new().effect(TurnLeft);
        let mut pixels: Vec<Pixel> = (1..7).map(|v| [v as f32, 0.0, 0.0, 1.0]).collect();
        let frame = crate::wire::FrameDesc { width: 3, height: 2, origin: [0, 4], input: None, spread: false };
        let call = Call::Render { effect: "turn-left".into(), cx: crate::RenderCx { size: [3, 9], ..Default::default() }, frame };
        let at = pixels.as_ptr();
        let reply: Outcome<serde_json::Value> = serde_json::from_slice(&plugin.dispatch(&serde_json::to_vec(&call).unwrap(), &mut pixels)).unwrap();
        assert!(matches!(reply, Outcome::Ok(_)), "{reply:?}");
        assert_eq!(
            pixels.iter().map(|pixel| [pixel[0], pixel[1]]).collect::<Vec<_>>(),
            [[2.0, 1.0], [3.0, 1.0], [1.0, 1.0], [5.0, 1.0], [6.0, 1.0], [4.0, 1.0]]
        );
        assert_eq!(pixels.as_ptr(), at);
    }

    #[test]
    fn a_custom_param_declares_its_default_text_and_its_control_returns_the_new_value_only_when_it_changes() {
        let plugin = Plugin::new().effect(Toggle);
        assert_eq!(plugin.contributions().effects[0].params[0].kind, ParamKind::Custom { default: "off".into() });
        let idle = controls(&plugin, "off", vec![]);
        assert_eq!(idle.value, None);
        let Widget::Button { id, .. } = idle.widgets[0] else { panic!("button expected") };
        let clicked = controls(&plugin, "off", vec![UiEvent { id, kind: UiEventKind::Click }]);
        assert_eq!(clicked.value.as_deref(), Some("on"));
    }

    #[test]
    fn a_control_row_subscribes_and_arms_timers_as_a_panel_does_and_an_older_reply_has_neither() {
        let reply = controls(&Plugin::new().effect(Toggle), "off", vec![]);
        assert_eq!((reply.topics, reply.timers), (vec![Topic::Selection], vec![Timer { name: "blink".into(), ms: 250, repeat: true }]));
        let older: ControlsReply = serde_json::from_str(r#"{"widgets":[],"value":"on"}"#).unwrap();
        assert_eq!(older, ControlsReply { value: Some("on".into()), ..Default::default() });
    }

    #[derive(Default)]
    struct Count(u32);

    impl Panel for Count {
        fn ui(&mut self, ui: &mut Ui) {
            self.0 += 1;
            ui.label(&format!("{} {}", ui.instance().unwrap_or("-"), self.0));
        }
    }

    struct Pencil;

    impl Tool for Pencil {
        fn event(&mut self, _: &crate::ToolEvent, _: &mut Overlay) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn modifiers_set_the_contribution_they_follow() {
        let plugin = Plugin::new()
            .panel("inspector", "Inspector", Count::default())
            .icon("sliders")
            .place(PanelPlace::Float)
            .size(PanelSize::Large)
            .keys(&["Delete"])
            .help("https://example.org/inspector")
            .command("tidy", "Tidy Layers", || Ok(()))
            .menu("Layer/Arrange")
            .context(Context::Layers)
            .context(Context::Layers)
            .toolbar()
            .icon("wand")
            .shortcut("ctrl+t")
            .tool("pencil", "Pencil", Pencil)
            .icon("pen")
            .options("pencil-options");
        let found = plugin.contributions();
        assert_eq!(
            found.panels[0],
            PanelSpec {
                id: "inspector".into(),
                title: "Inspector".into(),
                icon: Some("sliders".into()),
                place: PanelPlace::Float,
                size: PanelSize::Large,
                multiple: false,
                keys: vec!["Delete".into()],
                help: Some("https://example.org/inspector".into())
            }
        );
        assert_eq!(
            found.commands[0],
            CommandSpec {
                id: "tidy".into(),
                title: "Tidy Layers".into(),
                shortcut: Some("ctrl+t".into()),
                menu: Some("Layer/Arrange".into()),
                context: vec![Context::Layers],
                toolbar: true,
                icon: Some("wand".into())
            }
        );
        assert_eq!((found.tools[0].named_icon.as_deref(), found.tools[0].options.as_deref()), (Some("pen"), Some("pencil-options")));
    }

    #[test]
    fn a_modifier_after_the_wrong_contribution_says_where_it_belongs() {
        let caught = std::panic::catch_unwind(|| Plugin::new().command("a", "A", || Ok(())).place(PanelPlace::Float));
        let message = caught.err().and_then(|e| e.downcast_ref::<String>().cloned()).unwrap_or_default();
        assert_eq!(message, "`.place(..)` sets a panel, but it follows Command; call it right after the `.panel(..)` it belongs to");
    }

    fn panel_text(plugin: &Plugin, instance: Option<&str>) -> String {
        let input = UiInput { instance: instance.map(str::to_string), ..Default::default() };
        let call = Call::Panel { panel: "view".into(), input };
        let reply = plugin.dispatch(&serde_json::to_vec(&call).unwrap_or_default(), &mut []);
        match serde_json::from_slice::<Outcome<PanelReply>>(&reply) {
            Ok(Outcome::Ok(PanelReply { widgets, .. })) => match &widgets[0] {
                Widget::Label { text, .. } => text.clone(),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn each_instance_of_a_panel_each_keeps_its_own_state() {
        let plugin = Plugin::new().panel_each("view", "View", Count::default);
        assert!(plugin.contributions().panels[0].multiple);
        let texts = [panel_text(&plugin, Some("a")), panel_text(&plugin, Some("a")), panel_text(&plugin, Some("b")), panel_text(&plugin, None)];
        assert_eq!(texts, ["a 1", "a 2", "b 1", "- 1"]);
    }

    #[test]
    fn specs_events_and_input_from_an_older_sdk_or_host_still_read() {
        let panel: PanelSpec = serde_json::from_str(r#"{"id":"p","title":"P"}"#).unwrap();
        assert_eq!(panel, PanelSpec { id: "p".into(), title: "P".into(), ..Default::default() });
        let command: CommandSpec = serde_json::from_str(r#"{"id":"c","title":"C","shortcut":null}"#).unwrap();
        assert_eq!(command, CommandSpec { id: "c".into(), title: "C".into(), ..Default::default() });
        let tool: ToolSpec = serde_json::from_str(r#"{"id":"t","name":"T"}"#).unwrap();
        assert_eq!((tool.named_icon, tool.options), (None, None));
        let event: crate::ToolEvent = serde_json::from_str(r#"{"kind":"down","comp":[1.0,2.0],"pressure":1.0}"#).unwrap();
        assert_eq!((event.pressure, event.pen), (1.0, None));
        let found: Contributions = serde_json::from_str(r#"{"panels":[]}"#).unwrap();
        assert_eq!((found.ui_level, Plugin::new().contributions().ui_level), (0, crate::wire::UI_LEVEL));
        let input: UiInput = serde_json::from_str(r#"{"size":[10.0,10.0]}"#).unwrap();
        assert_eq!((input.views, input.instance, input.call), (vec![], None, 0));
        let scroll: Widget = serde_json::from_str(r#"{"w":"scroll","id":1,"height":80.0,"children":[]}"#).unwrap();
        assert_eq!(scroll, Widget::Scroll { id: 1, height: 80.0, rows: None, children: vec![] });
    }
}
