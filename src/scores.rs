use std::{collections::BTreeMap, fs};

use serde_json::{Map, Value, json};

const FILE: &str = "scores.json";

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Best {
    pub score: u64,
    pub accuracy: f64,
    pub max_combo: u32,
}

#[derive(Default)]
pub struct Scores {
    bests: BTreeMap<String, Best>,
}

fn parse(text: &str) -> Option<BTreeMap<String, Best>> {
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else {
        return None;
    };
    Some(
        map.into_iter()
            .filter_map(|(key, v)| {
                Some((
                    key,
                    Best {
                        score: v.get("score")?.as_u64()?,
                        accuracy: v.get("accuracy")?.as_f64()?,
                        max_combo: v.get("max_combo")?.as_u64()? as u32,
                    },
                ))
            })
            .collect(),
    )
}

fn render(bests: &BTreeMap<String, Best>) -> String {
    let map: Map<String, Value> = bests
        .iter()
        .map(|(key, b)| {
            (
                key.clone(),
                json!({ "score": b.score, "accuracy": b.accuracy, "max_combo": b.max_combo }),
            )
        })
        .collect();
    serde_json::to_string_pretty(&Value::Object(map)).unwrap_or_default()
}

impl Scores {
    pub fn load() -> Self {
        let Ok(text) = fs::read_to_string(FILE) else {
            return Self::default();
        };
        let Some(bests) = parse(&text) else {
            let kept = format!("{FILE}.bad");
            match fs::rename(FILE, &kept) {
                Ok(()) => eprintln!("{FILE}: not valid, kept as {kept}"),
                Err(e) => eprintln!("{FILE}: not valid and could not be moved: {e}"),
            }
            return Self::default();
        };
        Self { bests }
    }

    pub fn best(&self, key: &str) -> Option<Best> {
        self.bests.get(key).copied()
    }

    pub fn record(&mut self, key: &str, best: Best) -> bool {
        if self
            .bests
            .get(key)
            .is_some_and(|old| old.score >= best.score)
        {
            return false;
        }
        self.bests.insert(key.into(), best);
        true
    }

    pub fn save(&self) {
        if let Err(e) = crate::store::write_atomic(FILE, &render(&self.bests)) {
            eprintln!("{FILE}: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn best(score: u64) -> Best {
        Best {
            score,
            accuracy: 97.5,
            max_combo: 40,
        }
    }

    #[test]
    fn only_a_higher_score_replaces_the_best() {
        let mut s = Scores::default();
        assert!(s.record("a.sspm", best(100)));
        assert!(!s.record("a.sspm", best(100)));
        assert!(!s.record("a.sspm", best(50)));
        assert!(s.record("a.sspm", best(101)));
        assert!(s.record("b.sspm", best(1)));
        assert_eq!(s.best("a.sspm").map(|b| b.score), Some(101));
    }

    #[test]
    fn survives_a_round_trip_and_ignores_junk() {
        let mut s = Scores::default();
        s.record("a.sspm", best(7));
        let again = parse(&render(&s.bests)).unwrap();
        assert_eq!(again.get("a.sspm"), Some(&best(7)));
        assert!(parse("not json").is_none());
        assert!(parse(r#"{"x": {"score": "bad"}}"#).unwrap().is_empty());
    }
}
