//! Load `machines.json` + `patterns_regex.json` and compile regex patterns.

use crate::mhchem::error::{MhchemError, MhchemResult};
use crate::mhchem::json::Machines;
use fancy_regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

const MACHINES_JSON: &str = include_str!("data/machines.json");
const PATTERNS_JSON: &str = include_str!("data/patterns_regex.json");

#[derive(Debug)]
pub struct RegexPatterns {
    pub map: HashMap<String, Regex>,
}

#[derive(Debug)]
pub struct MhchemData {
    pub machines: Machines,
    pub regexes: RegexPatterns,
}

/// Compile regex patterns in parallel using std::thread
fn compile_regexes_parallel(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> MhchemResult<HashMap<String, Regex>> {
    // Collect all patterns into a Vec for parallel processing
    let patterns: Vec<(String, String)> = obj
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
        .collect();

    // Determine number of threads (use available parallelism or default to 4)
    let num_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let chunk_size = (patterns.len() + num_threads - 1) / num_threads;

    // Spawn threads to compile regexes in parallel
    let handles: Vec<_> = patterns
        .chunks(chunk_size)
        .map(|chunk| {
            let chunk = chunk.to_vec();
            std::thread::spawn(move || {
                let mut results = Vec::new();
                for (k, src) in chunk {
                    match Regex::new(&src) {
                        Ok(re) => results.push(Ok((k, re))),
                        Err(e) => results.push(Err(MhchemError::msg(format!(
                            "regex compile {:?}: {}",
                            k, e
                        )))),
                    }
                }
                results
            })
        })
        .collect();

    // Collect results from all threads
    let mut map = HashMap::new();
    for handle in handles {
        let results = handle
            .join()
            .map_err(|_| MhchemError::msg("thread panicked"))?;
        for result in results {
            let (k, re) = result?;
            map.insert(k, re);
        }
    }

    Ok(map)
}

impl MhchemData {
    pub fn load() -> MhchemResult<Self> {
        let machines: Machines =
            serde_json::from_str(MACHINES_JSON).map_err(|e| MhchemError::msg(e.to_string()))?;

        let v: serde_json::Value =
            serde_json::from_str(PATTERNS_JSON).map_err(|e| MhchemError::msg(e.to_string()))?;
        let obj = v
            .get("regex")
            .and_then(|x| x.as_object())
            .ok_or_else(|| MhchemError::msg("patterns_regex: missing regex"))?;

        let map = compile_regexes_parallel(obj)?;

        Ok(MhchemData {
            machines,
            regexes: RegexPatterns { map },
        })
    }
}

static MHCHEM_DATA: OnceLock<MhchemData> = OnceLock::new();

pub fn data() -> &'static MhchemData {
    MHCHEM_DATA
        .get_or_init(|| MhchemData::load().expect("mhchem static data must parse and compile"))
}
