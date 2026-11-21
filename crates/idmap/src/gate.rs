use std::str::FromStr;

use crate::check::{Registered, check, update};
use crate::lock::Lock;
use crate::name::Name;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    Rename { from: Name, to: Name },
    Retire(Name),
    Migrate { id: Name, migration: Name },
}

impl FromStr for Edit {
    type Err = String;

    fn from_str(text: &str) -> Result<Edit, String> {
        let name = |s: &str| Name::new(s).map_err(|e| format!("`{text}`: {e}"));
        fn pair<'a>(text: &str, rest: &'a str) -> Result<(&'a str, &'a str), String> {
            rest.split_once('=').ok_or_else(|| format!("`{text}` needs the form ID=VALUE"))
        }
        match text.trim().split_once(' ') {
            Some(("rename", rest)) => pair(text, rest).and_then(|(from, to)| Ok(Edit::Rename { from: name(from)?, to: name(to)? })),
            Some(("retire", rest)) => Ok(Edit::Retire(name(rest)?)),
            Some(("migrate", rest)) => pair(text, rest).and_then(|(id, m)| Ok(Edit::Migrate { id: name(id)?, migration: name(m)? })),
            _ => Err(format!("`{text}` is not `rename OLD=NEW`, `retire ID` or `migrate ID=NAME`")),
        }
    }
}

pub fn verify(text: &str, registry: &[Registered], migrations: &[&str]) -> Result<(), Vec<String>> {
    let lock = Lock::parse(text).map_err(|e| vec![e.to_string()])?;
    let mut problems: Vec<String> = check(&lock, registry, migrations).iter().map(ToString::to_string).collect();
    if problems.is_empty() && lock.to_text() != text {
        problems.push("the lock is not in the form `engine-lock --update` writes; run it".to_owned());
    }
    if problems.is_empty() { Ok(()) } else { Err(problems) }
}

pub fn regenerate(text: &str, registry: &[Registered], migrations: &[&str], edits: &[Edit]) -> Result<String, Vec<String>> {
    let mut lock = Lock::parse(text).map_err(|e| vec![e.to_string()])?;
    for edit in edits {
        let applied = match edit {
            Edit::Rename { from, to } => lock.rename(from, to.clone()),
            Edit::Retire(id) => lock.retire(id),
            Edit::Migrate { id, migration } => lock.migrate(id, migration.clone()),
        };
        applied.map_err(|e| vec![e.to_string()])?;
    }
    update(&lock, registry, migrations).map(|next| next.to_text()).map_err(|findings| findings.iter().map(ToString::to_string).collect())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Check,
    Update(Vec<Edit>),
}

impl Request {
    pub fn from_args(args: &[&str]) -> Result<Request, String> {
        let mut update = false;
        let mut edits = Vec::new();
        let mut rest = args.iter().map(|a| a.trim()).filter(|a| !a.is_empty());
        while let Some(arg) = rest.next() {
            match arg {
                "--check" => {}
                "--update" => update = true,
                "--rename" | "--retire" | "--migrate" => {
                    let value = rest.next().ok_or_else(|| format!("{arg} needs a value"))?;
                    edits.push(format!("{} {value}", arg.trim_start_matches('-')).parse()?);
                }
                other => {
                    return Err(format!(
                        "unknown argument `{other}`; use --check, or --update with --rename OLD=NEW, --retire ID or --migrate ID=NAME"
                    ));
                }
            }
        }
        match (update, edits.is_empty()) {
            (true, _) => Ok(Request::Update(edits)),
            (false, true) => Ok(Request::Check),
            (false, false) => Err("--rename, --retire and --migrate edit the lock: add --update".into()),
        }
    }

    pub fn run(&self, text: &str, registry: &[Registered], migrations: &[&str]) -> Result<Option<String>, Vec<String>> {
        match self {
            Request::Check => verify(text, registry, migrations).map(|()| None),
            Request::Update(edits) => regenerate(text, registry, migrations, edits).map(Some),
        }
    }
}
