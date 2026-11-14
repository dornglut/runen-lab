//! Independent, hand-authored outcomes live in fixtures/cases.json.
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Live,
    Stale,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct HitRequest {
    pub target: Target,
    pub damage: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "health_after", rename_all = "snake_case")]
pub enum Receipt {
    Applied(u32),
    Rejected,
}

#[derive(Debug, Deserialize)]
pub struct Case {
    pub tick: u32,
    pub hits: Vec<HitRequest>,
    pub expected: Expected,
}

#[derive(Debug, Deserialize)]
pub struct Expected {
    pub health: u32,
    pub receipts: Vec<Receipt>,
    pub deaths: Vec<u32>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub health: u32,
    pub receipts: Vec<Receipt>,
    pub deaths: Vec<u32>,
    pub second_consumer_deaths: Vec<u32>,
}

impl Snapshot {
    pub fn check(&self, case: &Case, backend: &str) {
        assert_eq!(
            self.health, case.expected.health,
            "{backend}: health at tick {}", case.tick
        );
        assert_eq!(
            self.receipts, case.expected.receipts,
            "{backend}: hit order at tick {}", case.tick
        );
        assert_eq!(
            self.deaths, case.expected.deaths,
            "{backend}: death delivery at tick {}", case.tick
        );
        assert_eq!(
            self.second_consumer_deaths, case.expected.deaths,
            "{backend}: second subscriber at tick {}", case.tick
        );
    }
}
