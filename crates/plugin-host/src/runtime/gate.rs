use craft_plugin::{EffectSpec, HostCall, Permission, Place, Root};

use super::serve::Identity;
use crate::host::{Access, Host, Origin};

fn access(call: &HostCall, host: &dyn Host, plugin: &Identity, origin: Origin) -> Access {
    match call {
        HostCall::Start { .. } | HostCall::Task { .. } | HostCall::Cancel { .. } | HostCall::Progress { .. } => Access::Free,
        HostCall::Frame { .. } if origin == Origin::Render => Access::Refused(
            "an effect cannot render a frame while it renders; read another layer through a layer parameter (cx.layer) instead".into(),
        ),
        HostCall::Audio { .. } if origin == Origin::Render => {
            Access::Refused("an effect cannot read audio while it renders; read it in a task and pass what the effect needs as a parameter".into())
        }
        HostCall::Frame { .. } | HostCall::Audio { .. } => Access::Needs(vec![Permission::DocumentRead]),
        HostCall::DataSet { .. } if origin == Origin::Render => {
            Access::Refused("an effect cannot write data while it renders; write it from a command, panel or task".into())
        }
        HostCall::DataGet { place: Place::Project | Place::Layer(_), .. } if origin == Origin::Render => {
            Access::Refused("an effect reads only its own layer's data (cx.layer_data), so its cached frames redraw when that data changes".into())
        }
        HostCall::DataGet { place: Place::OwnLayer, .. } | HostCall::DataSet { place: Place::OwnLayer, .. } if origin != Origin::Render => {
            Access::Refused("only a rendering effect has its own layer; name the layer with Place::Layer(id)".into())
        }
        HostCall::DataGet { .. } => Access::Needs(vec![Permission::DocumentRead]),
        HostCall::DataSet { .. } => Access::Needs(vec![Permission::DocumentWrite]),
        HostCall::PickFile { .. } if origin != Origin::Interactive => Access::Refused("a file can be picked only from a panel or a command".into()),
        HostCall::Form { .. } if origin != Origin::Interactive => Access::Refused("a form can open only from a panel or a command".into()),
        HostCall::PickFile { .. } | HostCall::Form { .. } => Access::Free,
        HostCall::ShowPanel { .. } | HostCall::ClosePanel { .. } | HostCall::CommandState { .. } if origin == Origin::Render => {
            Access::Refused("a panel can open or close only from a panel, a command, a tool or a task".into())
        }
        HostCall::ShowPanel { .. } | HostCall::ClosePanel { .. } | HostCall::CommandState { .. } => Access::Free,
        HostCall::ReadFile { root: Root::User, path, .. } | HostCall::ListFiles { root: Root::User, path } if plugin.picked(path, false) => {
            Access::Free
        }
        HostCall::WriteFile { root: Root::User, path } if plugin.picked(path, true) => Access::Free,
        HostCall::Log { .. }
        | HostCall::Notify { .. }
        | HostCall::Layer { .. }
        | HostCall::Source { .. }
        | HostCall::Masks
        | HostCall::Aux { .. }
        | HostCall::Scene
        | HostCall::SceneAt { .. }
        | HostCall::Placement { .. } => Access::Free,
        HostCall::CacheGet { .. }
        | HostCall::CachePut { .. }
        | HostCall::CacheRemove { .. }
        | HostCall::CacheHas { .. }
        | HostCall::UploadImage { .. } => Access::Free,
        HostCall::ParamsAt { .. } | HostCall::RemoveImage { .. } => Access::Free,
        HostCall::ClipboardRead | HostCall::ClipboardWrite { .. } => Access::Needs(vec![Permission::Clipboard]),
        HostCall::Command { id, params } => host.access(id, params),
        HostCall::ReadFile { root: Root::User, .. } | HostCall::ListFiles { root: Root::User, .. } => Access::Needs(vec![Permission::FilesRead]),
        HostCall::WriteFile { root: Root::User, .. } => Access::Needs(vec![Permission::FilesWrite]),
        HostCall::WriteFile { root: Root::Bundle, .. } => Access::Refused("the plugin folder is read-only".into()),
        HostCall::ReadFile { .. } | HostCall::ListFiles { .. } | HostCall::WriteFile { .. } => Access::Free,
    }
}

fn undeclared(call: &HostCall, spec: &EffectSpec) -> Option<String> {
    let other_time = match call {
        HostCall::Aux { channels, .. } => {
            let missing: Vec<String> =
                channels.iter().filter(|channel| !spec.reads_aux.contains(channel)).map(|channel| format!("Channel::{channel:?}")).collect();
            return (!missing.is_empty()).then(|| format!(".reads_aux([{}])", missing.join(", ")));
        }
        HostCall::SceneAt { .. } => true,
        HostCall::Placement { time, .. } => time.is_some(),
        _ => return None,
    };
    match (spec.reads_scene, spec.reads_other_frames || !other_time) {
        (false, _) => Some(if other_time { ".reads_scene().reads_other_frames()" } else { ".reads_scene()" }.into()),
        (true, false) => Some(".reads_other_frames()".into()),
        (true, true) => None,
    }
}

pub(crate) fn check(call: &HostCall, host: &dyn Host, plugin: &Identity, origin: Origin) -> Result<(), String> {
    if let Some(spec) = host.effect()
        && let Some(missing) = undeclared(call, spec)
    {
        return Err(format!(
            "refused: effect `{}` must declare `{missing}` in its EffectSpec to read this, so its cached frames redraw when it changes",
            spec.id
        ));
    }
    let grants = &plugin.grants;
    match access(call, host, plugin, origin) {
        Access::Free => Ok(()),
        Access::Refused(why) => Err(format!("refused: {why}")),
        Access::Needs(needed) => match needed.iter().find(|p| !grants.contains(p)) {
            None => Ok(()),
            Some(missing) => {
                Err(format!("permission denied: this plugin does not have `{missing}`; add \"{missing}\" to `permissions` in plugin.json"))
            }
        },
    }
}
