use std::sync::atomic::{AtomicU32, Ordering};

use craft_plugin::prelude::*;

static SHARED: AtomicU32 = AtomicU32::new(0);

fn shared() -> String {
    format!("shared {}", SHARED.load(Ordering::Relaxed))
}

struct Peek;

impl Effect for Peek {
    fn spec(&self) -> EffectSpec {
        EffectSpec::new("peek", "Peek", "Shared")
    }

    fn overlay(&self, _cx: &Cx, overlay: &mut Overlay) {
        overlay.text([0.0, 0.0], &shared());
    }
}

struct Census;

impl Tool for Census {
    fn event(&mut self, _event: &ToolEvent, overlay: &mut Overlay) -> Result<()> {
        overlay.text([0.0, 0.0], &shared());
        Ok(())
    }
}

struct Bump;

impl Panel for Bump {
    fn ui(&mut self, ui: &mut Ui) {
        if ui.button("Bump") {
            SHARED.fetch_add(1, Ordering::Relaxed);
        }
        ui.label(&shared());
    }
}

fn plugin() -> Plugin {
    Plugin::new().effect(Peek).tool("census", "Census", Census).panel("bump", "Bump", Bump)
}

craft_plugin::export!(plugin);
