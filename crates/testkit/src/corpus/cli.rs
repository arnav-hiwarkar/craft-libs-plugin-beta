use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use super::{APPEND_ONLY, Corpus};

pub const USAGE: &str = "usage: legacy-corpus [--check] [--root <dir>]";

#[macro_export]
macro_rules! legacy_paths {
    ($(#[$meta:meta])* $vis:vis enum $name:ident { $($variant:ident => $kind:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        $vis enum $name {
            $($variant),+
        }

        impl $name {
            pub const ALL: [$name; [$($kind),+].len()] = [$($name::$variant),+];

            pub const fn name(self) -> &'static str {
                match self {
                    $($name::$variant => $kind),+
                }
            }
        }
    };
}

pub fn run_cli<E: fmt::Display>(
    args: impl IntoIterator<Item = String>,
    default_root: &Path,
    generate: impl FnOnce() -> Result<Corpus, E>,
) -> ExitCode {
    match cli(args, default_root, generate, &mut std::io::stdout(), &mut std::io::stderr()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("legacy-corpus: {e}");
            ExitCode::from(2)
        }
    }
}

pub(crate) fn cli<E: fmt::Display>(
    args: impl IntoIterator<Item = String>,
    default_root: &Path,
    generate: impl FnOnce() -> Result<Corpus, E>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<bool, String> {
    let mut check = false;
    let mut root = default_root.to_owned();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--check" => check = true,
            "--root" => root = PathBuf::from(args.next().ok_or(USAGE)?),
            _ => return Err(USAGE.to_owned()),
        }
    }
    let corpus = generate().map_err(|e| e.to_string())?;
    let say = |w: &mut dyn Write, line: String| w.write_all(format!("{line}\n").as_bytes()).map_err(|e| e.to_string());
    if !check {
        corpus.write(&root).map_err(|e| e.to_string())?;
        say(out, format!("wrote {} members to {}", corpus.manifest().member.len(), root.display()))?;
        return Ok(true);
    }
    for entry in &corpus.manifest().member {
        say(out, format!("{:<26} {:<24} {} file(s)  {}", entry.name, entry.kind, entry.files.len(), entry.expect))?;
    }
    let drift = corpus.check(&root).map_err(|e| e.to_string())?;
    for d in &drift {
        say(err, d.to_string())?;
    }
    if drift.is_empty() {
        say(out, format!("{} matches the generator", root.display()))?;
    } else if drift.iter().any(super::Drift::breaks_append_only) {
        say(err, format!("{} drift(s): {APPEND_ONLY}", drift.len()))?;
    } else {
        say(err, format!("{} drift(s): regenerate to add the new members, then review the diff", drift.len()))?;
    }
    Ok(drift.is_empty())
}
