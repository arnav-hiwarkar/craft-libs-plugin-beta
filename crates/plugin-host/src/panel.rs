use std::time::Duration;

use craft_plugin::{
    AppEvent, ListEvent, MIN_TIMER_MS, MenuItem, PANEL_MENU, PanelReply, Pointer, PointerPhase, Topic, UiEvent, UiEventKind, UiInput, View, Widget,
};

const LEGACY_TIMER: &str = "";
pub const MAX_BACKLOG: usize = 64;
pub const MAX_VIEWS: usize = 64;

#[derive(Debug, Clone, PartialEq)]
struct Armed {
    name: String,
    interval: Duration,
    repeat: bool,
    due: Duration,
}

#[derive(Debug, Default)]
pub struct Retained {
    widgets: Vec<Widget>,
    error: Option<String>,
    events: Vec<UiEvent>,
    gestures: Vec<u64>,
    gesture: u64,
    started: u64,
    keys: String,
    app: Vec<AppEvent>,
    topics: Vec<Topic>,
    timers: Vec<Armed>,
    views: Vec<View>,
    pressed: Vec<String>,
    menu: Vec<MenuItem>,
    menus: Vec<u64>,
    level: u32,
    size: [f32; 2],
    drawn: bool,
    stale: bool,
    hidden: bool,
    in_flight: bool,
    legacy: bool,
    calls: u64,
}

fn merged(pending: &UiEventKind, next: &UiEventKind) -> Option<UiEventKind> {
    match (pending, next) {
        (UiEventKind::Pointer(before), UiEventKind::Pointer(after))
            if before.phase == after.phase
                && matches!(after.phase, PointerPhase::Hover | PointerPhase::Drag)
                && before.modifiers == after.modifiers =>
        {
            let scroll = [before.scroll[0] + after.scroll[0], before.scroll[1] + after.scroll[1]];
            Some(UiEventKind::Pointer(Pointer { scroll, ..*after }))
        }
        (UiEventKind::Number(_), UiEventKind::Number(_))
        | (UiEventKind::Text(_), UiEventKind::Text(_))
        | (UiEventKind::Color(_), UiEventKind::Color(_)) => Some(next.clone()),
        _ => None,
    }
}

fn motion(kind: &UiEventKind) -> bool {
    matches!(kind, UiEventKind::Pointer(pointer) if matches!(pointer.phase, PointerPhase::Hover | PointerPhase::Drag))
}

fn discrete(kind: &UiEventKind) -> bool {
    !motion(kind) && !matches!(kind, UiEventKind::Number(_) | UiEventKind::Text(_) | UiEventKind::Color(_))
}

pub(crate) fn walk(widgets: &[Widget], visit: &mut impl FnMut(&Widget)) {
    for widget in widgets {
        visit(widget);
        match widget {
            Widget::Row { children }
            | Widget::Column { children }
            | Widget::Group { children, .. }
            | Widget::Scroll { children, .. }
            | Widget::Collapsing { children, .. }
            | Widget::Grid { children, .. }
            | Widget::DropTarget { children, .. }
            | Widget::DragSource { children, .. } => walk(children, visit),
            Widget::Split { first, second, .. } => {
                walk(first, visit);
                walk(second, visit);
            }
            _ => {}
        }
    }
}

fn menu_ids(widgets: &[Widget]) -> Vec<u64> {
    let mut menus = vec![PANEL_MENU];
    walk(widgets, &mut |widget| {
        if let Widget::Menu { id, .. } = widget {
            menus.push(*id);
        }
    });
    menus
}

fn picked(event: &UiEvent, menus: &[u64]) -> bool {
    matches!(event.kind, UiEventKind::Number(_)) && menus.contains(&event.id)
}

