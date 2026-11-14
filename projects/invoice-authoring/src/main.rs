//! Lab #25 source-equivalent invoice projection, not a framework or universal transaction.
mod bevy;
mod model;
mod runen;

use model::{Case, Projection, Source, initial, preflight};
use sha2::{Digest, Sha256};

const FIXTURE: &str = include_str!("../fixtures/cases.json");

trait NativeBackend {
    fn source(&self) -> Source;
    fn computed(&self) -> Result<Projection, model::Rejection>;
    fn commit(&mut self, previous: &Source, next: &Source);
    fn refresh(&mut self) -> Vec<&'static str>;
    fn published(&self) -> Projection;
    fn refreshes(&self) -> usize;
}

impl NativeBackend for bevy::Backend {
    fn source(&self) -> Source { self.source() }
    fn computed(&self) -> Result<Projection, model::Rejection> { self.computed() }
    fn commit(&mut self, before: &Source, next: &Source) { self.commit(before, next); }
    fn refresh(&mut self) -> Vec<&'static str> { self.refresh() }
    fn published(&self) -> Projection { self.published() }
    fn refreshes(&self) -> usize { self.refreshes() }
}

impl NativeBackend for runen::Backend {
    fn source(&self) -> Source { self.source() }
    fn computed(&self) -> Result<Projection, model::Rejection> { self.computed() }
    fn commit(&mut self, before: &Source, next: &Source) { self.commit(before, next); }
    fn refresh(&mut self) -> Vec<&'static str> { self.refresh() }
    fn published(&self) -> Projection { self.published() }
    fn refreshes(&self) -> usize { self.refreshes() }
}

fn check<B: NativeBackend>(mut backend: B, cases: &[Case], label: &str) {
    assert_eq!(backend.source(), initial(), "{label}: initial source");
    assert_eq!(backend.computed().unwrap(), backend.published(), "{label}: initial view");
    let mut previous_publication = backend.published();
    let mut expected_refreshes = 1;
    for case in cases {
        let before = backend.source();
        let proposal = preflight(&before, &case.edit);
        match proposal {
            Ok(next) => {
                assert!(case.accepted, "{label}: unexpectedly admitted {}", case.name);
                assert!(case.error.is_none(), "{label}: accepted case with error");
                backend.commit(&before, &next);
                assert_eq!(backend.source(), next, "{label}: committed source {}", case.name);
                let changes = backend.refresh();
                let expected: Vec<_> = case.changes.iter().map(String::as_str).collect();
                assert_eq!(changes, expected, "{label}: published changes {}", case.name);
                expected_refreshes += 1;
                previous_publication = backend.published();
            }
            Err(error) => {
                assert!(!case.accepted, "{label}: rejected {}", case.name);
                assert_eq!(case.error.as_deref(), Some(error.id()), "{label}: error {}", case.name);
                assert_eq!(backend.source(), before, "{label}: rejection mutated source");
            }
        }
        assert_eq!(backend.computed().unwrap(), case.expected, "{label}: direct {}", case.name);
        assert_eq!(backend.published(), case.expected, "{label}: retained {}", case.name);
        assert_eq!(backend.published(), previous_publication, "{label}: retained lifetime");
        assert_eq!(backend.refreshes(), expected_refreshes, "{label}: refresh count");
    }
}

fn qualify() -> usize {
    let cases: Vec<Case> = serde_json::from_str(FIXTURE).expect("explicit independent oracle");
    check(bevy::Backend::new(), &cases, "Bevy 0.20.1");
    check(runen::Backend::new(), &cases, "accepted RunenECS");
    cases.len()
}

fn main() {
    let count = qualify();
    println!("invoice_authoring=pass cases={count} fixture_sha256={:x}", Sha256::digest(FIXTURE.as_bytes()));
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_one_world_invoice_results_match_independent_cases() {
        assert_eq!(super::qualify(), 12);
    }
}
