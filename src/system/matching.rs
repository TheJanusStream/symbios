use crate::core::SymbiosState;
use crate::system::{RuntimeRule, SystemError};
use crate::vm::VirtualMachine;

/// Scratch buffers for zero-allocation rule matching.
///
/// Reuse this struct across multiple `matches` calls to avoid
/// per-call allocations. Call `clear()` before each use.
#[derive(Debug, Default)]
pub struct MatchScratch {
    pub context_frame: Vec<f64>,
    pub left_indices: Vec<usize>,
    pub right_indices: Vec<usize>,
}

impl MatchScratch {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears all buffers while preserving capacity.
    #[inline]
    pub fn clear(&mut self) {
        self.context_frame.clear();
        self.left_indices.clear();
        self.right_indices.clear();
    }
}

pub fn matches(
    state: &SymbiosState,
    index: usize,
    rule: &RuntimeRule,
    ignore: &[u16],
    vm: &mut VirtualMachine,
    scratch: &mut MatchScratch,
) -> Result<bool, SystemError> {
    // Clear scratch buffers (preserves capacity)
    scratch.clear();

    let pred_view = state
        .get_view(index)
        .ok_or(SystemError::InvalidPredecessorParam)?;

    if pred_view.sym != rule.predecessor {
        return Ok(false);
    }

    if pred_view.params.len() != rule.expected_arities[0] {
        return Ok(false);
    }

    if !rule.left_context.is_empty()
        && !match_left(
            state,
            index,
            &rule.left_context,
            ignore,
            &mut scratch.left_indices,
        )
    {
        return Ok(false);
    }

    if !rule.right_context.is_empty()
        && !match_right(
            state,
            index,
            &rule.right_context,
            ignore,
            &mut scratch.right_indices,
        )
    {
        return Ok(false);
    }

    for (i, &ctx_idx) in scratch.left_indices.iter().enumerate() {
        let view = state
            .get_view(ctx_idx)
            .ok_or(SystemError::InvalidPredecessorParam)?;
        if view.params.len() != rule.expected_arities[1 + i] {
            return Ok(false);
        }
    }

    let right_offset = 1 + rule.left_context.len();
    for (i, &ctx_idx) in scratch.right_indices.iter().enumerate() {
        let view = state
            .get_view(ctx_idx)
            .ok_or(SystemError::InvalidPredecessorParam)?;
        if view.params.len() != rule.expected_arities[right_offset + i] {
            return Ok(false);
        }
    }

    if let Some(code) = &rule.condition {
        scratch.context_frame.extend_from_slice(pred_view.params);

        for &i in &scratch.left_indices {
            let ctx_view = state.get_view(i).ok_or(SystemError::StateCorruption(i))?;
            scratch.context_frame.extend_from_slice(ctx_view.params);
        }
        for &i in &scratch.right_indices {
            let ctx_view = state.get_view(i).ok_or(SystemError::StateCorruption(i))?;
            scratch.context_frame.extend_from_slice(ctx_view.params);
        }

        let res = vm
            .eval(code, &scratch.context_frame, pred_view.age)
            .map_err(SystemError::CompileError)?;

        if res == 0.0 {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Matches a left context pattern against the state, moving backwards from `start_index`.
///
/// # Symbol Processing Order
///
/// For each symbol encountered during the backward scan:
/// 1. **Attempt match** against the current pattern symbol
/// 2. **Skip ignored symbols** (from `#ignore` directive) — including brackets
/// 3. **Structural skipping** via topology links for non-ignored brackets
/// 4. **Mismatch** if none of the above apply
///
/// The ignore list is checked before topology links. This means `#ignore [ ]` correctly
/// disables branch-aware skipping and produces linear context matching.
pub fn match_left(
    state: &SymbiosState,
    start_index: usize,
    pattern: &[u16],
    ignore: &[u16],
    matched_indices: &mut Vec<usize>,
) -> bool {
    if start_index == 0 {
        return false;
    }
    let mut curr = (start_index - 1) as i64;
    let mut pat_idx = (pattern.len() - 1) as i64;

    while curr >= 0 {
        let view = match state.get_view(curr as usize) {
            Some(v) => v,
            None => return false, // Defensive: invalid index means no match
        };

        // 1. Attempt Match (Explicit context match takes priority)
        if view.sym == pattern[pat_idx as usize] {
            matched_indices.push(curr as usize);
            if pat_idx == 0 {
                matched_indices.reverse();
                return true;
            }
            pat_idx -= 1;
            curr -= 1;
            continue;
        }

        // 2. Skip ignored symbols (checked before topology so #ignore [ ] works)
        if ignore.contains(&view.sym) {
            curr -= 1;
            continue;
        }

        // 3. Structural Skipping (Topology Logic, only for non-ignored brackets)
        if let Some(skip_target) = view.skip_idx {
            if skip_target < curr as usize {
                // We hit a ']', signifying the end of a sibling branch.
                // Jump to the start of the branch '['.
                curr = skip_target as i64 - 1;
                continue;
            } else {
                // We hit a '[', signifying the start of the parent branch.
                // Transparently step over it.
                curr -= 1;
                continue;
            }
        }

        // 4. Mismatch
        return false;
    }
    false
}

/// Matches a right context pattern against the state, moving forward from `start_index`.
///
/// # Symbol Processing Order
///
/// For each symbol encountered during the forward scan:
/// 1. **Attempt match** against the current pattern symbol
/// 2. **Skip ignored symbols** (from `#ignore` directive) — including brackets
/// 3. **Structural skipping** via topology links for non-ignored brackets
/// 4. **Mismatch** if none of the above apply
///
/// The ignore list is checked before topology links. This means `#ignore [ ]` correctly
/// disables branch-aware skipping and produces linear context matching.
pub fn match_right(
    state: &SymbiosState,
    start_index: usize,
    pattern: &[u16],
    ignore: &[u16],
    matched_indices: &mut Vec<usize>,
) -> bool {
    let mut curr = start_index + 1;
    let mut pat_idx = 0;

    while curr < state.len() {
        let view = match state.get_view(curr) {
            Some(v) => v,
            None => return false,
        };

        // 1. Attempt Match
        if view.sym == pattern[pat_idx] {
            matched_indices.push(curr);
            pat_idx += 1;
            if pat_idx >= pattern.len() {
                return true;
            }
            curr += 1;
            continue;
        }

        // 2. Skip ignored symbols (checked before topology so #ignore [ ] works)
        if ignore.contains(&view.sym) {
            curr += 1;
            continue;
        }

        // 3. Structural Skipping (only for non-ignored brackets)
        if let Some(skip_target) = view.skip_idx {
            if skip_target > curr {
                // We hit a '[', signifying the start of a sibling branch.
                // Jump to the end of the branch ']'.
                curr = skip_target + 1;
                continue;
            } else {
                // We hit a ']', signifying the end of the parent branch.
                // Step over it to find the parent's right context.
                curr += 1;
                continue;
            }
        }

        // 4. Mismatch
        return false;
    }
    false
}