fn one_call(events: &[UiEvent], gestures: &[u64], menus: &[u64]) -> usize {
    let mut acted = Vec::new();
    let collapses = |event: &UiEvent| matches!(event.kind, UiEventKind::Click | UiEventKind::Bool(_) | UiEventKind::Drop(_)) || picked(event, menus);
    events
        .iter()
        .zip(gestures)
        .position(|(event, gesture)| {
            let again = (collapses(event) && acted.contains(&event.id)) || Some(gesture) != gestures.first();
            if collapses(event) {
                acted.push(event.id);
            }
            again
        })
        .unwrap_or(events.len())
}

impl Retained {
    pub fn new() -> Self {
        Retained::with_level(1)
    }

    pub fn with_level(level: u32) -> Self {
        Retained { level, ..Retained::default() }
    }

    pub fn menu(&self) -> &[MenuItem] {
        &self.menu
    }

    pub fn press(&mut self, chord: &str) {
        self.pressed.push(chord.to_string());
    }

    pub fn widgets(&self) -> &[Widget] {
        &self.widgets
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn calls(&self) -> u64 {
        self.calls
    }

    pub fn in_flight(&self) -> bool {
        self.in_flight
    }

    pub fn pending(&self) -> bool {
        !self.events.is_empty()
    }

    pub fn split(&mut self) {
        self.gesture += 1;
    }

    pub fn gesture(&self) -> u64 {
        self.gesture
    }

    pub fn started_gesture(&self) -> u64 {
        self.started
    }

    pub fn pending_in(&self, gesture: u64) -> bool {
        self.gestures.contains(&gesture)
    }

    pub fn subscribed(&self, topic: Topic) -> bool {
        self.topics.contains(&topic)
    }

    pub fn push(&mut self, events: Vec<UiEvent>, keys: &str) {
        for id in menu_ids(&self.widgets) {
            if !self.menus.contains(&id) {
                self.menus.push(id);
            }
        }
        let (in_flight, level, menus) = (self.in_flight, self.level, self.menus.clone());
        for event in events.into_iter().filter(|event| event.kind.ui_level() <= level) {
            if let UiEventKind::List(ListEvent::Show { first, count }) = event.kind {
                self.show(View { id: event.id, first, count });
                continue;
            }
            let gesture = self.gesture;
            let last = self
                .events
                .iter()
                .zip(&self.gestures)
                .rposition(|(pending, made)| pending.id == event.id || discrete(&pending.kind) || picked(pending, &menus) || *made != gesture)
                .filter(|at| self.events[*at].id == event.id && self.gestures[*at] == gesture && !picked(&event, &menus));
            match last.filter(|_| in_flight).and_then(|at| merged(&self.events[at].kind, &event.kind).map(|kind| (at, kind))) {
                Some((at, kind)) => self.events[at].kind = kind,
                None => {
                    self.events.push(event);
                    self.gestures.push(gesture);
                }
            }
        }
        while self.events.len() > MAX_BACKLOG
            && let Some(oldest) = self.events.iter().position(|event| motion(&event.kind))
        {
            self.events.remove(oldest);
            self.gestures.remove(oldest);
        }
        self.keys.push_str(keys);
    }

    fn show(&mut self, view: View) {
        self.views.retain(|shown| shown.id != view.id);
        if self.views.len() == MAX_VIEWS {
            self.views.remove(0);
        }
        self.views.push(view);
        self.stale = true;
    }

    pub fn notify(&mut self, event: &AppEvent) {
        if !self.subscribed(event.topic()) {
            return;
        }
        match (self.app.iter_mut().find(|pending| pending.topic() == event.topic()), event) {
            (Some(AppEvent::Edit { layers: pending }), AppEvent::Edit { layers }) => {
                pending.extend(layers);
                pending.sort_unstable();
                pending.dedup();
            }
            (Some(pending), _) => *pending = event.clone(),
            (None, _) => self.app.push(event.clone()),
        }
    }

    pub fn reset(&mut self) {
        let (topics, app, timers) = (std::mem::take(&mut self.topics), std::mem::take(&mut self.app), std::mem::take(&mut self.timers));
        *self = Retained { level: self.level, calls: self.calls, gesture: self.gesture + 1, topics, app, timers, ..Retained::default() };
    }

    pub fn invalidate(&mut self) {
        self.stale = true;
    }

    pub fn hide(&mut self) {
        self.hidden = true;
        self.events.clear();
        self.gestures.clear();
        self.menus.clear();
        self.keys.clear();
        self.pressed.clear();
    }

    pub fn next_due(&self) -> Option<Duration> {
        if self.hidden || self.in_flight {
            return None;
        }
        self.timers.iter().map(|timer| timer.due).min()
    }

    pub fn start(&mut self, size: [f32; 2], now: Duration) -> Option<UiInput> {
        if self.in_flight {
            return None;
        }
        if std::mem::take(&mut self.hidden) {
            self.timers.iter_mut().for_each(|timer| timer.due = now + timer.interval);
        }
        let fired: Vec<String> = self.timers.iter().filter(|timer| timer.due <= now).map(|timer| timer.name.clone()).collect();
        let triggered = !self.drawn
            || self.stale
            || size != self.size
            || !self.events.is_empty()
            || !self.keys.is_empty()
            || !self.app.is_empty()
            || !self.pressed.is_empty();
        if !triggered && fired.is_empty() {
            return None;
        }
        self.timers.retain_mut(|timer| {
            if timer.due > now {
                return true;
            }
            while timer.due <= now {
                timer.due += timer.interval;
            }
            timer.repeat
        });
        let end = one_call(&self.events, &self.gestures, &self.menus);
        self.started = self.gestures.first().copied().unwrap_or(self.gesture);
        self.gestures.drain(..end);
        let mut events: Vec<UiEvent> = self.events.drain(..end).collect();
        if self.events.is_empty() {
            self.menus.clear();
        }
        if self.legacy {
            events.retain(|event| event.kind.known_to_every_sdk());
        }
        let timers = fired.into_iter().filter(|name| name != LEGACY_TIMER).collect();
        self.calls += 1;
        let (keys, app, views) = (std::mem::take(&mut self.keys), std::mem::take(&mut self.app), self.views.clone());
        let pressed = std::mem::take(&mut self.pressed);
        let input = UiInput { events, size, keys, app, timers, views, instance: None, call: self.calls, pressed, closed: false };
        (self.size, self.stale, self.in_flight) = (size, false, true);
        Some(input)
    }

    pub fn abandon(&mut self) {
        (self.in_flight, self.stale) = (false, true);
    }

    pub fn finish(&mut self, reply: Result<PanelReply, String>, now: Duration) {
        if self.hidden {
            return self.abandon();
        }
        self.in_flight = false;
        self.drawn = true;
        let reply = match reply {
            Ok(reply) => reply,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        self.error = None;
        self.widgets = reply.widgets;
        self.menu = reply.menu;
        self.topics = reply.topics;
        self.legacy = reply.legacy;
        let mut wanted = reply.timers;
        if reply.legacy {
            wanted.push(craft_plugin::Timer { name: LEGACY_TIMER.into(), ms: MIN_TIMER_MS, repeat: true });
        }
        let before = std::mem::take(&mut self.timers);
        self.timers = wanted
            .into_iter()
            .map(|timer| {
                let interval = Duration::from_millis(u64::from(timer.ms.max(MIN_TIMER_MS)));
                let kept = before.iter().find(|armed| armed.name == timer.name && armed.interval == interval && armed.repeat == timer.repeat);
                Armed { due: kept.map_or(now + interval, |armed| armed.due), name: timer.name, interval, repeat: timer.repeat }
            })
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_plugin::{Key, Modifiers, TextStyle, Timer};

    const SIZE: [f32; 2] = [300.0, 200.0];

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn label(text: &str) -> Widget {
        Widget::Label { text: text.into(), style: TextStyle::Body }
    }

    fn reply(timers: Vec<Timer>, topics: Vec<Topic>) -> Result<PanelReply, String> {
        Ok(PanelReply { widgets: vec![label("drawn")], topics, timers, ..Default::default() })
    }

    fn drawn(timers: Vec<Timer>, topics: Vec<Topic>) -> Retained {
        let mut panel = Retained::new();
        panel.start(SIZE, ms(0));
        panel.finish(reply(timers, topics), ms(0));
        panel
    }

    fn click(id: u64) -> UiEvent {
        UiEvent { id, kind: UiEventKind::Click }
    }

    fn pointer(id: u64, x: f32, phase: PointerPhase) -> UiEvent {
        UiEvent { id, kind: UiEventKind::Pointer(Pointer { scroll: [0.0, 1.0], ..Pointer::new([x, 0.0], phase) }) }
    }

    fn slow_call(panel: &mut Retained) {
        panel.invalidate();
        assert!(panel.start(SIZE, ms(16)).is_some());
    }

    fn events(panel: &mut Retained, now: u64) -> Option<Vec<UiEvent>> {
        let input = panel.start(SIZE, ms(now))?;
        panel.finish(reply(vec![], vec![]), ms(now));
        Some(input.events)
    }

    #[test]
    fn a_drawn_panel_is_not_called_again_until_something_happens() {
        let mut panel = drawn(vec![], vec![]);
        assert_eq!((0..100).filter_map(|frame| panel.start(SIZE, ms(16 * frame))).count(), 0);
        panel.push(vec![click(7)], "");
        assert_eq!(panel.start(SIZE, ms(1700)).map(|input| input.events), Some(vec![click(7)]));
        assert_eq!(panel.start(SIZE, ms(1716)), None);
    }

    #[test]
    fn two_clicks_on_one_button_during_a_slow_call_reach_two_calls_in_order() {
        let mut panel = drawn(vec![], vec![]);
        slow_call(&mut panel);
        panel.push(vec![click(7), UiEvent { id: 9, kind: UiEventKind::Number(2.0) }], "");
        panel.push(vec![click(7), click(8)], "");
        panel.finish(reply(vec![], vec![]), ms(40));
        assert!(panel.pending());
        assert_eq!(events(&mut panel, 48), Some(vec![click(7), UiEvent { id: 9, kind: UiEventKind::Number(2.0) }]));
        assert_eq!(events(&mut panel, 64), Some(vec![click(7), click(8)]));
        assert_eq!(events(&mut panel, 80), None);
    }

    #[test]
    fn pointer_motion_and_values_made_during_a_call_coalesce_and_presses_stay_in_order() {
        let mut panel = drawn(vec![], vec![]);
        slow_call(&mut panel);
        for x in 0..1000 {
            panel.push(vec![pointer(3, x as f32, PointerPhase::Drag), UiEvent { id: 4, kind: UiEventKind::Number(f64::from(x)) }], "");
        }
        panel.push(vec![pointer(3, 5.0, PointerPhase::Up), pointer(3, 6.0, PointerPhase::Hover), pointer(3, 7.0, PointerPhase::Hover)], "");
        panel.finish(reply(vec![], vec![]), ms(40));
        let drag = UiEvent { id: 3, kind: UiEventKind::Pointer(Pointer { scroll: [0.0, 1000.0], ..Pointer::new([999.0, 0.0], PointerPhase::Drag) }) };
        let hover = UiEvent { id: 3, kind: UiEventKind::Pointer(Pointer { scroll: [0.0, 2.0], ..Pointer::new([7.0, 0.0], PointerPhase::Hover) }) };
        assert_eq!(
            events(&mut panel, 48),
            Some(vec![drag, UiEvent { id: 4, kind: UiEventKind::Number(999.0) }, pointer(3, 5.0, PointerPhase::Up), hover])
        );
    }

    #[test]
    fn a_frame_of_events_made_while_idle_reaches_the_panel_whole() {
        let mut panel = drawn(vec![], vec![]);
        let moves: Vec<UiEvent> = [20.0, 30.0, 40.0].map(|x| pointer(3, x, PointerPhase::Drag)).to_vec();
        panel.push(moves.clone(), "");
        assert_eq!(events(&mut panel, 16), Some(moves));
    }

    #[test]
    fn the_events_of_a_failed_call_are_consumed_and_never_delivered_again() {
        let mut panel = drawn(vec![], vec![]);
        panel.push(vec![click(7)], "");
        panel.start(SIZE, ms(16));
        panel.finish(Err("trapped".into()), ms(16));
        assert_eq!(panel.error(), Some("trapped"));
        assert_eq!(panel.widgets(), &[label("drawn")]);
        assert_eq!(panel.start(SIZE, ms(32)), None);
        panel.push(vec![click(8)], "");
        assert_eq!(events(&mut panel, 48), Some(vec![click(8)]));
    }

    #[test]
    fn a_reply_that_arrives_after_the_panel_was_hidden_is_discarded_and_the_panel_is_called_again_when_shown() {
        let mut panel = drawn(vec![], vec![]);
        panel.push(vec![click(7)], "");
        panel.start(SIZE, ms(16));
        panel.hide();
        panel.finish(Ok(PanelReply { widgets: vec![label("late")], ..Default::default() }), ms(20));
        assert_eq!(panel.widgets(), &[label("drawn")]);
        assert_eq!(panel.start(SIZE, ms(32)).map(|input| input.events), Some(vec![]));
    }

    #[test]
    fn a_click_made_before_the_panel_is_hidden_is_dropped_and_timers_wait_while_it_is_hidden() {
        let mut panel = drawn(vec![Timer { name: "tick".into(), ms: 100, repeat: true }], vec![]);
        panel.push(vec![click(7)], "x");
        panel.hide();
        assert_eq!(panel.next_due(), None);
        assert_eq!(panel.start(SIZE, ms(5000)), None);
        assert_eq!(panel.next_due(), Some(ms(5100)));
        assert_eq!(
            panel.start(SIZE, ms(5100)).map(|input| (input.events, input.keys, input.timers)),
            Some((vec![], String::new(), vec!["tick".into()]))
        );
    }

    #[test]
    fn app_events_reach_only_subscribed_panels_once_per_kind_with_the_latest_contents() {
        let mut panel = drawn(vec![], vec![Topic::Time]);
        panel.notify(&AppEvent::Comp { comp: Some(2) });
        assert_eq!(panel.start(SIZE, ms(16)), None);
        panel.notify(&AppEvent::Time { comp: 2, time: 1.0 });
        panel.notify(&AppEvent::Time { comp: 2, time: 1.5 });
        assert_eq!(panel.start(SIZE, ms(32)).map(|input| input.app), Some(vec![AppEvent::Time { comp: 2, time: 1.5 }]));
    }

    #[test]
    fn edits_made_while_a_panel_waits_union_their_layers() {
        let mut panel = drawn(vec![], vec![Topic::Edit]);
        panel.notify(&AppEvent::Edit { layers: vec![4, 1] });
        panel.notify(&AppEvent::Edit { layers: vec![2, 4] });
        assert_eq!(panel.start(SIZE, ms(16)).map(|input| input.app), Some(vec![AppEvent::Edit { layers: vec![1, 2, 4] }]));
    }

    #[test]
    fn a_repeating_timer_fires_ten_times_in_a_simulated_second_and_a_one_shot_once() {
        let timers = vec![Timer { name: "tick".into(), ms: 100, repeat: true }, Timer { name: "once".into(), ms: 250, repeat: false }];
        let mut panel = drawn(timers.clone(), vec![]);
        let mut fired: Vec<String> = Vec::new();
        for frame in 1..=60 {
            let now = Duration::from_secs_f64(f64::from(frame) / 60.0);
            if let Some(input) = panel.start(SIZE, now) {
                let once_done = fired.iter().any(|name| name == "once");
                fired.extend(input.timers.iter().cloned());
                let restated = timers.iter().filter(|timer| timer.repeat || !once_done).cloned().collect();
                panel.finish(reply(restated, vec![]), now);
            }
        }
        assert_eq!(fired.iter().filter(|name| *name == "tick").count(), 10);
        assert_eq!(fired.iter().filter(|name| *name == "once").count(), 1);
    }

    #[test]
    fn a_timer_below_the_floor_runs_at_sixteen_milliseconds() {
        let mut panel = drawn(vec![Timer { name: "fast".into(), ms: 1, repeat: true }], vec![]);
        assert_eq!(panel.next_due(), Some(ms(16)));
        assert_eq!(panel.start(SIZE, ms(15)), None);
        assert!(panel.start(SIZE, ms(16)).is_some());
    }

    #[test]
    fn a_panel_from_an_older_sdk_is_redrawn_every_frame_and_never_gets_event_kinds_it_cannot_read() {
        let mut panel = Retained::new();
        panel.start(SIZE, ms(0));
        panel.finish(Ok(PanelReply { widgets: vec![], legacy: true, ..Default::default() }), ms(0));
        let key = UiEvent { id: 3, kind: UiEventKind::Key(Key { name: "A".into(), modifiers: Modifiers::default() }) };
        panel.push(vec![key, click(3)], "");
        let input = panel.start(SIZE, ms(16)).unwrap_or_default();
        assert_eq!((input.events, input.timers), (vec![click(3)], vec![]));
        panel.finish(Ok(PanelReply { legacy: true, ..Default::default() }), ms(16));
        assert!(panel.start(SIZE, ms(32)).is_some());
    }

    #[test]
    fn a_value_made_after_another_click_on_the_same_button_stays_with_that_click() {
        let mut panel = drawn(vec![], vec![]);
        slow_call(&mut panel);
        let number = |value| UiEvent { id: 9, kind: UiEventKind::Number(value) };
        for events in [vec![click(7)], vec![number(2.0)], vec![click(7)], vec![number(3.0)]] {
            panel.push(events, "");
        }
        panel.finish(reply(vec![], vec![]), ms(40));
        assert_eq!(events(&mut panel, 48), Some(vec![click(7), number(2.0)]));
        assert_eq!(events(&mut panel, 64), Some(vec![click(7), number(3.0)]));
    }

    #[test]
    fn menu_picks_made_during_a_slow_call_are_kept_apart_and_reach_one_call_each_in_order() {
        let mut panel = Retained::new();
        panel.start(SIZE, ms(0));
        let menu = Widget::Column { children: vec![Widget::Menu { id: 5, text: Some("Actions".into()), items: Vec::new() }] };
        panel.finish(Ok(PanelReply { widgets: vec![menu], ..Default::default() }), ms(0));
        slow_call(&mut panel);
        let pick = |id, index| UiEvent { id, kind: UiEventKind::Number(index) };
        for events in [vec![pick(5, 1.0)], vec![pick(5, 3.0)], vec![pick(PANEL_MENU, 0.0)], vec![pick(PANEL_MENU, 2.0)]] {
            panel.push(events, "");
        }
        panel.finish(reply(vec![], vec![]), ms(40));
        assert_eq!(events(&mut panel, 48), Some(vec![pick(5, 1.0)]));
        assert_eq!(events(&mut panel, 64), Some(vec![pick(5, 3.0), pick(PANEL_MENU, 0.0)]));
        assert_eq!(events(&mut panel, 80), Some(vec![pick(PANEL_MENU, 2.0)]));
    }

    #[test]
    fn values_of_two_gestures_made_during_a_slow_call_stay_apart_and_each_call_says_its_gesture() {
        let mut panel = drawn(vec![], vec![]);
        slow_call(&mut panel);
        let number = |value| UiEvent { id: 9, kind: UiEventKind::Number(value) };
        let first = panel.gesture();
        panel.push(vec![number(1.0)], "");
        panel.push(vec![number(2.0)], "");
        panel.split();
        panel.push(vec![number(6.0)], "");
        panel.push(vec![number(7.0)], "");
        panel.finish(reply(vec![], vec![]), ms(40));
        assert_eq!((events(&mut panel, 48), panel.started_gesture()), (Some(vec![number(2.0)]), first));
        assert!(!panel.pending_in(first));
        assert_eq!((events(&mut panel, 64), panel.started_gesture()), (Some(vec![number(7.0)]), first + 1));
    }

    #[test]
    fn a_reset_panel_forgets_its_tree_and_input_but_keeps_its_app_events_and_is_called_again() {
        let mut panel = drawn(vec![], vec![Topic::Project]);
        slow_call(&mut panel);
        panel.push(vec![click(7)], "x");
        panel.press("Delete");
        panel.push(vec![UiEvent { id: 5, kind: UiEventKind::List(ListEvent::Show { first: 10, count: 5 }) }], "");
        panel.notify(&AppEvent::Project { path: None });
        panel.reset();
        assert_eq!((panel.widgets(), panel.in_flight(), panel.pending()), (&[][..], false, false));
        let input = panel.start(SIZE, ms(48)).unwrap_or_default();
        assert_eq!(
            (input.events, input.keys, input.pressed, input.views, input.app),
            (vec![], String::new(), Vec::<String>::new(), vec![], vec![AppEvent::Project { path: None }])
        );
    }

    #[test]
    fn a_backlog_past_its_bound_drops_its_oldest_pointer_motion_and_keeps_every_click() {
        let mut panel = drawn(vec![], vec![]);
        slow_call(&mut panel);
        for x in 0..50 {
            panel.push(vec![pointer(3, x as f32, PointerPhase::Drag), click(100 + x)], "");
        }
        panel.finish(reply(vec![], vec![]), ms(40));
        let kept: Vec<UiEvent> =
            (0..50).flat_map(|x| [(x >= 36).then(|| pointer(3, x as f32, PointerPhase::Drag)), Some(click(100 + x))]).flatten().collect();
        assert_eq!(events(&mut panel, 48), Some(kept));
    }

    #[test]
    fn a_list_view_change_calls_once_and_every_later_call_carries_the_view_and_its_number() {
        let mut panel = drawn(vec![], vec![]);
        let show = |first| UiEvent { id: 5, kind: UiEventKind::List(ListEvent::Show { first, count: 40 }) };
        panel.push(vec![show(100), show(300)], "");
        let first = panel.start(SIZE, ms(1)).map(|input| (input.events, input.views, input.call));
        assert_eq!(first, Some((vec![], vec![View { id: 5, first: 300, count: 40 }], 2)));
        panel.finish(reply(vec![], vec![]), ms(1));
        assert_eq!(panel.start(SIZE, ms(2)), None);
        panel.push(vec![click(1)], "");
        let later = panel.start(SIZE, ms(3)).map(|input| (input.views, input.call));
        assert_eq!(later, Some((vec![View { id: 5, first: 300, count: 40 }], 3)));
    }

    #[test]
    fn a_chord_pressed_on_a_panel_that_is_then_hidden_is_not_delivered_when_it_shows_again() {
        let mut panel = drawn(vec![], vec![]);
        panel.press("Delete");
        panel.hide();
        panel.invalidate();
        assert_eq!(panel.start(SIZE, ms(1)).map(|input| input.pressed), Some(vec![]));
    }

    #[test]
    fn events_newer_than_the_plugins_ui_level_never_reach_it_and_pressed_keys_call_once() {
        let at = Pointer::new([1.0, 1.0], PointerPhase::Down);
        let newer = || vec![UiEvent { id: 3, kind: UiEventKind::Context(at) }, click(4)];
        let mut old = drawn(vec![], vec![]);
        old.push(newer(), "");
        assert_eq!(old.start(SIZE, ms(1)).map(|input| input.events), Some(vec![click(4)]));
        let mut new = Retained::with_level(craft_plugin::UI_LEVEL);
        new.start(SIZE, ms(0));
        new.finish(reply(vec![], vec![]), ms(0));
        new.push(newer(), "");
        assert_eq!(new.start(SIZE, ms(1)).map(|input| input.events), Some(newer()));
        new.finish(reply(vec![], vec![]), ms(1));
        new.press("Delete");
        assert_eq!(new.start(SIZE, ms(2)).map(|input| input.pressed), Some(vec!["Delete".to_string()]));
    }
}
