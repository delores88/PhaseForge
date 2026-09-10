use rand::{Rng, SeedableRng, seq::SliceRandom};
use rand_chacha::ChaCha20Rng;
use rand_distr::{Distribution, Normal};
use serde::{Deserialize, Serialize};

use crate::domain::{SearchAlgorithm, SearchSpec};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub values: Vec<f64>,
    #[serde(with = "finite_score")]
    pub score: f64,
}

// JSON has no infinity. Unevaluated/invalid scores round trip as null, never zero.
mod finite_score {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(*value)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
        Ok(Option::<f64>::deserialize(deserializer)?.unwrap_or(f64::INFINITY))
    }
}

impl Candidate {
    pub fn unevaluated(values: Vec<f64>) -> Self {
        Self {
            values,
            score: f64::INFINITY,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchGeneration {
    pub generation: usize,
    #[serde(with = "finite_score")]
    pub best_score: f64,
    #[serde(with = "finite_score")]
    pub median_score: f64,
    pub finite_candidates: usize,
    pub population: usize,
}

pub struct PopulationEngine {
    random: ChaCha20Rng,
    targets: Vec<Candidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PopulationState {
    seed: [u8; 32],
    stream: u64,
    word_position: String,
    targets: Vec<Candidate>,
}

impl PopulationEngine {
    pub(crate) fn checkpoint(&self) -> PopulationState {
        PopulationState {
            seed: self.random.get_seed(),
            stream: self.random.get_stream(),
            word_position: self.random.get_word_pos().to_string(),
            targets: self.targets.clone(),
        }
    }

    pub(crate) fn restore(state: PopulationState) -> anyhow::Result<Self> {
        let mut random = ChaCha20Rng::from_seed(state.seed);
        random.set_stream(state.stream);
        random.set_word_pos(state.word_position.parse()?);
        Ok(Self { random, targets: state.targets })
    }
    pub fn new(seed: u64) -> Self {
        Self {
            random: ChaCha20Rng::seed_from_u64(seed),
            targets: Vec::new(),
        }
    }

    pub fn initial(&mut self, search: &SearchSpec, population: usize) -> Vec<Candidate> {
        if matches!(search.algorithm, SearchAlgorithm::LatinHypercube | SearchAlgorithm::DifferentialEvolution) {
            let mut rows = vec![Candidate::unevaluated(vec![0.0; search.variables.len()]); population];
            if population == 0 { return rows; }
            for (j, variable) in search.variables.iter().enumerate() {
                let mut strata = (0..population).collect::<Vec<_>>();
                strata.shuffle(&mut self.random);
                for (i, stratum) in strata.into_iter().enumerate() {
                    let q = (stratum as f64 + self.random.gen::<f64>()) / population as f64;
                    rows[i].values[j] = variable.minimum + q * (variable.maximum - variable.minimum);
                }
            }
            return rows;
        }
        (0..population)
            .map(|_| {
                Candidate::unevaluated(
                    search
                        .variables
                        .iter()
                        .map(|variable| {
                            self.random
                                .gen_range(variable.minimum..=variable.maximum)
                        })
                        .collect(),
                )
            })
            .collect()
    }

    /// Called before sorting so each DE trial is compared to its actual target.
    pub fn select(&mut self, search: &SearchSpec, evaluated: &mut [Candidate]) {
        if search.algorithm == SearchAlgorithm::DifferentialEvolution && self.targets.len() == evaluated.len() {
            for (trial, target) in evaluated.iter_mut().zip(&self.targets) {
                if !trial.score.is_finite() || (target.score.is_finite() && target.score < trial.score) {
                    *trial = target.clone();
                }
            }
            self.targets.clear();
        }
    }

    pub fn next(
        &mut self,
        search: &SearchSpec,
        evaluated: &[Candidate],
        population: usize,
    ) -> Vec<Candidate> {
        if search.algorithm == SearchAlgorithm::DifferentialEvolution && evaluated.len() >= 4 {
            self.targets = evaluated.to_vec();
            let dimensions = search.variables.len();
            if dimensions == 0 { return self.initial(search, population); }
            return evaluated.iter().enumerate().map(|(index, target)| {
                let mut others = (0..evaluated.len()).filter(|i| *i != index).collect::<Vec<_>>();
                others.shuffle(&mut self.random);
                let forced = self.random.gen_range(0..dimensions);
                let values = search.variables.iter().enumerate().map(|(j, bound)| {
                    if j == forced || self.random.gen::<f64>() < 0.9 {
                        (evaluated[others[0]].values[j] + 0.7 *
                            (evaluated[others[1]].values[j] - evaluated[others[2]].values[j]))
                            .clamp(bound.minimum, bound.maximum)
                    } else { target.values[j] }
                }).collect();
                Candidate::unevaluated(values)
            }).collect();
        }
        if matches!(search.algorithm, SearchAlgorithm::Random | SearchAlgorithm::LatinHypercube) || evaluated.is_empty() {
            return self.initial(search, population);
        }

        let elite_count = ((evaluated.len() as f64 * search.elite_fraction).ceil() as usize)
            .clamp(1, evaluated.len());
        let mut next = evaluated[..elite_count]
            .iter()
            .cloned()
            .map(|mut candidate| {
                candidate.score = f64::INFINITY;
                candidate
            })
            .collect::<Vec<_>>();

        while next.len() < population {
            let parent = &evaluated[self.random.gen_range(0..elite_count)];
            let values = parent
                .values
                .iter()
                .zip(&search.variables)
                .map(|(value, variable)| {
                    let span = variable.maximum - variable.minimum;
                    let sigma = (span * search.mutation_scale).max(f64::EPSILON);
                    let noise = Normal::new(0.0, sigma)
                        .map(|distribution| distribution.sample(&mut self.random))
                        .unwrap_or(0.0);
                    (value + noise).clamp(variable.minimum, variable.maximum)
                })
                .collect();
            next.push(Candidate::unevaluated(values));
        }
        next
    }
}

pub fn sort_candidates(candidates: &mut [Candidate]) {
    candidates.sort_by(|left, right| {
        let left = if left.score.is_finite() {
            left.score
        } else {
            f64::INFINITY
        };
        let right = if right.score.is_finite() {
            right.score
        } else {
            f64::INFINITY
        };
        left.total_cmp(&right)
    });
}

pub fn generation_summary(generation: usize, candidates: &[Candidate]) -> SearchGeneration {
    let mut finite = candidates
        .iter()
        .map(|candidate| candidate.score)
        .filter(|score| score.is_finite())
        .collect::<Vec<_>>();
    finite.sort_by(f64::total_cmp);
    let best_score = finite.first().copied().unwrap_or(f64::INFINITY);
    let median_score = finite
        .get(finite.len().saturating_sub(1) / 2)
        .copied()
        .unwrap_or(f64::INFINITY);
    SearchGeneration {
        generation,
        best_score,
        median_score,
        finite_candidates: finite.len(),
        population: candidates.len(),
    }
}

#[cfg(test)]
mod discovery_tests {
    use super::*;
    use crate::domain::SearchVariableSpec;
    fn spec(algorithm: SearchAlgorithm) -> SearchSpec {
        SearchSpec { algorithm, enabled: true, variables: vec![SearchVariableSpec {
            name: "x".into(), target: "initial:x".into(), minimum: -1.0, maximum: 1.0,
        }], ..SearchSpec::default() }
    }
    #[test]
    fn lhs_hits_every_stratum_and_reproduces() {
        let s = spec(SearchAlgorithm::LatinHypercube);
        let a = PopulationEngine::new(7).initial(&s, 16);
        let b = PopulationEngine::new(7).initial(&s, 16);
        let mut bins = a.iter().map(|c| ((c.values[0]+1.0)/2.0*16.0).floor() as usize).collect::<Vec<_>>();
        bins.sort_unstable(); assert_eq!(bins, (0..16).collect::<Vec<_>>());
        assert_eq!(a.iter().map(|c|c.values.clone()).collect::<Vec<_>>(), b.iter().map(|c|c.values.clone()).collect::<Vec<_>>());
    }
    #[test]
    fn de_never_replaces_finite_target_with_worse_trial() {
        let s = spec(SearchAlgorithm::DifferentialEvolution); let mut e = PopulationEngine::new(42);
        let mut parents = e.initial(&s, 8); for c in &mut parents { c.score=c.values[0]*c.values[0]; }
        sort_candidates(&mut parents); let mut trials=e.next(&s,&parents,8);
        assert_eq!(trials.len(),8); assert!(trials.iter().all(|c|(-1.0..=1.0).contains(&c.values[0])));
        for c in &mut trials { c.score=999.0; } e.select(&s,&mut trials);
        assert_eq!(parents.iter().map(|c|c.score).collect::<Vec<_>>(),trials.iter().map(|c|c.score).collect::<Vec<_>>());
    }
}
