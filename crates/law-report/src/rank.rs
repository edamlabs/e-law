/// The order a tag's values are shown in: by number when every one is a number, else by text.
pub(crate) struct Rank;

impl Rank {
    /// The values sorted: numerically when every one parses as a number, lexically otherwise.
    pub(crate) fn sort(values: &mut [String]) {
        let numbers: Option<Vec<u64>> = values.iter().map(|value| value.parse().ok()).collect();
        match numbers {
            Some(_) => values.sort_by_key(|value| value.parse::<u64>().unwrap_or_default()),
            None => values.sort(),
        }
    }
}
