use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use craft_plugin::{Form, Value};

use super::remote::lock;

pub type Values = BTreeMap<String, Value>;

pub struct AskedForm {
    pub plugin: String,
    pub form: Form,
    pub values: Values,
}

struct Pending {
    id: u64,
    project: Arc<AtomicBool>,
    asked: AskedForm,
    reply: Sender<Option<Values>>,
}

static PENDING: Mutex<Vec<Pending>> = Mutex::new(Vec::new());
static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) fn ask(plugin: &str, project: Arc<AtomicBool>, form: &Form, reply: Sender<Option<Values>>) -> u64 {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let asked = AskedForm { plugin: plugin.to_string(), values: form.defaults(), form: form.clone() };
    lock(&PENDING).push(Pending { id, project, asked, reply });
    id
}

pub(super) fn withdraw(id: u64) {
    lock(&PENDING).retain(|form| form.id != id);
}

pub fn asked_form<T>(project: &Arc<AtomicBool>, look: impl FnOnce(&mut AskedForm) -> T) -> Option<T> {
    lock(&PENDING).iter_mut().find(|form| Arc::ptr_eq(&form.project, project)).map(|form| look(&mut form.asked))
}

pub fn answer_form(project: &Arc<AtomicBool>, values: Option<Values>) -> Result<(), String> {
    let mut forms = lock(&PENDING);
    let at = forms.iter().position(|form| Arc::ptr_eq(&form.project, project)).ok_or("no plug-in form is open")?;
    let answer = match values {
        Some(values) => {
            let asked = &forms[at].asked;
            Some(asked.form.answer_over(&asked.values, values)?)
        }
        None => None,
    };
    let _ = forms.remove(at).reply.send(answer);
    Ok(())
}
