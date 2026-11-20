#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TestId(String);

impl TestId {
    pub fn parse(text: &str) -> Option<TestId> {
        is_test_id(text).then(|| TestId(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub(crate) struct Wildcard {
    stem: String,
}

impl Wildcard {
    fn parse(text: &str) -> Option<Wildcard> {
        let stem = text.strip_suffix('*')?;
        let scope = stem.strip_suffix('-')?;
        (is_namespace(scope) || is_test_id(scope)).then(|| Wildcard { stem: stem.to_owned() })
    }

    pub(crate) fn matches(&self, id: &TestId) -> bool {
        id.as_str().starts_with(&self.stem)
    }
}

pub(crate) enum Reference {
    Id(TestId),
    Wildcard(Wildcard),
}

impl Reference {
    pub(crate) fn parse(text: &str) -> Option<Reference> {
        TestId::parse(text).map(Reference::Id).or_else(|| Wildcard::parse(text).map(Reference::Wildcard))
    }
}

fn is_namespace(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase()) && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

fn is_word(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

fn is_test_id(text: &str) -> bool {
    let mut parts = text.split('-');
    let (Some(namespace), Some(first)) = (parts.next(), parts.next()) else {
        return false;
    };
    is_namespace(namespace) && first.starts_with(|c: char| c.is_ascii_lowercase()) && is_word(first) && parts.all(is_word)
}
