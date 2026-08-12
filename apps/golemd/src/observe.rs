//! The verdict a probe of the live host can report about one glyph, and the
//! total map keyed by glyph — never host state itself (ADR 0058). It answers
//! the same "does the host already realize this glyph" question every
//! `apply_*` in `reconcilers.rs` already asks itself before touching
//! anything; the comparison that produces the answer (resolving an owner
//! name, unsealing a secret) stays on the host side of the `Reconciler`
//! port, so only this four-valued verdict ever crosses it.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observation {
    Realized,
    Divergent,
    Absent,
    Unknown(Unknowable),
}

/// Why a glyph could not be settled to one of the three real verdicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unknowable {
    /// The keyring could not open this glyph's secret-bearing field.
    Sealed,
    /// The probe itself failed on the host — an I/O error or an unparseable
    /// command result — not an unopened secret.
    Unreadable,
    /// No probe ever ran against this key: a kind `observe` does not model,
    /// or a key [`Observations::record`] was never called with.
    NotModelled,
}

#[derive(Debug, Clone, Default)]
pub struct Observations(std::collections::BTreeMap<String, Observation>);

impl Observations {
    pub fn record(&mut self, key: String, observation: Observation) {
        self.0.insert(key, observation);
    }

    /// Total: a key nothing ever probed reads `Unknown(NotModelled)`, never
    /// `None`. A partial probe — a kind not yet modelled, a glyph the batch
    /// skipped — degrades to stated ignorance for that one row rather than a
    /// missing entry callers must special-case, or a panic.
    pub fn get(&self, key: &str) -> Observation {
        self.0
            .get(key)
            .copied()
            .unwrap_or(Observation::Unknown(Unknowable::NotModelled))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl FromIterator<(String, Observation)> for Observations {
    fn from_iter<T: IntoIterator<Item = (String, Observation)>>(iter: T) -> Self {
        Self(std::collections::BTreeMap::from_iter(iter))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unrecorded_key_reads_as_not_modelled() {
        let observations = Observations::default();
        assert_eq!(
            observations.get("apt:nginx"),
            Observation::Unknown(Unknowable::NotModelled)
        );
    }

    #[test]
    fn a_recorded_key_reads_back_verbatim() {
        let mut observations = Observations::default();
        observations.record("apt:nginx".to_string(), Observation::Realized);
        assert_eq!(observations.get("apt:nginx"), Observation::Realized);
    }

    #[test]
    fn recording_a_key_twice_keeps_the_last_verdict() {
        let mut observations = Observations::default();
        observations.record("file:/etc/motd".to_string(), Observation::Absent);
        observations.record("file:/etc/motd".to_string(), Observation::Divergent);
        assert_eq!(observations.get("file:/etc/motd"), Observation::Divergent);
    }

    #[test]
    fn a_default_observations_is_empty_and_a_recorded_one_is_not() {
        let mut observations = Observations::default();
        assert!(observations.is_empty());
        observations.record("apt:curl".to_string(), Observation::Realized);
        assert!(!observations.is_empty());
    }

    #[test]
    fn collecting_pairs_builds_the_same_map() {
        let observations: Observations = vec![
            ("apt:nginx".to_string(), Observation::Realized),
            ("apt:curl".to_string(), Observation::Absent),
        ]
        .into_iter()
        .collect();
        assert_eq!(observations.get("apt:nginx"), Observation::Realized);
        assert_eq!(observations.get("apt:curl"), Observation::Absent);
        assert_eq!(
            observations.get("apt:jq"),
            Observation::Unknown(Unknowable::NotModelled)
        );
    }
}
