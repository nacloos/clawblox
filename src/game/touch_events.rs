use std::collections::HashSet;

/// Pair-wise touch lifecycle for one tick.
#[derive(Debug, Default, Clone)]
pub struct TouchTransitions {
    pub began: Vec<(u64, u64)>,
    pub ended: Vec<(u64, u64)>,
}

/// Compute touch begin/end transitions from current and previous overlap sets.
pub fn compute_touch_transitions(
    current: &HashSet<(u64, u64)>,
    previous: &HashSet<(u64, u64)>,
) -> TouchTransitions {
    let began = current
        .iter()
        .filter(|pair| !previous.contains(pair))
        .copied()
        .collect();

    let ended = previous
        .iter()
        .filter(|pair| !current.contains(pair))
        .copied()
        .collect();

    TouchTransitions { began, ended }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_touch_transitions() {
        let previous = HashSet::from([(1, 2), (3, 4)]);
        let current = HashSet::from([(3, 4), (5, 6)]);

        let transitions = compute_touch_transitions(&current, &previous);
        assert_eq!(transitions.began, vec![(5, 6)]);
        assert_eq!(transitions.ended, vec![(1, 2)]);
    }
}

