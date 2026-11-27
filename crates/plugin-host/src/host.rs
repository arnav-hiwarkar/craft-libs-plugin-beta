use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use craft_plugin::{
    AudioRequest, Channel, EffectSpec, Form, FrameRequest, LayerPlacement, MaskPath, ParamsAt, Permission, PickFile, Pixel, Place, Scene,
};
use serde_json::Value;

pub const MAX_FRAME_SCALE: f64 = 4.0;

pub fn check_frame(request: &FrameRequest) -> Result<(), String> {
    if !(request.scale > 0.0 && request.scale <= MAX_FRAME_SCALE) || !request.time.is_finite() {
        return Err(format!("a frame needs a scale above 0 and at most {MAX_FRAME_SCALE}, and a finite time"));
    }
    if request.source && request.layer.is_none() {
        return Err("a source frame names its layer".into());
    }
    let numeric = |name: &Option<String>| name.as_ref().is_none_or(|name| name.parse::<u64>().is_ok());
    if request.ids && !(numeric(&request.comp) && numeric(&request.layer)) {
        return Err("a frame asked for by ids names its composition and layer by number".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Interactive,
    Render,
    Task,
}

pub struct Samples {
    pub channels: u32,
    pub rate: u32,
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Access {
    Free,
    Needs(Vec<Permission>),
    Refused(String),
}

pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub data: Vec<Pixel>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AuxPlanes {
    pub width: u32,
    pub height: u32,
    pub planes: Vec<(Channel, Vec<f32>)>,
}

pub trait Host {
    fn effect(&self) -> Option<&EffectSpec> {
        None
    }

    fn access(&self, _command: &str, _params: &Value) -> Access {
        Access::Refused("commands are not available here".into())
    }

    fn command(&mut self, _id: &str, _params: Value) -> Result<Value, String> {
        Err("commands are not available here".into())
    }

    fn notify(&mut self, _message: &str) {}

    fn layer(&mut self, _param: &str, _comp_time: Option<f64>) -> Option<Pixels> {
        None
    }

    fn source(&mut self, _layer_time: f64) -> Option<Pixels> {
        None
    }

    fn masks(&mut self) -> Vec<MaskPath> {
        Vec::new()
    }

    fn aux(&mut self, _channels: &[Channel], _rows: Option<[u32; 2]>, _size: Option<[u32; 2]>) -> Result<Option<AuxPlanes>, String> {
        Ok(None)
    }

    fn scene(&mut self) -> Option<Scene> {
        None
    }

    fn scene_at(&mut self, _comp_time: f64) -> Option<Scene> {
        None
    }

    fn placement(&mut self, _param: &str, _comp_time: Option<f64>) -> Option<LayerPlacement> {
        None
    }

    fn frame(&mut self, _request: &FrameRequest) -> Result<Pixels, String> {
        Err("frames are not available here".into())
    }

    fn audio(&mut self, _request: &AudioRequest) -> Result<Samples, String> {
        Err("audio is not available here".into())
    }

    fn pick_file(&mut self, _request: &PickFile) -> Result<Option<PathBuf>, String> {
        Err("a file dialog is not available here".into())
    }

    fn form(&mut self, _form: &Form) -> Result<Option<std::collections::BTreeMap<String, craft_plugin::Value>>, String> {
        Err("a form cannot open here".into())
    }

    fn command_state(&mut self, _command: &str, _enabled: bool, _checked: Option<bool>) -> Result<(), String> {
        Err("this app does not show plugin command state".into())
    }

    fn panel(&mut self, _panel: &str, _instance: Option<&str>, _open: bool) -> Result<(), String> {
        Err("this app cannot open or close plugin panels".into())
    }

    fn params_at(&mut self, _layer_time: f64) -> Option<ParamsAt> {
        None
    }

    fn project_closed(&self) -> Option<Arc<AtomicBool>> {
        None
    }

    fn cancel(&self) -> Option<Arc<AtomicBool>> {
        None
    }

    fn finish(&mut self, _written: &mut [Pixel]) {}

    fn clipboard(&mut self) -> Result<String, String> {
        Err("the clipboard is not available here".into())
    }

    fn set_clipboard(&mut self, _text: &str) -> Result<(), String> {
        Err("the clipboard is not available here".into())
    }

    fn data(&mut self, _plugin: &str, _place: Place, _key: &str) -> Result<Option<Vec<u8>>, String> {
        Err("this app keeps no plugin data here".into())
    }

    fn set_data(&mut self, _plugin: &str, _place: Place, _key: &str, _value: Option<Vec<u8>>) -> Result<(), String> {
        Err("this app keeps no plugin data here".into())
    }
}
